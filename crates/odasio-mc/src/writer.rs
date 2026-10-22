//! Écriture de mondes Minecraft Java (format 1.21.1) : chunks, régions Anvil, `level.dat`.
//!
//! Les chunks sont écrits sans lumière (`isLightOn = 0`) : le jeu la recalcule à l'ouverture.
//! Une version plus récente de Minecraft met le monde à jour automatiquement.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use fastnbt::{LongArray, Value};

/// Valeur NBT (block entities à ajouter aux chunks).
pub use fastnbt::Value as Nbt;
use flate2::Compression;
use flate2::write::{GzEncoder, ZlibEncoder};

use crate::block::BlockState;
use crate::{McError, Result};

/// DataVersion de Minecraft 1.21.1.
pub const DATA_VERSION: i32 = 3955;
pub const VERSION_NAME: &str = "1.21.1";
/// Bas du monde et nombre de sections (Y = -64 à 319).
pub const MIN_Y: i32 = -64;
pub const SECTIONS: usize = 24;

fn compound(entries: Vec<(&str, Value)>) -> Value {
    Value::Compound(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn string(s: &str) -> Value {
    Value::String(s.to_string())
}

/// Chunk Minecraft en construction. Les blocs sont des index dans une table d'états partagée
/// par tout le monde converti (0 = air), pour éviter de dupliquer les états par chunk.
pub struct ChunkBuilder {
    pub x: i32,
    pub z: i32,
    sections: Vec<Option<Box<[u32]>>>,
    pub block_entities: Vec<Value>,
}

impl ChunkBuilder {
    pub fn new(x: i32, z: i32) -> Self {
        Self {
            x,
            z,
            sections: (0..SECTIONS).map(|_| None).collect(),
            block_entities: Vec::new(),
        }
    }

    /// `x`, `z` locaux (0..16), `y` absolu (-64..319). Hors hauteur : ignoré.
    #[inline]
    pub fn set(&mut self, x: usize, y: i32, z: usize, state: u32) -> bool {
        let section = (y - MIN_Y).div_euclid(16);
        if !(0..SECTIONS as i32).contains(&section) {
            return false;
        }
        if state == 0 && self.sections[section as usize].is_none() {
            return true;
        }
        let ly = (y - MIN_Y).rem_euclid(16) as usize;
        self.sections[section as usize]
            .get_or_insert_with(|| vec![0u32; 4096].into_boxed_slice())[ly * 256 + z * 16 + x] =
            state;
        true
    }

    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(Option::is_none) && self.block_entities.is_empty()
    }

    /// NBT du chunk (non compressé). `states[i]` est l'état de l'index `i`.
    pub fn to_nbt(&self, states: &[BlockState]) -> Result<Vec<u8>> {
        let plains = compound(vec![(
            "palette",
            Value::List(vec![string("minecraft:plains")]),
        )]);
        let mut sections = Vec::with_capacity(SECTIONS);
        for (i, cells) in self.sections.iter().enumerate() {
            let y = (MIN_Y / 16) + i as i32;
            let block_states = match cells {
                None => compound(vec![(
                    "palette",
                    Value::List(vec![state_nbt(&BlockState::new("minecraft:air"))]),
                )]),
                Some(cells) => section_states(cells, states),
            };
            sections.push(compound(vec![
                ("Y", Value::Byte(y as i8)),
                ("block_states", block_states),
                ("biomes", plains.clone()),
            ]));
        }
        let root = compound(vec![
            ("DataVersion", Value::Int(DATA_VERSION)),
            ("xPos", Value::Int(self.x)),
            ("zPos", Value::Int(self.z)),
            ("yPos", Value::Int(MIN_Y / 16)),
            ("Status", string("minecraft:full")),
            ("LastUpdate", Value::Long(0)),
            ("InhabitedTime", Value::Long(0)),
            // Lumière absente : recalculée par le jeu au chargement.
            ("isLightOn", Value::Byte(0)),
            ("sections", Value::List(sections)),
            ("block_entities", Value::List(self.block_entities.clone())),
            ("PostProcessing", Value::List(Vec::new())),
            (
                "structures",
                compound(vec![
                    ("starts", compound(vec![])),
                    ("References", compound(vec![])),
                ]),
            ),
        ]);
        Ok(fastnbt::to_bytes(&root)?)
    }
}

fn state_nbt(state: &BlockState) -> Value {
    let mut entries = vec![("Name", string(&state.name))];
    let props;
    if !state.properties.is_empty() {
        props = Value::Compound(
            state
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), string(v)))
                .collect(),
        );
        entries.push(("Properties", props));
    }
    compound(entries)
}

