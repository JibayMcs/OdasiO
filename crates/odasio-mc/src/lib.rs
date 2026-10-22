//! Lecture des mondes Minecraft Java Edition : `level.dat`, régions Anvil, sections à palette.
//!
//! Le décodage des chunks est versionné par `DataVersion` (voir [`chunk::ChunkFormat`]) :
//! formats 1.18+, 1.13 à 1.17 et antérieurs à 1.13 (identifiants numériques).

pub mod block;
pub mod block_entity;
pub mod chunk;
pub mod legacy;
pub mod level;
pub mod region;
pub mod world;

pub use block::BlockState;
pub use block_entity::{McBlockEntity, SignText};
pub use chunk::{Chunk, ChunkFormat, Section};
pub use level::LevelInfo;
pub use region::{ChunkLocation, Region};
pub use world::{Dimension, World};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum McError {
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
    #[error("NBT invalide : {0}")]
    Nbt(#[from] fastnbt::error::Error),
    #[error("compression de chunk non supportée : {0}")]
    UnsupportedCompression(u8),
    #[error("format de chunk non supporté (DataVersion {0})")]
    UnsupportedVersion(i32),
    #[error("données corrompues : {0}")]
    Corrupt(String),
    #[error("ce dossier n'est pas un monde Minecraft : {0}")]
    NotAWorld(String),
}

pub type Result<T> = std::result::Result<T, McError>;
