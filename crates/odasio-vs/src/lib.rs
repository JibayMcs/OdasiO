//! Lecture et écriture des sauvegardes Vintage Story (`.vcdbs`).
//! Format décrit dans `docs/vs-save-format.md`.

pub mod column;
pub mod db;
pub mod layer;
pub mod light;
pub mod proto;
pub mod savegame;
pub mod template;
pub mod tree;

pub use column::{Column, EncodedColumn, LightSource, WorldGenPass};
pub use db::{VsDatabase, position_key};
pub use light::{LightTables, light_columns};
pub use savegame::SaveGame;
pub use template::{TemplateOptions, VsInstall};
pub use tree::{Attr, BlockEntity, Tree};

use thiserror::Error;

/// Version du jeu ciblée.
pub const TARGET_GAME_VERSION: &str = "1.22.7";
/// Arête d'un chunk VS, en blocs.
pub const CHUNK_SIZE: usize = 32;
pub const CHUNK_CELLS: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
/// Taille de la carte par défaut (X et Z) ; son centre accueille le monde converti.
pub const DEFAULT_MAP_SIZE_XZ: u32 = 1_024_000;

#[derive(Debug, Error)]
pub enum VsError {
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON : {0}")]
    Json(#[from] serde_json::Error),
    #[error("données corrompues : {0}")]
    Corrupt(String),
    #[error("serveur Vintage Story : {0}")]
    Server(String),
}

pub type Result<T> = std::result::Result<T, VsError>;
