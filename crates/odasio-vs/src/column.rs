//! Colonne de chunks 32x32 sur toute la hauteur du monde, prête à être écrite.

use crate::proto::ProtoWriter;
use crate::tree::BlockEntity;
use crate::{CHUNK_CELLS, CHUNK_SIZE, Result, TARGET_GAME_VERSION, layer};

/// Passe de génération VS (`EnumWorldGenPass`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WorldGenPass {
    /// Le jeu exécute la passe Végétation : en monde superflat, elle ne fait que calculer
    /// l'éclairage (soleil et sources de lumière), puis la colonne passe à Done.
    Vegetation = 3,
    /// Colonne chargée telle quelle.
    Done = 6,
}

/// Source de lumière à allumer par le jeu (`ScheduledBlockLightUpdates`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightSource {
    /// Position absolue du bloc.
    pub pos: [i32; 3],
    pub block_id: u32,
}

pub fn index3d(x: usize, y: usize, z: usize) -> usize {
    (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
}

type Cells = Box<[u32]>;

fn new_cells() -> Cells {
    vec![0u32; CHUNK_CELLS].into_boxed_slice()
}

pub struct Column {
    /// Coordonnées en chunks.
    pub cx: u32,
    pub cz: u32,
    blocks: Vec<Option<Cells>>,
    fluids: Vec<Option<Cells>>,
    pub light_sources: Vec<LightSource>,
    pub block_entities: Vec<BlockEntity>,
    /// Block entities à « poser » au premier chargement (`NewBlockEntities` du map chunk) :
    /// le jeu appelle leur `OnBlockPlaced`, par exemple pour lancer la croissance d'une pousse.
    pub placed_on_load: Vec<[i32; 3]>,
}

/// Blobs prêts pour la base : chunks par `cy` et map chunk.
pub struct EncodedColumn {
    pub cx: u32,
    pub cz: u32,
    pub chunks: Vec<Vec<u8>>,
    pub map_chunk: Vec<u8>,
    pub non_air: bool,
}

impl Column {
    pub fn new(cx: u32, cz: u32, height_chunks: usize) -> Self {
        Self {
            cx,
            cz,
            blocks: (0..height_chunks).map(|_| None).collect(),
            fluids: (0..height_chunks).map(|_| None).collect(),
            light_sources: Vec::new(),
            block_entities: Vec::new(),
            placed_on_load: Vec::new(),
        }
    }

    pub fn height(&self) -> usize {
        self.blocks.len() * CHUNK_SIZE
    }

    /// `x`, `z` locaux à la colonne (0..32), `y` absolu.
    #[inline]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, id: u32) {
        if id == 0 && self.blocks[y / CHUNK_SIZE].is_none() {
            return;
        }
        self.blocks[y / CHUNK_SIZE].get_or_insert_with(new_cells)[index3d(x, y % CHUNK_SIZE, z)] =
            id;
    }

    #[inline]
    pub fn set_fluid(&mut self, x: usize, y: usize, z: usize, id: u32) {
        if id == 0 && self.fluids[y / CHUNK_SIZE].is_none() {
            return;
        }
        self.fluids[y / CHUNK_SIZE].get_or_insert_with(new_cells)[index3d(x, y % CHUNK_SIZE, z)] =
            id;
    }

    pub fn block(&self, x: usize, y: usize, z: usize) -> u32 {
        self.blocks[y / CHUNK_SIZE]
            .as_ref()
            .map_or(0, |c| c[index3d(x, y % CHUNK_SIZE, z)])
    }

    pub fn fluid(&self, x: usize, y: usize, z: usize) -> u32 {
        self.fluids[y / CHUNK_SIZE]
            .as_ref()
            .map_or(0, |c| c[index3d(x, y % CHUNK_SIZE, z)])
    }

    /// Plus haut bloc solide ou fluide par (x, z), ordre `z * 32 + x`.
    fn height_map(&self) -> Vec<u16> {
        let mut map = vec![0u16; CHUNK_SIZE * CHUNK_SIZE];
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let top = (0..self.height())
                    .rev()
                    .find(|&y| self.block(x, y, z) != 0 || self.fluid(x, y, z) != 0);
                map[z * CHUNK_SIZE + x] = top.unwrap_or(0) as u16;
            }
        }
        map
    }

    pub fn encode(&self, pass: WorldGenPass, top_rock_id: u32) -> Result<EncodedColumn> {
        let empty = vec![0u32; CHUNK_CELLS];
        let mut chunks = Vec::with_capacity(self.blocks.len());
        let mut non_air = false;
        for (cy, (blocks, fluids)) in self.blocks.iter().zip(&self.fluids).enumerate() {
            let blocks = blocks.as_deref().unwrap_or(&empty);
            let fluids = fluids.as_deref().unwrap_or(&empty);
            let is_empty = blocks.iter().all(|&v| v == 0) && fluids.iter().all(|&v| v == 0);
            non_air |= !is_empty;

            let mut w = ProtoWriter::new();
            w.bytes(1, &layer::encode(blocks)?)
                // Lumière vide : calculée par le jeu (passe Végétation) ou absente.
                .bytes(2, &[])
                .bytes(3, &[])
                .string(12, TARGET_GAME_VERSION)
                .uint(15, 2)
                .bytes(16, &layer::encode(fluids)?);
            if is_empty {
                w.bool(13, true);
            }
            // Une block entity vit dans le chunk qui contient sa position.
            let entities: Vec<&BlockEntity> = self
                .block_entities
                .iter()
                .filter(|be| be.pos[1].div_euclid(CHUNK_SIZE as i32) == cy as i32)
                .collect();
            if !entities.is_empty() {
                w.uint(7, entities.len() as u64);
                for be in entities {
                    w.bytes(8, &be.to_bytes());
                }
            }
            chunks.push(w.into_bytes());
        }

        let heights = self.height_map();
        let y_max = heights.iter().copied().max().unwrap_or(0);
        let mut w = ProtoWriter::new();
        for &h in &heights {
            w.uint(3, h as u64);
        }
        w.uint(4, pass as u64);
        for &h in &heights {
            w.uint(7, h as u64);
        }
        w.uint(10, y_max as u64);
        for _ in 0..heights.len() {
            w.uint(13, top_rock_id as u64);
        }
        w.uint(16, 3);
        for pos in &self.placed_on_load {
            w.message(9, |m| {
                m.int32(1, pos[0]).int32(2, pos[1]).int32(3, pos[2]);
            });
        }
        for source in &self.light_sources {
            w.message(17, |m| {
                m.int32(1, source.pos[0])
                    .int32(2, source.pos[1])
                    .int32(3, source.pos[2])
                    .int32(4, source.block_id as i32);
            });
        }

        Ok(EncodedColumn {
            cx: self.cx,
            cz: self.cz,
            chunks,
            map_chunk: w.into_bytes(),
            non_air,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::ProtoReader;

    #[test]
    fn encodes_every_chunk_of_the_column() {
        let mut column = Column::new(16000, 16000, 12);
        for x in 0..32 {
            for z in 0..32 {
                column.set_block(x, 0, z, 4605);
            }
        }
        column.set_block(5, 40, 5, 4605);
        column.set_fluid(10, 1, 10, 7416);
        let encoded = column.encode(WorldGenPass::Vegetation, 4605).unwrap();
        assert_eq!(encoded.chunks.len(), 12);

        let fields: Vec<_> = ProtoReader::new(&encoded.chunks[0])
            .collect::<Result<_>>()
            .unwrap();
        let blocks = fields.iter().find(|f| f.number == 1).unwrap();
        let cells = layer::decode(blocks.value.as_bytes().unwrap()).unwrap();
        assert_eq!(cells[index3d(3, 0, 7)], 4605);
        let fluids = fields.iter().find(|f| f.number == 16).unwrap();
        let cells = layer::decode(fluids.value.as_bytes().unwrap()).unwrap();
        assert_eq!(cells[index3d(10, 1, 10)], 7416);
        assert!(
            fields
                .iter()
                .any(|f| f.number == 15 && f.value.as_u64() == Some(2))
        );

        let map: Vec<_> = ProtoReader::new(&encoded.map_chunk)
            .collect::<Result<_>>()
            .unwrap();
        let y_max = map.iter().find(|f| f.number == 10).unwrap();
        assert_eq!(y_max.value.as_u64(), Some(40));
        assert_eq!(map.iter().filter(|f| f.number == 3).count(), 1024);
    }
}
