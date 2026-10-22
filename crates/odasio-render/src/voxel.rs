//! Chargement des chunks d'un monde Minecraft pour le rendu 3D : blocs d'une colonne de
//! chunk, indexés dans un registre d'états partagé par tous les chunks.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use odasio_mc::{BlockState, Chunk, ChunkLocation, Region};

/// États de bloc rencontrés, index 0 = air.
pub struct Registry {
    states: Vec<BlockState>,
    index: HashMap<BlockState, u16>,
}

impl Registry {
    fn new() -> Self {
        let air = BlockState::new("minecraft:air");
        Self {
            states: vec![air.clone()],
            index: HashMap::from([(air, 0)]),
        }
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn get(&self, id: u16) -> Option<&BlockState> {
        self.states.get(id as usize)
    }
}

/// Blocs d'une colonne de chunk, de `min_y` à `min_y + height` exclus.
pub struct ChunkBlocks {
    pub cx: i32,
    pub cz: i32,
    pub min_y: i32,
    pub height: usize,
    /// Index du registre, ordre `(y * 16 + z) * 16 + x`.
    pub ids: Vec<u16>,
    /// Génération terminée ; les chunks incomplets sont ignorés par la conversion.
    pub complete: bool,
}

impl ChunkBlocks {
    /// Bloc aux coordonnées locales (`y` relatif à `min_y`), 0 hors limites.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u16 {
        if !(0..16).contains(&x) || !(0..16).contains(&z) || y < 0 || y as usize >= self.height {
            return 0;
        }
        self.ids[((y as usize * 16) + z as usize) * 16 + x as usize]
    }
}

/// Fichiers de région ouverts (`None` : absent ou illisible).
type Regions = Mutex<HashMap<(i32, i32), Option<Arc<Mutex<Region>>>>>;

/// Source de chunks d'une dimension, utilisable depuis plusieurs fils.
pub struct ChunkSource {
    region_dir: PathBuf,
    regions: Regions,
    pub registry: RwLock<Registry>,
}

impl ChunkSource {
    pub fn new(region_dir: PathBuf) -> Self {
        Self {
            region_dir,
            regions: Mutex::new(HashMap::new()),
            registry: RwLock::new(Registry::new()),
        }
    }

    fn region(&self, rx: i32, rz: i32) -> Option<Arc<Mutex<Region>>> {
        let mut regions = self.regions.lock().unwrap();
        regions
            .entry((rx, rz))
            .or_insert_with(|| {
                let path = self.region_dir.join(format!("r.{rx}.{rz}.mca"));
                Region::open(&path).ok().map(|r| Arc::new(Mutex::new(r)))
            })
            .clone()
    }

    fn id(&self, state: &BlockState) -> u16 {
        if let Some(&id) = self.registry.read().unwrap().index.get(state) {
            return id;
        }
        let mut registry = self.registry.write().unwrap();
        if let Some(&id) = registry.index.get(state) {
            return id;
        }
        let id = registry.states.len() as u16;
        registry.states.push(state.clone());
        registry.index.insert(state.clone(), id);
        id
    }

    /// Lit le chunk (`cx`, `cz`) ; `None` s'il n'existe pas ou est vide.
    pub fn load(&self, cx: i32, cz: i32) -> Option<ChunkBlocks> {
        let region = self.region(cx.div_euclid(32), cz.div_euclid(32))?;
        let nbt = region
            .lock()
            .unwrap()
            .read_chunk_nbt(ChunkLocation { x: cx, z: cz })
            .ok()??;
        let chunk = Chunk::from_nbt(&nbt).ok()?;
        let sections: Vec<_> = chunk.sections.iter().filter(|s| !s.is_empty()).collect();
        let (low, high) = (sections.first()?.y as i32, sections.last()?.y as i32);
        let (min_y, height) = (low * 16, ((high - low + 1) * 16) as usize);
        let mut ids = vec![0u16; height * 256];
        for section in sections {
            // Ids locaux à la section, puis recopie.
            let local: Vec<u16> = section.palette.iter().map(|s| self.id(s)).collect();
            let base = ((section.y as i32 - low) * 16) as usize * 256;
            match &section.indices {
                Some(indices) => {
                    for (i, &p) in indices.iter().enumerate() {
                        ids[base + i] = local[p as usize];
                    }
                }
                None => ids[base..base + 4096].fill(local[0]),
            }
        }
        Some(ChunkBlocks {
            cx,
            cz,
            min_y,
            height,
            ids,
            complete: chunk.complete,
        })
    }
}
