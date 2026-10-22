//! Décodage des chunks, versionné par `DataVersion`.
//!
//! Minecraft ne met à jour un chunk que lorsqu'il est rechargé : un même monde mélange
//! couramment des chunks de plusieurs époques. Chaque chunk est donc décodé selon sa propre
//! `DataVersion`, et les noms de blocs sont ramenés au vocabulaire de la dernière version.

use serde::Deserialize;

use crate::block::BlockState;
use crate::block_entity::{self, McBlockEntity};
use crate::legacy;
use crate::{McError, Result};

/// Familles de formats de chunk rencontrées au fil des versions de Minecraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkFormat {
    /// 1.18+ (21w43a, DataVersion 2844) : `sections` / `block_states` à la racine.
    Modern,
    /// 1.13 à 1.17 : compound `Level`, `Palette` + `BlockStates`.
    Flattened,
    /// Avant 1.13 (y compris avant 1.9, sans `DataVersion`) : identifiants numériques.
    Legacy,
}

impl ChunkFormat {
    pub fn for_data_version(data_version: i32) -> Self {
        match data_version {
            2844.. => Self::Modern,
            1451.. => Self::Flattened,
            _ => Self::Legacy,
        }
    }
}

/// Section de 16x16x16 blocs.
#[derive(Debug, Clone)]
pub struct Section {
    /// Index vertical de la section (y bloc = `y * 16`).
    pub y: i8,
    pub palette: Vec<BlockState>,
    /// Index dans la palette pour chaque bloc, ordre `y * 256 + z * 16 + x`.
    /// `None` quand la palette n'a qu'une entrée (section uniforme).
    pub indices: Option<Box<[u16]>>,
}

impl Section {
    pub fn block(&self, x: usize, y: usize, z: usize) -> &BlockState {
        let i = match &self.indices {
            Some(indices) => indices[y * 256 + z * 16 + x] as usize,
            None => 0,
        };
        &self.palette[i]
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_none() && self.palette.first().is_none_or(BlockState::is_air)
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    pub data_version: i32,
    pub format: ChunkFormat,
    /// Génération terminée (terrain, structures et décorations posés).
    /// Les chunks partiels (bordure de zone explorée) sont à ignorer à la conversion.
    pub complete: bool,
    /// Sections triées par `y` croissant.
    pub sections: Vec<Section>,
    pub block_entities: Vec<McBlockEntity>,
}

impl Chunk {
    /// Décode le NBT brut d'un chunk.
    pub fn from_nbt(bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct Probe {
            #[serde(rename = "DataVersion", default)]
            data_version: i32,
        }
        let probe: Probe = fastnbt::from_bytes(bytes)?;
        let mut chunk = match ChunkFormat::for_data_version(probe.data_version) {
            ChunkFormat::Modern => modern::decode(bytes)?,
            ChunkFormat::Flattened => flattened::decode(bytes, probe.data_version)?,
            ChunkFormat::Legacy => legacy_format::decode(bytes, probe.data_version)?,
        };
        for section in &mut chunk.sections {
            for state in &mut section.palette {
                legacy::normalize(state, chunk.data_version);
            }
        }
        chunk.sections.sort_by_key(|s| s.y);
        Ok(chunk)
    }
}

mod modern {
    use super::*;
    use fastnbt::LongArray;

    #[derive(Deserialize)]
    struct RawChunk {
        #[serde(rename = "DataVersion")]
        data_version: i32,
        #[serde(rename = "xPos")]
        x: i32,
        #[serde(rename = "zPos")]
        z: i32,
        #[serde(rename = "Status", default)]
        status: String,
        #[serde(default)]
        sections: Vec<RawSection>,
        #[serde(default)]
        block_entities: Vec<fastnbt::Value>,
    }

    #[derive(Deserialize)]
    struct RawSection {
        #[serde(rename = "Y")]
        y: i8,
        block_states: Option<RawBlockStates>,
    }

    #[derive(Deserialize)]
    struct RawBlockStates {
        palette: Vec<BlockState>,
        data: Option<LongArray>,
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Chunk> {
        let raw: RawChunk = fastnbt::from_bytes(bytes)?;
        let mut sections = Vec::with_capacity(raw.sections.len());
        for s in raw.sections {
            let Some(states) = s.block_states else {
                continue;
            };
            if states.palette.is_empty() {
                continue;
            }
            let indices = match states.data {
                Some(data) if states.palette.len() > 1 => {
                    Some(unpack_padded(&data, states.palette.len())?)
                }
                _ => None,
            };
            sections.push(Section {
                y: s.y,
                palette: states.palette,
                indices,
            });
        }
        Ok(Chunk {
            x: raw.x,
            z: raw.z,
            data_version: raw.data_version,
            format: ChunkFormat::Modern,
            complete: matches!(raw.status.as_str(), "minecraft:full" | "full"),
            sections,
            block_entities: raw
                .block_entities
                .iter()
                .filter_map(block_entity::parse)
                .collect(),
        })
    }
}

mod flattened {
    use super::*;
    use fastnbt::LongArray;