/// Palette locale et index compactés d'une section (format 1.16+, sans chevauchement).
fn section_states(cells: &[u32], states: &[BlockState]) -> Value {
    let mut palette: Vec<u32> = Vec::new();
    let mut lookup: HashMap<u32, u64> = HashMap::new();
    let indices: Vec<u64> = cells
        .iter()
        .map(|&s| {
            *lookup.entry(s).or_insert_with(|| {
                palette.push(s);
                (palette.len() - 1) as u64
            })
        })
        .collect();
    let palette_nbt = Value::List(
        palette
            .iter()
            .map(|&s| state_nbt(&states[s as usize]))
            .collect(),
    );
    if palette.len() == 1 {
        return compound(vec![("palette", palette_nbt)]);
    }
    let bits = ((usize::BITS - (palette.len() - 1).leading_zeros()) as usize).max(4);
    let per_long = 64 / bits;
    let mut data = vec![0i64; 4096usize.div_ceil(per_long)];
    for (i, &index) in indices.iter().enumerate() {
        data[i / per_long] |= (index << ((i % per_long) * bits)) as i64;
    }
    compound(vec![
        ("palette", palette_nbt),
        ("data", Value::LongArray(LongArray::new(data))),
    ])
}

/// Écrit un fichier région `r.<x>.<z>.mca` à partir des NBT de ses chunks (coordonnées
/// absolues).
pub fn write_region(dir: &Path, rx: i32, rz: i32, chunks: &[(i32, i32, Vec<u8>)]) -> Result<()> {
    const SECTOR: usize = 4096;
    let mut header = vec![0u8; 2 * SECTOR];
    let mut body: Vec<u8> = Vec::new();
    for (x, z, nbt) in chunks {
        let index = (x.rem_euclid(32) + z.rem_euclid(32) * 32) as usize;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(nbt)?;
        let compressed = encoder.finish()?;
        let mut payload = Vec::with_capacity(compressed.len() + 5);
        payload.extend_from_slice(&(compressed.len() as u32 + 1).to_be_bytes());
        payload.push(2); // zlib
        payload.extend_from_slice(&compressed);
        let sectors = payload.len().div_ceil(SECTOR);
        if sectors > 255 {
            return Err(McError::Corrupt(format!("chunk {x},{z} trop volumineux")));
        }
        let offset = 2 + body.len() / SECTOR;
        header[index * 4..index * 4 + 3].copy_from_slice(&(offset as u32).to_be_bytes()[1..]);
        header[index * 4 + 3] = sectors as u8;
        payload.resize(sectors * SECTOR, 0);
        body.extend_from_slice(&payload);
    }
    let mut file = std::fs::File::create(dir.join(format!("r.{rx}.{rz}.mca")))?;
    file.write_all(&header)?;
    file.write_all(&body)?;
    Ok(())
}

/// Générateur de l'Overworld hors de la zone convertie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generator {
    /// Terrain Minecraft normal.
    Normal,
    /// Eau à perte de vue au niveau de la mer.
    Ocean,
    /// Vide.
    Void,
}

#[derive(Debug, Clone)]
pub struct LevelOptions {
    pub name: String,
    pub spawn: [i32; 3],
    pub creative: bool,
    pub seed: i64,
    pub generator: Generator,
}

