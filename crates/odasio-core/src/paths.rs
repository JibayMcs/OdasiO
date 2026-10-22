//! Emplacements par défaut des mondes Minecraft et des sauvegardes Vintage Story.

use std::path::{Path, PathBuf};

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn appdata() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

/// Dossier `saves` du lanceur Minecraft officiel.
pub fn minecraft_saves_dir() -> Option<PathBuf> {
    [
        appdata().map(|a| a.join(".minecraft/saves")),
        home().map(|h| h.join(".minecraft/saves")),
        home().map(|h| h.join(".var/app/com.mojang.Minecraft/.minecraft/saves")),
    ]
    .into_iter()
    .flatten()
    .find(|p| p.is_dir())
}

/// Dossier des sauvegardes de Vintage Story.
pub fn vs_saves_dir() -> Option<PathBuf> {
    match appdata() {
        Some(a) => Some(a.join("VintagestoryData/Saves")),
        None => home().map(|h| h.join(".config/VintagestoryData/Saves")),
    }
}

/// Mondes Minecraft (dossiers contenant `level.dat`) directement sous `dir`.
pub fn list_worlds(dir: &Path) -> Vec<PathBuf> {
    let mut worlds: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("level.dat").is_file())
        .collect();
    worlds.sort();
    worlds
}
