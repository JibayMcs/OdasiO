//! Chargement en continu autour de la caméra : chunks détaillés (lus et maillés en tâche de
//! fond) et relief lointain par région, au niveau de détail adapté à la distance.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};

use eframe::egui_wgpu::RenderState;
use odasio_core::Mapping;
use odasio_render::BlockLook;
use odasio_render::mesh::{Atlas, ChunkMesh, MeshData, mesh_chunk, mesh_far};
use odasio_render::topmap::TopMap;
use odasio_render::voxel::{ChunkBlocks, ChunkSource, Registry};

use super::Assets;
use super::gpu::{ChunkGpu, Gpu, GpuMesh};

pub const MC: usize = 0;
pub const VS: usize = 1;
/// Travaux en cours au plus (chargements, maillages).
const MAX_LOADS: usize = 48;
const MAX_MESHES: usize = 24;
const MAX_FAR: usize = 8;
pub const ATLAS_SIZE: u32 = 2048;

/// Aspect de chaque état du registre, pour un des deux mondes ; complété à la demande.
pub struct LookTable {
    assets: Arc<Assets>,
    /// Correspondances (côté Vintage Story) ; `None` côté Minecraft.
    mapping: Option<Arc<Mapping>>,
    looks: RwLock<Vec<Option<Arc<BlockLook>>>>,
}

impl LookTable {
    pub fn new(assets: Arc<Assets>, mapping: Option<Arc<Mapping>>) -> Self {
        Self {
            assets,
            mapping,
            looks: RwLock::new(Vec::new()),
        }
    }

    pub fn assets(&self) -> Arc<Assets> {
        self.assets.clone()
    }

    /// Aspects de tous les états connus du registre.
    pub fn snapshot(&self, registry: &RwLock<Registry>) -> Arc<Vec<Option<Arc<BlockLook>>>> {
        let registry = registry.read().unwrap();
        let known = self.looks.read().unwrap().len();
        if known < registry.len() {
            let mut looks = self.looks.write().unwrap();
            for id in looks.len()..registry.len() {
                let look = registry.get(id as u16).and_then(|s| self.look(s));
                looks.push(look.map(Arc::new));
            }
        }
        Arc::new(self.looks.read().unwrap().clone())
    }

    pub fn look(&self, state: &odasio_core::mc::BlockState) -> Option<BlockLook> {
        if state.is_air() {
            return None;
        }
        match &self.mapping {
            None => self.assets.mc.as_ref()?.look(state),
            Some(mapping) => {
                let vs = self.assets.vs.as_ref()?;
                let resolved = mapping.resolve(state);
                match &resolved.fluid {
                    Some(fluid) if resolved.block == "game:air" => vs.look(fluid),
                    _ => vs.look(&resolved.block),
                }
            }
        }
    }
}

enum Msg {
    Loaded((i32, i32), Option<Arc<ChunkBlocks>>),
    Meshed {
        side: usize,
        pos: (i32, i32),
        rev: u64,
        mesh: ChunkMesh,
    },
    Far {
        side: usize,
        key: (i32, i32),
        step: usize,
        rev: u64,
        mesh: MeshData,
    },
}

pub struct Streamer {
    pub source: Arc<ChunkSource>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Chunks connus : `None` = absent du monde.
    pub chunks: HashMap<(i32, i32), Option<Arc<ChunkBlocks>>>,
    loading: HashSet<(i32, i32)>,
    meshing: HashMap<(usize, (i32, i32)), u64>,
    meshed: HashMap<(usize, (i32, i32)), u64>,
    far_pending: HashSet<(usize, (i32, i32))>,
    far_done: HashMap<(usize, (i32, i32)), (usize, u64)>,
    pub tables: [Arc<LookTable>; 2],
    pub atlases: [Arc<Mutex<Atlas>>; 2],
    /// Couleurs de carte par état de la carte du dessus, et leur version.
    pub far_colors: [Arc<Vec<[u8; 3]>>; 2],
    pub revs: [u64; 2],
    /// Zone détaillée entièrement prête (x0, z0, x1, z1), en blocs.
    pub near_rect: [f32; 4],
}

