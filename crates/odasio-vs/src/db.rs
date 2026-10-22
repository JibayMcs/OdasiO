//! Base SQLite d'une sauvegarde VS (`.vcdbs`). À n'écrire que jeu et serveur arrêtés.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::column::EncodedColumn;
use crate::layer;
use crate::proto::ProtoReader;
use crate::savegame::SaveGame;
use crate::tree::BlockEntity;
use crate::{Result, VsError};

/// Clé de position des tables `chunk`, `mapchunk` et `mapregion` (dimension 0).
/// `x` et `z` ne sont pas masqués par le jeu : ils doivent tenir sur 22 bits.
pub fn position_key(x: u32, y: u32, z: u32) -> i64 {
    debug_assert!(x < 1 << 22 && z < 1 << 22 && y < 1024);
    (x as u64 | (z as u64) << 27 | (y as u64) << 54) as i64
}

/// Colonne relue depuis une sauvegarde.
pub struct DecodedColumn {
    pub cx: u32,
    pub cz: u32,
    /// 32 768 IDs de blocs par chunk, `None` si le chunk est vide.
    pub blocks: Vec<Option<Vec<u32>>>,
    pub fluids: Vec<Option<Vec<u32>>>,
    pub block_entities: Vec<BlockEntity>,
}

fn non_empty(cells: Vec<u32>) -> Option<Vec<u32>> {
    cells.iter().any(|&v| v != 0).then_some(cells)
}

pub struct VsDatabase {
    conn: Connection,
    pub save: SaveGame,
}

impl VsDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(VsError::Corrupt(format!(
                "sauvegarde introuvable : {}",
                path.display()
            )));
        }
        let conn = Connection::open(path)?;
        let raw: Vec<u8> = conn
            .query_row("SELECT data FROM gamedata WHERE savegameid = 1", [], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| VsError::Corrupt("table gamedata vide".into()))?;
        Ok(Self {
            conn,
            save: SaveGame::parse(raw)?,
        })
    }

    pub fn read_chunk(&self, cx: u32, cy: u32, cz: u32) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT data FROM chunk WHERE position = ?1",
                [position_key(cx, cy, cz)],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn read_map_chunk(&self, cx: u32, cz: u32) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT data FROM mapchunk WHERE position = ?1",
                [position_key(cx, 0, cz)],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Toutes les colonnes du monde : (cx, cz, blob du map chunk).
    pub fn map_chunks(&self) -> Result<Vec<(u32, u32, Vec<u8>)>> {
        let mut stmt = self.conn.prepare("SELECT position, data FROM mapchunk")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (key, data) = row?;
            let key = key as u64;
            out.push((
                (key & 0x3F_FFFF) as u32,
                ((key >> 27) & 0x3F_FFFF) as u32,
                data,
            ));
        }
        Ok(out)
    }

    /// Lit une colonne : blocs et fluides par `cy` (`None` = vide) et block entities.
    /// `None` si la colonne n'existe pas dans la sauvegarde.
    pub fn read_column(&self, cx: u32, cz: u32) -> Result<Option<DecodedColumn>> {
        let height = (self.save.map_size[1] / 32) as usize;
        let mut column = DecodedColumn {
            cx,
            cz,
            blocks: vec![None; height],
            fluids: vec![None; height],
            block_entities: Vec::new(),
        };
        let mut found = false;
        for cy in 0..height {
            let Some(blob) = self.read_chunk(cx, cy as u32, cz)? else {
                continue;
            };
            found = true;
            for field in ProtoReader::new(&blob) {
                let field = field?;
                let bytes = field.value.as_bytes().unwrap_or_default();
                match field.number {
                    1 => column.blocks[cy] = non_empty(layer::decode(bytes)?),
                    16 => column.fluids[cy] = non_empty(layer::decode(bytes)?),
                    8 => column.block_entities.extend(BlockEntity::from_bytes(bytes)),
                    _ => {}
                }
            }
        }
        Ok(found.then_some(column))
    }

    pub fn chunk_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM chunk", [], |r| r.get(0))?)
    }

    /// Supprime le terrain du monde modèle (chunks et map chunks). Les map regions
    /// (climat, gisements) sont conservées.
    pub fn clear_terrain(&mut self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM chunk; DELETE FROM mapchunk;")?;
        Ok(())
    }

    /// Écrit des colonnes en une transaction.
    pub fn write_columns(&mut self, columns: &[EncodedColumn]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut chunk =
                tx.prepare_cached("INSERT OR REPLACE INTO chunk (position, data) VALUES (?1, ?2)")?;
            let mut map = tx.prepare_cached(
                "INSERT OR REPLACE INTO mapchunk (position, data) VALUES (?1, ?2)",
            )?;
            for column in columns {
                for (cy, blob) in column.chunks.iter().enumerate() {
                    chunk.execute(params![position_key(column.cx, cy as u32, column.cz), blob])?;
                }
                map.execute(params![
                    position_key(column.cx, 0, column.cz),
                    column.map_chunk
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn save_gamedata(&mut self) -> Result<()> {
        self.conn.execute(
            "UPDATE gamedata SET data = ?1 WHERE savegameid = 1",
            params![self.save.to_bytes()],
        )?;
        Ok(())
    }

    /// Termine proprement : rapatrie le journal WAL dans le fichier principal.
    pub fn close(self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        self.conn.close().map_err(|(_, e)| e)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_match_the_game() {
        // Valeurs relevées dans un monde généré par le jeu.
        assert_eq!(position_key(15993, 0, 15993), 2146544139897);
        assert_eq!(position_key(998, 0, 998), 133949293542);
    }
}