    /// 20w17a (1.16) : les index ne chevauchent plus deux mots.
    const PADDED_SINCE: i32 = 2529;

    #[derive(Deserialize)]
    struct RawChunk {
        #[serde(rename = "Level")]
        level: Level,
    }

    #[derive(Deserialize)]
    struct Level {
        #[serde(rename = "xPos")]
        x: i32,
        #[serde(rename = "zPos")]
        z: i32,
        #[serde(rename = "Status", default)]
        status: String,
        #[serde(rename = "Sections", default)]
        sections: Vec<RawSection>,
        #[serde(rename = "TileEntities", default)]
        tile_entities: Vec<fastnbt::Value>,
    }

    #[derive(Deserialize)]
    struct RawSection {
        #[serde(rename = "Y")]
        y: i8,
        #[serde(rename = "Palette")]
        palette: Option<Vec<BlockState>>,
        #[serde(rename = "BlockStates")]
        block_states: Option<LongArray>,
    }

    pub(super) fn decode(bytes: &[u8], data_version: i32) -> Result<Chunk> {
        let level = fastnbt::from_bytes::<RawChunk>(bytes)?.level;
        let mut sections = Vec::with_capacity(level.sections.len());
        for s in level.sections {
            let Some(palette) = s.palette.filter(|p| !p.is_empty()) else {
                continue;
            };
            let indices = match s.block_states {
                Some(data) if palette.len() > 1 => Some(if data_version >= PADDED_SINCE {
                    unpack_padded(&data, palette.len())?
                } else {
                    unpack_spanning(&data, palette.len())?
                }),
                _ => None,
            };
            sections.push(Section {
                y: s.y,
                palette,
                indices,
            });
        }
        Ok(Chunk {
            x: level.x,
            z: level.z,
            data_version,
            format: ChunkFormat::Flattened,
            // 1.14+ : `full` ; 1.13 : `postprocessed` / `fullchunk`.
            complete: matches!(
                level.status.as_str(),
                "full" | "minecraft:full" | "postprocessed" | "fullchunk"
            ),
            sections,
            block_entities: level
                .tile_entities
                .iter()
                .filter_map(block_entity::parse)
                .collect(),
        })
    }
}

mod legacy_format {
    use super::*;
    use fastnbt::ByteArray;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct RawChunk {
        #[serde(rename = "Level")]
        level: Level,
    }

    #[derive(Deserialize)]
    struct Level {
        #[serde(rename = "xPos")]
        x: i32,
        #[serde(rename = "zPos")]
        z: i32,
        #[serde(rename = "TerrainPopulated", default)]
        terrain_populated: i8,
        #[serde(rename = "Sections", default)]
        sections: Vec<RawSection>,
        #[serde(rename = "TileEntities", default)]
        tile_entities: Vec<fastnbt::Value>,
    }

    #[derive(Deserialize)]
    struct RawSection {
        #[serde(rename = "Y")]
        y: i8,
        #[serde(rename = "Blocks")]
        blocks: ByteArray,
        #[serde(rename = "Add")]
        add: Option<ByteArray>,
        #[serde(rename = "Data")]
        data: Option<ByteArray>,
    }

    fn nibble(array: &[i8], i: usize) -> u16 {
        let byte = array.get(i / 2).copied().unwrap_or(0) as u8;
        (if i % 2 == 0 { byte & 0x0f } else { byte >> 4 }) as u16
    }