impl Streamer {
    pub fn new(source: ChunkSource, assets: Arc<Assets>, mapping: Arc<Mapping>) -> Self {
        let (tx, rx) = channel();
        Self {
            source: Arc::new(source),
            tx,
            rx,
            chunks: HashMap::new(),
            loading: HashSet::new(),
            meshing: HashMap::new(),
            meshed: HashMap::new(),
            far_pending: HashSet::new(),
            far_done: HashMap::new(),
            tables: [
                Arc::new(LookTable::new(assets.clone(), None)),
                Arc::new(LookTable::new(assets, Some(mapping))),
            ],
            atlases: [
                Arc::new(Mutex::new(Atlas::new(ATLAS_SIZE))),
                Arc::new(Mutex::new(Atlas::new(ATLAS_SIZE))),
            ],
            far_colors: [Arc::new(Vec::new()), Arc::new(Vec::new())],
            revs: [0, 0],
            near_rect: [0.0, 0.0, -1.0, -1.0],
        }
    }

    /// Nouvelles correspondances : tout le côté Vintage Story sera refait.
    pub fn set_mapping(&mut self, mapping: Arc<Mapping>, rev: u64, far_colors: Vec<[u8; 3]>) {
        let assets = self.tables[VS].assets.clone();
        self.tables[VS] = Arc::new(LookTable::new(assets, Some(mapping)));
        self.revs[VS] = rev;
        self.far_colors[VS] = Arc::new(far_colors);
    }

    /// Travaux en cours (pour continuer à rafraîchir l'affichage).
    pub fn busy(&self) -> bool {
        !self.loading.is_empty() || !self.meshing.is_empty() || !self.far_pending.is_empty()
    }