fn flat(layers: &[(&str, i32)], biome: &str) -> Value {
    compound(vec![
        ("type", string("minecraft:flat")),
        (
            "settings",
            compound(vec![
                (
                    "layers",
                    Value::List(
                        layers
                            .iter()
                            .map(|(block, height)| {
                                compound(vec![
                                    ("block", string(block)),
                                    ("height", Value::Int(*height)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("biome", string(biome)),
                ("lakes", Value::Byte(0)),
                ("features", Value::Byte(0)),
                ("structure_overrides", Value::List(Vec::new())),
            ]),
        ),
    ])
}

fn noise(settings: &str, preset: &str) -> Value {
    compound(vec![
        ("type", string("minecraft:noise")),
        ("settings", string(settings)),
        (
            "biome_source",
            compound(vec![
                ("type", string("minecraft:multi_noise")),
                ("preset", string(preset)),
            ]),
        ),
    ])
}

/// Écrit `level.dat` (NBT compressé en gzip).
pub fn write_level_dat(path: &Path, opts: &LevelOptions) -> Result<()> {
    let overworld = match opts.generator {
        Generator::Normal => noise("minecraft:overworld", "minecraft:overworld"),
        // Fond de pierre puis eau jusqu'au niveau de la mer (Y = 62).
        Generator::Ocean => flat(
            &[
                ("minecraft:bedrock", 1),
                ("minecraft:stone", 90),
                ("minecraft:sand", 6),
                ("minecraft:water", 30),
            ],
            "minecraft:ocean",
        ),
        Generator::Void => flat(&[("minecraft:air", 1)], "minecraft:the_void"),
    };
    let dimension = |kind: &str, generator: Value| {
        compound(vec![("type", string(kind)), ("generator", generator)])
    };
    let end = compound(vec![
        ("type", string("minecraft:noise")),
        ("settings", string("minecraft:end")),
        (
            "biome_source",
            compound(vec![("type", string("minecraft:the_end"))]),
        ),
    ]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let data = compound(vec![
        ("DataVersion", Value::Int(DATA_VERSION)),
        ("version", Value::Int(19133)),
        (
            "Version",
            compound(vec![
                ("Id", Value::Int(DATA_VERSION)),
                ("Name", string(VERSION_NAME)),
                ("Series", string("main")),
                ("Snapshot", Value::Byte(0)),
            ]),
        ),
        ("LevelName", string(&opts.name)),
        ("SpawnX", Value::Int(opts.spawn[0])),
        ("SpawnY", Value::Int(opts.spawn[1])),
        ("SpawnZ", Value::Int(opts.spawn[2])),
        ("SpawnAngle", Value::Float(0.0)),
        ("GameType", Value::Int(if opts.creative { 1 } else { 0 })),
        ("allowCommands", Value::Byte(1)),
        ("hardcore", Value::Byte(0)),
        ("Difficulty", Value::Byte(2)),
        ("initialized", Value::Byte(1)),
        ("LastPlayed", Value::Long(now)),
        ("Time", Value::Long(0)),
        ("DayTime", Value::Long(6000)),
        ("raining", Value::Byte(0)),
        ("thundering", Value::Byte(0)),
        (
            "DataPacks",
            compound(vec![
                ("Enabled", Value::List(vec![string("vanilla")])),
                ("Disabled", Value::List(Vec::new())),
            ]),
        ),
        (
            "WorldGenSettings",
            compound(vec![
                ("seed", Value::Long(opts.seed)),
                ("generate_features", Value::Byte(1)),
                ("bonus_chest", Value::Byte(0)),
                (
                    "dimensions",
                    compound(vec![
                        (
                            "minecraft:overworld",
                            dimension("minecraft:overworld", overworld),
                        ),
                        (
                            "minecraft:the_nether",
                            dimension(
                                "minecraft:the_nether",
                                noise("minecraft:nether", "minecraft:nether"),
                            ),
                        ),
                        ("minecraft:the_end", dimension("minecraft:the_end", end)),
                    ]),
                ),
            ]),
        ),
    ]);
    let root = compound(vec![("Data", data)]);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&fastnbt::to_bytes(&root)?)?;
    std::fs::write(path, encoder.finish()?)?;
    Ok(())
}

/// Block entity de panneau (1.20+) : face avant avec le texte, face arrière vide.
pub fn sign_entity(id: &str, pos: [i32; 3], lines: &[String], color: &str) -> Value {
    let messages = |lines: &[String]| {
        Value::List(
            (0..4)
                .map(|i| {
                    let text = lines.get(i).map_or("", String::as_str);
                    string(&serde_json::Value::String(text.to_string()).to_string())
                })
                .collect(),
        )
    };
    let side = |lines: &[String]| {
        compound(vec![
            ("messages", messages(lines)),
            ("color", string(color)),
            ("has_glowing_text", Value::Byte(0)),
        ])
    };
    compound(vec![
        ("id", string(id)),
        ("x", Value::Int(pos[0])),
        ("y", Value::Int(pos[1])),
        ("z", Value::Int(pos[2])),
        ("keepPacked", Value::Byte(0)),
        ("is_waxed", Value::Byte(0)),
        ("front_text", side(lines)),
        ("back_text", side(&[])),
    ])
}

/// Block entity sans données particulières (coffre vide, four...).
pub fn plain_entity(id: &str, pos: [i32; 3]) -> Value {
    compound(vec![
        ("id", string(id)),
        ("x", Value::Int(pos[0])),
        ("y", Value::Int(pos[1])),
        ("z", Value::Int(pos[2])),
        ("keepPacked", Value::Byte(0)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chunk;
    use crate::region::Region;

    #[test]
    fn written_chunks_read_back() {
        let states = vec![
            BlockState::new("minecraft:air"),
            BlockState::new("minecraft:stone"),
            BlockState::parse("minecraft:oak_log[axis=x]"),
        ];
        let mut chunk = ChunkBuilder::new(-3, 5);
        for x in 0..16 {
            for z in 0..16 {
                chunk.set(x, -64, z, 1);
            }
        }
        chunk.set(4, 70, 9, 2);
        let nbt = chunk.to_nbt(&states).unwrap();

        let dir = std::env::temp_dir().join(format!("odasio-writer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_region(&dir, -1, 0, &[(-3, 5, nbt)]).unwrap();
        let mut region = Region::open(&dir.join("r.-1.0.mca")).unwrap();
        let loc = region.chunks().next().unwrap();
        assert_eq!((loc.x, loc.z), (-3, 5));
        let read = Chunk::from_nbt(&region.read_chunk_nbt(loc).unwrap().unwrap()).unwrap();
        assert!(read.complete);
        let bottom = read.sections.iter().find(|s| s.y == -4).unwrap();
        assert_eq!(bottom.block(7, 0, 7).name, "minecraft:stone");
        let log_section = read.sections.iter().find(|s| s.y == 4).unwrap();
        assert_eq!(log_section.block(4, 70 - 64, 9).property("axis"), Some("x"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn level_dat_reads_back() {
        let path = std::env::temp_dir().join(format!("odasio-level-{}.dat", std::process::id()));
        write_level_dat(
            &path,
            &LevelOptions {
                name: "Test".into(),
                spawn: [10, 80, -5],
                creative: true,
                seed: 42,
                generator: Generator::Ocean,
            },
        )
        .unwrap();
        let level = crate::LevelInfo::read(&path).unwrap();
        assert_eq!(level.name, "Test");
        assert_eq!(level.data_version, DATA_VERSION);
        assert_eq!(level.spawn, Some([10, 80, -5]));
        std::fs::remove_file(&path).unwrap();
    }
}
