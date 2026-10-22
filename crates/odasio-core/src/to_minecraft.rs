//! Conversion d'une sauvegarde Vintage Story en monde Minecraft Java.

use std::collections::{BTreeMap, HashMap};
use std::f32::consts::{FRAC_PI_2, PI};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use odasio_mc::BlockState;
use odasio_mc::writer::{self, ChunkBuilder, Generator, LevelOptions, MIN_Y};
use odasio_vs::{Attr, BlockEntity, CHUNK_SIZE, DEFAULT_MAP_SIZE_XZ, VsDatabase};
use rayon::prelude::*;

use crate::convert::{ConvertError, MC_SEA_LEVEL, Result, vs_sea_level};
use crate::mapping::Quality;
use crate::reverse::ReverseMapping;

#[derive(Debug, Clone)]
pub struct ToMcOptions {
    /// Dossier du monde Minecraft à créer.
    pub output: PathBuf,
    pub overwrite: bool,
    pub world_name: String,
    /// Terrain hors de la zone convertie.
    pub generator: Generator,
    pub creative: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ToMcProgress {
    Converting { done: usize, total: usize },
    Finishing,
}

#[derive(Debug, Clone, Default)]
pub struct ToMcReport {
    pub columns: usize,
    pub chunks: usize,
    /// Codes VS sans correspondance (repli), par nombre de blocs.
    pub unmapped: BTreeMap<String, u64>,
    pub block_entities: usize,
    pub signs: usize,
    pub spawn: [i32; 3],
    pub elapsed: Duration,
}

/// Table des états Minecraft du monde converti ; l'index 0 est l'air.
struct States {
    list: RwLock<Vec<BlockState>>,
    index: Mutex<HashMap<BlockState, u32>>,
}

impl States {
    fn new() -> Self {
        let air = BlockState::new("minecraft:air");
        Self {
            list: RwLock::new(vec![air.clone()]),
            index: Mutex::new(HashMap::from([(air, 0)])),
        }
    }

    fn intern(&self, state: BlockState) -> u32 {
        let mut index = self.index.lock().unwrap();
        if let Some(&i) = index.get(&state) {
            return i;
        }
        let mut list = self.list.write().unwrap();
        list.push(state.clone());
        let i = (list.len() - 1) as u32;
        index.insert(state, i);
        i
    }

    fn get(&self, i: u32) -> BlockState {
        self.list.read().unwrap()[i as usize].clone()
    }
}

fn float(be: &BlockEntity, key: &str) -> Option<f32> {
    match be.attrs.get(key) {
        Some(Attr::Float(v)) => Some(*v),
        _ => None,
    }
}

fn boolean(be: &BlockEntity, key: &str) -> bool {
    matches!(be.attrs.get(key), Some(Attr::Bool(true)))
}

/// Direction dont l'angle VS (sud 0, est pi/2, nord pi, ouest -pi/2) est le plus proche.
fn facing_from_angle(angle: f32) -> &'static str {
    let quarter = (angle / FRAC_PI_2).round() as i32;
    match quarter.rem_euclid(4) {
        0 => "south",
        1 => "east",
        2 => "north",
        _ => "west",
    }
}

/// Blocs Minecraft qui acceptent la propriété `waterlogged`.
fn can_hold_water(name: &str) -> bool {
    const SUFFIXES: &[&str] = &[
        "_slab",
        "_stairs",
        "_fence",
        "_wall",
        "_pane",
        "_bars",
        "_sign",
        "_trapdoor",
        "lantern",
        "chain",
        "ladder",
        "_coral",
        "_coral_fan",
        "sea_pickle",
        "scaffolding",
        "candle",
        "_rail",
        "conduit",
        "lightning_rod",
        "chest",
    ];
    let path = name.split_once(':').map_or(name, |(_, p)| p);
    SUFFIXES.iter().any(|s| path.ends_with(s)) && !path.ends_with("_fence_gate")
}

fn with_props(mut state: BlockState, props: &[(&str, &str)]) -> BlockState {
    for (k, v) in props {
        state.properties.insert((*k).to_string(), (*v).to_string());
    }
    state
}