    pub(super) fn decode(bytes: &[u8], data_version: i32) -> Result<Chunk> {
        let level = fastnbt::from_bytes::<RawChunk>(bytes)?.level;
        let mut sections = Vec::with_capacity(level.sections.len());
        for s in level.sections {
            if s.blocks.len() < 4096 {
                return Err(McError::Corrupt(format!(
                    "section legacy de {} blocs",
                    s.blocks.len()
                )));
            }
            // Palette locale construite à partir des couples (id, meta) rencontrés.
            let mut lookup: HashMap<(u16, u16), u16> = HashMap::new();
            let mut palette = Vec::new();
            let mut indices = vec![0u16; 4096].into_boxed_slice();
            for (i, slot) in indices.iter_mut().enumerate() {
                let mut id = s.blocks[i] as u8 as u16;
                if let Some(add) = &s.add {
                    id |= nibble(add, i) << 8;
                }
                let meta = s.data.as_deref().map_or(0, |d| nibble(d, i));
                *slot = *lookup.entry((id, meta)).or_insert_with(|| {
                    palette.push(legacy::block_state(id, meta));
                    (palette.len() - 1) as u16
                });
            }
            let indices = (palette.len() > 1).then_some(indices);
            sections.push(Section {
                y: s.y,
                palette,
                indices,
            });
        }
        Ok(Chunk {
            x: level.x,
            z: level.z,
            data_version,
            format: ChunkFormat::Legacy,
            complete: level.terrain_populated != 0,
            sections,
            block_entities: level
                .tile_entities
                .iter()
                .filter_map(block_entity::parse)
                .collect(),
        })
    }
}

fn bits_for(palette_len: usize) -> usize {
    ((usize::BITS - (palette_len - 1).leading_zeros()) as usize).max(4)
}

fn check_index(value: u64, palette_len: usize) -> Result<u16> {
    if value as usize >= palette_len {
        return Err(McError::Corrupt(format!(
            "index de palette hors limites : {value}"
        )));
    }
    Ok(value as u16)
}

/// Index compactés sans chevauchement entre mots (1.16+). Au moins 4 bits par index.
fn unpack_padded(data: &[i64], palette_len: usize) -> Result<Box<[u16]>> {
    let bits = bits_for(palette_len);
    let per_long = 64 / bits;
    if data.len() < 4096usize.div_ceil(per_long) {
        return Err(McError::Corrupt(format!(
            "section tronquée : {} mots pour {bits} bits/bloc",
            data.len()
        )));
    }
    let mask = (1u64 << bits) - 1;
    let mut out = vec![0u16; 4096].into_boxed_slice();
    for (i, slot) in out.iter_mut().enumerate() {
        let word = data[i / per_long] as u64;
        *slot = check_index((word >> ((i % per_long) * bits)) & mask, palette_len)?;
    }
    Ok(out)
}

/// Index compactés en flux continu, un index pouvant chevaucher deux mots (1.13 à 1.15).
fn unpack_spanning(data: &[i64], palette_len: usize) -> Result<Box<[u16]>> {
    let bits = bits_for(palette_len);
    if data.len() * 64 < 4096 * bits {
        return Err(McError::Corrupt(format!(
            "section tronquée : {} mots pour {bits} bits/bloc",
            data.len()
        )));
    }
    let mask = (1u64 << bits) - 1;
    let mut out = vec![0u16; 4096].into_boxed_slice();
    for (i, slot) in out.iter_mut().enumerate() {
        let bit = i * bits;
        let (word, offset) = (bit / 64, bit % 64);
        let mut value = (data[word] as u64) >> offset;
        if offset + bits > 64 {
            value |= (data[word + 1] as u64) << (64 - offset);
        }
        *slot = check_index(value & mask, palette_len)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_padded(values: &[u16], bits: usize) -> Vec<i64> {
        let per_long = 64 / bits;
        let mut out = vec![0u64; values.len().div_ceil(per_long)];
        for (i, &v) in values.iter().enumerate() {
            out[i / per_long] |= (v as u64) << ((i % per_long) * bits);
        }
        out.into_iter().map(|w| w as i64).collect()
    }

    fn pack_spanning(values: &[u16], bits: usize) -> Vec<i64> {
        let mut out = vec![0u64; (values.len() * bits).div_ceil(64)];
        for (i, &v) in values.iter().enumerate() {
            let bit = i * bits;
            let (word, offset) = (bit / 64, bit % 64);
            out[word] |= (v as u64) << offset;
            if offset + bits > 64 {
                out[word + 1] |= (v as u64) >> (64 - offset);
            }
        }
        out.into_iter().map(|w| w as i64).collect()
    }

    #[test]
    fn unpacks_with_minimum_four_bits() {
        let values: Vec<u16> = (0..4096).map(|i| (i % 3) as u16).collect();
        let unpacked = unpack_padded(&pack_padded(&values, 4), 3).unwrap();
        assert_eq!(&*unpacked, &values[..]);
    }

    #[test]
    fn unpacks_odd_bit_widths_without_spanning() {
        // 33 entrées -> 6 bits, 10 index par mot, 4 bits perdus en fin de mot.
        let values: Vec<u16> = (0..4096).map(|i| (i * 7 % 33) as u16).collect();
        let unpacked = unpack_padded(&pack_padded(&values, 6), 33).unwrap();
        assert_eq!(&*unpacked, &values[..]);
    }

    #[test]
    fn unpacks_spanning_indices() {
        // 5 bits : des index chevauchent deux mots.
        let values: Vec<u16> = (0..4096).map(|i| (i * 11 % 20) as u16).collect();
        let unpacked = unpack_spanning(&pack_spanning(&values, 5), 20).unwrap();
        assert_eq!(&*unpacked, &values[..]);
    }

    #[test]
    fn rejects_out_of_range_indices() {
        let values = vec![5u16; 4096];
        assert!(unpack_padded(&pack_padded(&values, 4), 3).is_err());
    }

    #[test]
    fn picks_format_from_data_version() {
        assert_eq!(ChunkFormat::for_data_version(3839), ChunkFormat::Modern);
        assert_eq!(ChunkFormat::for_data_version(2586), ChunkFormat::Flattened);
        assert_eq!(ChunkFormat::for_data_version(1343), ChunkFormat::Legacy);
        assert_eq!(ChunkFormat::for_data_version(0), ChunkFormat::Legacy);
    }
}
