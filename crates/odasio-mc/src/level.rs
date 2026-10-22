use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use serde::Deserialize;

use crate::Result;

/// Informations générales du monde, lues dans `level.dat`.
#[derive(Debug, Clone)]
pub struct LevelInfo {
    pub name: String,
    pub data_version: i32,
    /// Nom de version lisible (`1.20.6`), absent sur les mondes anciens ou générés par des outils.
    pub version_name: Option<String>,
    pub spawn: Option<[i32; 3]>,
    /// Marques du serveur ayant ouvert le monde (`vanilla`, `forge`, `fabric`...).
    pub server_brands: Vec<String>,
}

impl LevelInfo {
    pub fn is_modded(&self) -> bool {
        self.server_brands
            .iter()
            .any(|b| matches!(b.as_str(), "forge" | "fabric" | "neoforge" | "quilt"))
    }
}

#[derive(Deserialize)]
struct Root {
    #[serde(rename = "Data")]
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "LevelName", default)]
    level_name: String,
    #[serde(rename = "DataVersion", default)]
    data_version: i32,
    #[serde(rename = "Version")]
    version: Option<Version>,
    #[serde(rename = "SpawnX")]
    spawn_x: Option<i32>,
    #[serde(rename = "SpawnY")]
    spawn_y: Option<i32>,
    #[serde(rename = "SpawnZ")]
    spawn_z: Option<i32>,
    #[serde(rename = "ServerBrands", default)]
    server_brands: Vec<String>,
}

#[derive(Deserialize)]
struct Version {
    #[serde(rename = "Name")]
    name: Option<String>,
}

impl LevelInfo {
    pub fn read(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        GzDecoder::new(std::fs::File::open(path)?).read_to_end(&mut bytes)?;
        let root: Root = fastnbt::from_bytes(&bytes)?;
        let d = root.data;
        let spawn = match (d.spawn_x, d.spawn_y, d.spawn_z) {
            (Some(x), Some(y), Some(z)) => Some([x, y, z]),
            _ => None,
        };
        Ok(Self {
            name: d.level_name,
            data_version: d.data_version,
            version_name: d.version.and_then(|v| v.name),
            spawn,
            server_brands: d.server_brands,
        })
    }
}