/// Ajustements qui dépendent des block entities VS : orientation des portes et des coffres,
/// rotation et texte des panneaux. Renvoie les états à poser (position MC, index d'état) et les
/// block entities Minecraft à créer.
struct BeOutcome {
    blocks: Vec<([i32; 3], u32)>,
    entities: Vec<([i32; 3], writer::Nbt)>,
    signs: usize,
}

fn apply_block_entity(
    be: &BlockEntity,
    base: &BlockState,
    pos: [i32; 3],
    states: &States,
) -> BeOutcome {
    let mut out = BeOutcome {
        blocks: Vec::new(),
        entities: Vec::new(),
        signs: 0,
    };
    let name = base.name.as_str();
    if name.ends_with("_door") && be.block_code.starts_with("door-") {
        // Porte fermée du côté de son poseur : facing MC opposé à l'angle VS.
        let yaw = float(be, "rotateYRad").unwrap_or(0.0);
        let facing = match facing_from_angle(yaw) {
            "south" => "north",
            "west" => "east",
            "north" => "south",
            _ => "west",
        };
        let hinge = if boolean(be, "invertHandles") {
            "right"
        } else {
            "left"
        };
        let open = if boolean(be, "opened") {
            "true"
        } else {
            "false"
        };
        for (half, dy) in [("lower", 0), ("upper", 1)] {
            let state = with_props(
                base.clone(),
                &[
                    ("facing", facing),
                    ("hinge", hinge),
                    ("open", open),
                    ("half", half),
                    ("powered", "false"),
                ],
            );
            out.blocks
                .push(([pos[0], pos[1] + dy, pos[2]], states.intern(state)));
        }
    } else if name.ends_with("chest") || name == "minecraft:barrel" {
        if let Some(angle) = float(be, "meshAngle") {
            let state = with_props(base.clone(), &[("facing", facing_from_angle(angle))]);
            out.blocks.push((pos, states.intern(state)));
        }
        out.entities.push((pos, writer::plain_entity(name, pos)));
    } else if name.ends_with("_sign") {
        let text = match be.attrs.get("text") {
            Some(Attr::Str(t)) => t.clone(),
            _ => String::new(),
        };
        if !name.ends_with("_wall_sign") {
            // Panneau sur pied : rotation MC (16 crans, 0 = texte vers le sud, sens horaire).
            let angle = float(be, "meshAngle").unwrap_or(0.0);
            let rotation = ((-angle / (PI / 8.0)).round() as i32)
                .rem_euclid(16)
                .to_string();
            let state = with_props(base.clone(), &[("rotation", &rotation)]);
            out.blocks.push((pos, states.intern(state)));
        }
        let lines: Vec<String> = text.split('\n').take(4).map(str::to_string).collect();
        out.signs += usize::from(!text.trim().is_empty());
        out.entities.push((
            pos,
            writer::sign_entity("minecraft:sign", pos, &lines, "black"),
        ));
    } else if name.ends_with("furnace")
        || name == "minecraft:smoker"
        || name.ends_with("shulker_box")
    {
        out.entities.push((pos, writer::plain_entity(name, pos)));
    }
    out
}