    /// Reçoit les résultats, décharge ce qui est loin, lance les travaux suivants.
    /// `center` : chunk sous la caméra ; `radius` : rayon détaillé en chunks (0 = aucun).
    /// `far` : pas voulu pour chaque région de la carte.
    pub fn update(
        &mut self,
        rs: &RenderState,
        gpu: &mut Gpu,
        map: &Arc<TopMap>,
        center: (i32, i32),
        radius: i32,
        far: &[((i32, i32), usize)],
    ) {
        let in_range = |p: (i32, i32), r: i32| {
            (p.0 - center.0).abs() <= r && (p.1 - center.1).abs() <= r && radius > 0
        };
        // Résultats.
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Loaded(pos, chunk) => {
                    self.loading.remove(&pos);
                    self.chunks.insert(pos, chunk);
                }
                Msg::Meshed {
                    side,
                    pos,
                    rev,
                    mesh,
                } => {
                    if self.meshing.get(&(side, pos)) == Some(&rev) {
                        self.meshing.remove(&(side, pos));
                    }
                    if rev != self.revs[side] || !self.chunks.contains_key(&pos) {
                        continue;
                    }
                    gpu.sides[side].chunks.insert(
                        pos,
                        ChunkGpu {
                            opaque: GpuMesh::new(&rs.device, &mesh.opaque),
                            translucent: GpuMesh::new(&rs.device, &mesh.translucent),
                        },
                    );
                    self.meshed.insert((side, pos), rev);
                }
                Msg::Far {
                    side,
                    key,
                    step,
                    rev,
                    mesh,
                } => {
                    self.far_pending.remove(&(side, key));
                    if rev != self.revs[side] {
                        continue;
                    }
                    gpu.sides[side]
                        .far
                        .insert(key, (step, GpuMesh::new(&rs.device, &mesh)));
                    self.far_done.insert((side, key), (step, rev));
                }
            }
        }

        // Déchargement : un peu au-delà du rayon, pour ne pas recharger au moindre geste.
        let keep = radius + 3;
        let gone: Vec<(i32, i32)> = self
            .chunks
            .keys()
            .filter(|&&p| !in_range(p, keep))
            .copied()
            .collect();
        for pos in gone {
            self.chunks.remove(&pos);
            for side in [MC, VS] {
                gpu.sides[side].chunks.remove(&pos);
                self.meshed.remove(&(side, pos));
            }
        }

        // Chunks voulus, du plus proche au plus lointain.
        let mut wanted: Vec<(i32, i32)> = Vec::new();
        if radius > 0 {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    wanted.push((center.0 + dx, center.1 + dz));
                }
            }
            wanted.sort_by_key(|p| (p.0 - center.0).pow(2) + (p.1 - center.1).pow(2));
        }
        for &pos in &wanted {
            if self.loading.len() >= MAX_LOADS {
                break;
            }
            if self.chunks.contains_key(&pos) || self.loading.contains(&pos) {
                continue;
            }
            // Région absente de la carte : rien à lire.
            if !map
                .regions
                .contains_key(&(pos.0.div_euclid(32), pos.1.div_euclid(32)))
            {
                self.chunks.insert(pos, None);
                continue;
            }
            self.loading.insert(pos);
            let (source, tx) = (self.source.clone(), self.tx.clone());
            rayon::spawn(move || {
                let chunk = source.load(pos.0, pos.1).map(Arc::new);
                let _ = tx.send(Msg::Loaded(pos, chunk));
            });
        }

        // Maillages : quand le chunk et ses voisins (voulus) sont connus.
        for &pos in &wanted {
            if self.meshing.len() >= MAX_MESHES {
                break;
            }
            let Some(Some(chunk)) = self.chunks.get(&pos) else {
                continue;
            };
            let neighbor_pos = [
                (pos.0, pos.1 + 1),
                (pos.0, pos.1 - 1),
                (pos.0 + 1, pos.1),
                (pos.0 - 1, pos.1),
            ];
            if neighbor_pos
                .iter()
                .any(|n| in_range(*n, radius) && !self.chunks.contains_key(n))
            {
                continue;
            }
            let neighbors = neighbor_pos.map(|n| self.chunks.get(&n).cloned().flatten());
            for side in [MC, VS] {
                let rev = self.revs[side];
                if self.meshed.get(&(side, pos)) == Some(&rev)
                    || self.meshing.get(&(side, pos)) == Some(&rev)
                {
                    continue;
                }
                self.meshing.insert((side, pos), rev);
                let (chunk, neighbors, tx) = (chunk.clone(), neighbors.clone(), self.tx.clone());
                let (table, atlas, source) = (
                    self.tables[side].clone(),
                    self.atlases[side].clone(),
                    self.source.clone(),
                );
                rayon::spawn(move || {
                    // Côté Vintage Story, les chunks incomplets sont ignorés à la conversion.
                    let mesh = if side == VS && !chunk.complete {
                        ChunkMesh::default()
                    } else {
                        let looks = table.snapshot(&source.registry);
                        let n = neighbors.each_ref().map(|n| n.as_deref());
                        mesh_chunk(&chunk, n, &looks, &atlas)
                    };
                    let _ = tx.send(Msg::Meshed {
                        side,
                        pos,
                        rev,
                        mesh,
                    });
                });
            }
        }

        // Zone détaillée prête : le plus grand carré centré dont tout est maillé.
        let ready = |p: (i32, i32)| match self.chunks.get(&p) {
            Some(None) => true,
            Some(Some(_)) => [MC, VS].iter().all(|&s| self.meshed.contains_key(&(s, p))),
            None => false,
        };
        let mut r_ok = -1;
        'rings: for r in 0..=radius {
            for d in -r..=r {
                for p in [
                    (center.0 + d, center.1 - r),
                    (center.0 + d, center.1 + r),
                    (center.0 - r, center.1 + d),
                    (center.0 + r, center.1 + d),
                ] {
                    if !ready(p) {
                        break 'rings;
                    }
                }
            }
            r_ok = r;
        }
        self.near_rect = if radius > 0 && r_ok >= 0 {
            [
                ((center.0 - r_ok) * 16) as f32,
                ((center.1 - r_ok) * 16) as f32,
                ((center.0 + r_ok + 1) * 16) as f32,
                ((center.1 + r_ok + 1) * 16) as f32,
            ]
        } else {
            [0.0, 0.0, -1.0, -1.0]
        };

        // Relief lointain.
        for &(key, step) in far {
            for side in [MC, VS] {
                if self.far_pending.len() >= MAX_FAR {
                    return;
                }
                let rev = self.revs[side];
                if self.far_done.get(&(side, key)) == Some(&(step, rev))
                    || self.far_pending.contains(&(side, key))
                {
                    continue;
                }
                self.far_pending.insert((side, key));
                let (map, colors, tx) =
                    (map.clone(), self.far_colors[side].clone(), self.tx.clone());
                rayon::spawn(move || {
                    let mesh = match map.regions.get(&key) {
                        Some(region) => mesh_far(region, key.0, key.1, step, &colors, side == VS),
                        None => MeshData::default(),
                    };
                    let _ = tx.send(Msg::Far {
                        side,
                        key,
                        step,
                        rev,
                        mesh,
                    });
                });
            }
        }
    }
}
