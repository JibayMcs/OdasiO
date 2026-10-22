use std::path::{Path, PathBuf};

use crate::level::LevelInfo;
use crate::region::Region;
use crate::{McError, Result};

/// Dimension d'un monde (Overworld, Nether, End ou dimension de datapack / mod).
#[derive(Debug, Clone)]
pub struct Dimension {
    /// Identifiant namespacé (`minecraft:overworld`, `monmod:ciel`...).
    pub id: String,
    pub region_dir: PathBuf,
}

impl Dimension {
    pub fn region_files(&self) -> Result<Vec<PathBuf>> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.region_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| Region::coords_from_name(p).is_some())
            .collect();
        files.sort();
        Ok(files)
    }
}

pub struct World {
    pub root: PathBuf,
    pub level: LevelInfo,
    pub dimensions: Vec<Dimension>,
}

impl World {
    pub fn open(root: &Path) -> Result<Self> {
        let level_path = root.join("level.dat");
        if !level_path.is_file() {
            return Err(McError::NotAWorld(root.display().to_string()));
        }
        let level = LevelInfo::read(&level_path)?;
        Ok(Self {
            root: root.to_path_buf(),
            level,
            dimensions: discover_dimensions(root),
        })
    }

    pub fn dimension(&self, id: &str) -> Option<&Dimension> {
        self.dimensions.iter().find(|d| d.id == id)
    }

    pub fn overworld(&self) -> Option<&Dimension> {
        self.dimension("minecraft:overworld")
    }
}

/// Emplacements des régions : historiques (`region`, `DIM-1`, `DIM1`) et
/// `dimensions/<namespace>/<nom>/region` (dimensions personnalisées, et l'Overworld
/// pour certains outils comme WorldPainter).
fn discover_dimensions(root: &Path) -> Vec<Dimension> {
    let mut dims = Vec::new();
    let mut push = |id: String, dir: PathBuf| {
        if dir.is_dir() && !dims.iter().any(|d: &Dimension| d.id == id) {
            dims.push(Dimension {
                id,
                region_dir: dir,
            });
        }
    };
    push("minecraft:overworld".into(), root.join("region"));
    push("minecraft:the_nether".into(), root.join("DIM-1/region"));
    push("minecraft:the_end".into(), root.join("DIM1/region"));

    if let Ok(namespaces) = std::fs::read_dir(root.join("dimensions")) {
        for ns in namespaces.filter_map(|e| e.ok()) {
            let Ok(names) = std::fs::read_dir(ns.path()) else {
                continue;
            };
            for name in names.filter_map(|e| e.ok()) {
                let id = format!(
                    "{}:{}",
                    ns.file_name().to_string_lossy(),
                    name.file_name().to_string_lossy()
                );
                push(id, name.path().join("region"));
            }
        }
    }
    dims
}
