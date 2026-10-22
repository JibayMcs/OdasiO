//! Blob `gamedata` (`SaveGame`) : table des IDs de blocs, dimensions du monde, spawn.

use std::collections::HashMap;

use crate::proto::{ProtoReader, ProtoWriter};
use crate::{Result, VsError};

const F_MAP_SIZE_X: u32 = 1;
const F_MAP_SIZE_Y: u32 = 2;
const F_MAP_SIZE_Z: u32 = 3;
const F_MOD_DATA: u32 = 11;
const F_WORLD_NAME: u32 = 13;
const F_WORLD_TYPE: u32 = 30;
const F_DEFAULT_SPAWN: u32 = 40;

#[derive(Debug, Clone)]
pub struct SaveGame {
    raw: Vec<u8>,
    pub map_size: [u32; 3],
    pub world_name: String,
    pub world_type: String,
    /// Point d'apparition par défaut (`DefaultSpawn`), `None` = centre de la carte.
    pub default_spawn: Option<[i32; 3]>,
}

impl SaveGame {
    pub fn parse(raw: Vec<u8>) -> Result<Self> {
        let mut map_size = [0u32; 3];
        let mut world_name = String::new();
        let mut world_type = String::new();
        let mut default_spawn = None;
        for field in ProtoReader::new(&raw) {
            let field = field?;
            match field.number {
                F_MAP_SIZE_X | F_MAP_SIZE_Y | F_MAP_SIZE_Z => {
                    map_size[(field.number - 1) as usize] =
                        field.value.as_u64().unwrap_or(0) as u32;
                }
                F_WORLD_NAME => world_name = text(field.value.as_bytes()),
                F_WORLD_TYPE => world_type = text(field.value.as_bytes()),
                F_DEFAULT_SPAWN => {
                    let mut pos = [0i32; 3];
                    for f in ProtoReader::new(field.value.as_bytes().unwrap_or_default()) {
                        let f = f?;
                        if (1..=3).contains(&f.number) {
                            pos[f.number as usize - 1] =
                                f.value.as_u64().unwrap_or(0) as i64 as i32;
                        }
                    }
                    default_spawn = Some(pos);
                }
                _ => {}
            }
        }
        Ok(Self {
            raw,
            map_size,
            world_name,
            world_type,
            default_spawn,
        })
    }

    pub fn to_bytes(&self) -> &[u8] {
        &self.raw
    }

    fn mod_data(&self, key: &str) -> Result<Option<&[u8]>> {
        for field in ProtoReader::new(&self.raw) {
            let field = field?;
            if field.number != F_MOD_DATA {
                continue;
            }
            let entry = field.value.as_bytes().unwrap_or_default();
            let mut name = None;
            let mut value = None;
            for f in ProtoReader::new(entry) {
                let f = f?;
                match f.number {
                    1 => name = f.value.as_bytes(),
                    2 => value = f.value.as_bytes(),
                    _ => {}
                }
            }
            if name == Some(key.as_bytes()) {
                return Ok(Some(value.unwrap_or_default()));
            }
        }
        Ok(None)
    }

    /// Table code VS complet (`game:rock-granite`) vers ID numérique du monde.
    pub fn block_ids(&self) -> Result<HashMap<String, u32>> {
        let data = self
            .mod_data("BlockIDs")?
            .ok_or_else(|| VsError::Corrupt("table BlockIDs absente du gamedata".into()))?;
        let mut ids = HashMap::new();
        for entry in ProtoReader::new(data) {
            let entry = entry?;
            let mut id = 0u32;
            let mut code = None;
            for f in ProtoReader::new(entry.value.as_bytes().unwrap_or_default()) {
                let f = f?;
                match f.number {
                    1 => id = f.value.as_u64().unwrap_or(0) as u32,
                    2 => code = Some(text(f.value.as_bytes())),
                    _ => {}
                }
            }
            if let Some(code) = code {
                // Le domaine `game:` est omis dans la sauvegarde.
                let code = if code.contains(':') {
                    code
                } else {
                    format!("game:{code}")
                };
                ids.insert(code, id);
            }
        }
        Ok(ids)
    }

    /// Remplace les champs donnés et recopie les autres à l'identique.
    fn rewrite(&mut self, replaced: &[u32], append: impl FnOnce(&mut ProtoWriter)) -> Result<()> {
        let mut w = ProtoWriter::new();
        for field in ProtoReader::new(&self.raw) {
            let field = field?;
            if !replaced.contains(&field.number) {
                w.raw(field.raw);
            }
        }
        append(&mut w);
        self.raw = w.into_bytes();
        Ok(())
    }

    pub fn set_default_spawn(&mut self, x: i32, y: i32, z: i32) -> Result<()> {
        self.rewrite(&[F_DEFAULT_SPAWN], |w| {
            w.message(F_DEFAULT_SPAWN, |m| {
                m.int32(1, x).int32(2, y).int32(3, z).int32(7, -1);
            });
        })
    }

    pub fn set_world_name(&mut self, name: &str) -> Result<()> {
        self.rewrite(&[F_WORLD_NAME], |w| {
            w.string(F_WORLD_NAME, name);
        })?;
        self.world_name = name.to_string();
        Ok(())
    }
}

fn text(bytes: Option<&[u8]>) -> String {
    String::from_utf8_lossy(bytes.unwrap_or_default()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut w = ProtoWriter::new();
        w.uint(1, 1024000).uint(2, 384).uint(3, 1024000);
        w.message(F_MOD_DATA, |e| {
            let mut ids = ProtoWriter::new();
            ids.message(1, |m| {
                m.uint(1, 0).string(2, "air");
            });
            ids.message(1, |m| {
                m.uint(1, 4605).string(2, "rock-granite");
            });
            ids.message(1, |m| {
                m.uint(1, 9000).string(2, "monmod:truc");
            });
            e.string(1, "BlockIDs").bytes(2, &ids.into_bytes());
        });
        w.string(F_WORLD_NAME, "flat");
        w.into_bytes()
    }

    #[test]
    fn reads_block_ids_and_sizes() {
        let save = SaveGame::parse(sample()).unwrap();
        assert_eq!(save.map_size, [1024000, 384, 1024000]);
        let ids = save.block_ids().unwrap();
        assert_eq!(ids["game:air"], 0);
        assert_eq!(ids["game:rock-granite"], 4605);
        assert_eq!(ids["monmod:truc"], 9000);
    }

    #[test]
    fn rewrites_spawn_and_name_without_losing_fields() {
        let mut save = SaveGame::parse(sample()).unwrap();
        save.set_default_spawn(512000, 120, 512000).unwrap();
        save.set_default_spawn(512001, 121, 512001).unwrap();
        save.set_world_name("Odasio").unwrap();
        let reparsed = SaveGame::parse(save.to_bytes().to_vec()).unwrap();
        assert_eq!(reparsed.default_spawn, Some([512001, 121, 512001]));
        assert_eq!(reparsed.world_name, "Odasio");
        assert_eq!(reparsed.block_ids().unwrap().len(), 3);
        let spawns = ProtoReader::new(save.to_bytes())
            .filter(|f| f.as_ref().unwrap().number == F_DEFAULT_SPAWN)
            .count();
        assert_eq!(spawns, 1);
    }
}