pub fn convert_to_minecraft(
    save: &Path,
    mapping: &ReverseMapping,
    opts: &ToMcOptions,
    progress: impl Fn(ToMcProgress) + Sync,
) -> Result<ToMcReport> {
    let started = Instant::now();
    if opts.output.exists() {
        if !opts.overwrite {
            return Err(ConvertError::OutputExists(opts.output.clone()));
        }
        std::fs::remove_dir_all(&opts.output)?;
    }
    let region_dir = opts.output.join("region");
    std::fs::create_dir_all(&region_dir)?;

    let db = VsDatabase::open(save)?;
    let ids = db.save.block_ids()?;
    let height = db.save.map_size[1];
    // La mer VS arrive au niveau de la mer Minecraft.
    let offset = vs_sea_level(height) - MC_SEA_LEVEL;
    let center = (DEFAULT_MAP_SIZE_XZ / 2) as i32;
    let spawn = db
        .save
        .default_spawn
        .unwrap_or([center, vs_sea_level(height), center]);
    // Origine alignée sur 512 : une région Minecraft couvre exactement 16 x 16 colonnes VS.
    let origin = [
        spawn[0].div_euclid(512) * 512,
        spawn[2].div_euclid(512) * 512,
    ];

    // Correspondance de chaque ID du monde VS, calculée une fois.
    let states = States::new();
    let size = ids.values().copied().max().unwrap_or(0) as usize + 1;
    let mut table = vec![0u32; size];
    let mut quality = vec![Quality::Exact; size];
    let mut codes = vec![String::new(); size];
    for (code, &id) in &ids {
        let resolved = mapping.resolve(code);
        table[id as usize] = states.intern(resolved.state);
        quality[id as usize] = resolved.quality;
        codes[id as usize] = code.clone();
    }
    table[0] = 0;
    let is_water: Vec<bool> = codes
        .iter()
        .map(|c| c.starts_with("game:water-") || c.starts_with("game:saltwater-"))
        .collect();
    let waterlogged: Vec<u32> = table
        .iter()
        .map(|&s| {
            let state = states.get(s);
            if can_hold_water(&state.name) {
                states.intern(with_props(state, &[("waterlogged", "true")]))
            } else {
                s
            }
        })
        .collect();

    // Colonnes regroupées par région Minecraft.
    let mut regions: HashMap<(i32, i32), Vec<(u32, u32)>> = HashMap::new();
    for (cx, cz, _) in db.map_chunks()? {
        let (x, z) = (cx as i32 * 32 - origin[0], cz as i32 * 32 - origin[1]);
        regions
            .entry((x.div_euclid(512), z.div_euclid(512)))
            .or_default()
            .push((cx, cz));
    }
    let regions: Vec<_> = regions.into_iter().collect();
    let total = regions.len();
    let done = AtomicUsize::new(0);
    let report = Mutex::new(ToMcReport::default());
    progress(ToMcProgress::Converting { done: 0, total });

    let deepslate = states.intern(BlockState::parse("minecraft:deepslate[axis=y]"));
    let bedrock = states.intern(BlockState::new("minecraft:bedrock"));
    let bottom = -offset; // Y Minecraft du fond du monde VS

    regions
        .par_iter()
        .try_for_each(|((rx, rz), columns)| -> Result<()> {
            let db = VsDatabase::open(save)?;
            let mut chunks: HashMap<(i32, i32), ChunkBuilder> = HashMap::new();
            let mut unmapped: BTreeMap<String, u64> = BTreeMap::new();
            let (mut entities_count, mut signs) = (0usize, 0usize);
            for &(cx, cz) in columns {
                let Some(column) = db.read_column(cx, cz)? else {
                    continue;
                };
                let base_x = cx as i32 * 32 - origin[0];
                let base_z = cz as i32 * 32 - origin[1];
                for (dx, dz) in [(0, 0), (16, 0), (0, 16), (16, 16)] {
                    let key = ((base_x + dx).div_euclid(16), (base_z + dz).div_euclid(16));
                    let chunk = chunks
                        .entry(key)
                        .or_insert_with(|| ChunkBuilder::new(key.0, key.1));
                    // Sous le fond du monde VS : ardoise des profondeurs.
                    for z in 0..16 {
                        for x in 0..16 {
                            for y in MIN_Y + 1..bottom {
                                chunk.set(x, y, z, deepslate);
                            }
                        }
                    }
                }
                for cy in 0..column.blocks.len() {
                    let (blocks, fluids) = (&column.blocks[cy], &column.fluids[cy]);
                    if blocks.is_none() && fluids.is_none() {
                        continue;
                    }
                    for i in 0..CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE {
                        let block = blocks.as_ref().map_or(0, |b| b[i]) as usize;
                        let fluid = fluids.as_ref().map_or(0, |f| f[i]) as usize;
                        let id = if block != 0 { block } else { fluid };
                        if id == 0 || id >= table.len() {
                            continue;
                        }
                        // Eau dans un bloc qui peut en contenir : variante waterlogged.
                        let state = if block != 0 && fluid < is_water.len() && is_water[fluid] {
                            waterlogged[block]
                        } else {
                            table[id]
                        };
                        if quality[id] == Quality::Unmapped {
                            *unmapped.entry(codes[id].clone()).or_default() += 1;
                        }
                        let (x, z) = (i % 32, (i / 32) % 32);
                        let y = cy as i32 * 32 + (i / 1024) as i32 - offset;
                        let key = (
                            (base_x + x as i32).div_euclid(16),
                            (base_z + z as i32).div_euclid(16),
                        );
                        if let Some(chunk) = chunks.get_mut(&key) {
                            chunk.set(x % 16, y, z % 16, state);
                        }
                    }
                }
                // Plancher de bedrock posé en dernier : le fond VS peut descendre sous Y = -64.
                for (dx, dz) in [(0, 0), (16, 0), (0, 16), (16, 16)] {
                    let key = ((base_x + dx).div_euclid(16), (base_z + dz).div_euclid(16));
                    if let Some(chunk) = chunks.get_mut(&key) {
                        for z in 0..16 {
                            for x in 0..16 {
                                chunk.set(x, MIN_Y, z, bedrock);
                            }
                        }
                    }
                }
                for be in &column.block_entities {
                    let id = ids
                        .get(&format!("game:{}", be.block_code))
                        .copied()
                        .unwrap_or(0) as usize;
                    if id == 0 || id >= table.len() {
                        continue;
                    }
                    let base = states.get(table[id]);
                    let pos = [
                        be.pos[0] - origin[0],
                        be.pos[1] - offset,
                        be.pos[2] - origin[1],
                    ];
                    let outcome = apply_block_entity(be, &base, pos, &states);
                    signs += outcome.signs;
                    for (p, state) in outcome.blocks {
                        if let Some(chunk) =
                            chunks.get_mut(&(p[0].div_euclid(16), p[2].div_euclid(16)))
                        {
                            chunk.set(
                                p[0].rem_euclid(16) as usize,
                                p[1],
                                p[2].rem_euclid(16) as usize,
                                state,
                            );
                        }
                    }
                    for (p, entity) in outcome.entities {
                        if let Some(chunk) =
                            chunks.get_mut(&(p[0].div_euclid(16), p[2].div_euclid(16)))
                        {
                            chunk.block_entities.push(entity);
                            entities_count += 1;
                        }
                    }
                }
            }

            let list = states.list.read().unwrap().clone();
            let mut encoded = Vec::with_capacity(chunks.len());
            for chunk in chunks.values().filter(|c| !c.is_empty()) {
                encoded.push((chunk.x, chunk.z, chunk.to_nbt(&list)?));
            }
            writer::write_region(&region_dir, *rx, *rz, &encoded)?;

            let mut r = report.lock().unwrap();
            r.columns += columns.len();
            r.chunks += encoded.len();
            r.block_entities += entities_count;
            r.signs += signs;
            for (code, n) in unmapped {
                *r.unmapped.entry(code).or_default() += n;
            }
            drop(r);
            let done = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(ToMcProgress::Converting { done, total });
            Ok(())
        })?;

    progress(ToMcProgress::Finishing);
    let mut report = report.into_inner().unwrap();
    report.spawn = [
        spawn[0] - origin[0],
        spawn[1] - offset + 1,
        spawn[2] - origin[1],
    ];
    writer::write_level_dat(
        &opts.output.join("level.dat"),
        &LevelOptions {
            name: opts.world_name.clone(),
            spawn: report.spawn,
            creative: opts.creative,
            seed: 0,
            generator: opts.generator,
        },
    )?;
    report.elapsed = started.elapsed();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_round_to_the_nearest_side() {
        assert_eq!(facing_from_angle(0.0), "south");
        assert_eq!(facing_from_angle(FRAC_PI_2), "east");
        assert_eq!(facing_from_angle(PI), "north");
        assert_eq!(facing_from_angle(-FRAC_PI_2), "west");
        assert_eq!(facing_from_angle(-PI), "north");
    }
}
