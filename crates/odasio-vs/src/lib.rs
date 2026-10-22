//! Lecture et écriture des sauvegardes Vintage Story (`.vcdbs`).
//! Format décrit dans `docs/vs-save-format.md`.

use thiserror::Error;

/// Version du jeu ciblée.
pub const TARGET_GAME_VERSION: &str = "1.22.7";

#[derive(Debug, Error)]
pub enum VsError {
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, VsError>;
