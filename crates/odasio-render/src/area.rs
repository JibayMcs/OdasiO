//! Extraction d'une zone carrée d'un monde Minecraft pour la prévisualisation.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use odasio_mc::{BlockState, Chunk, ChunkLocation, Dimension, Region};

use crate::iso::Scene;

/// Blocs d'une zone : une scène dont les cellules indexent `palette` (0 = air).
#[derive(Debug, Clone)]
pub struct Area {
    /// Coin nord-ouest de la zone et altitude de la couche la plus basse.
    pub x0: i32,
    pub z0: i32,
    pub bottom: i32,
    pub scene: Scene,
    pub palette: Vec<BlockState>,
}

impl Area {
    /// Lit la zone de `size` blocs de côté centrée sur (`cx`, `cz`), de `depth` blocs sous la
    /// surface la plus basse jusqu'au plus haut bloc.
    pub fn load(
        dim: &Dimension,
        cx: i32,
        cz: i32,
        size: i32,
        depth: i32,
    ) -> odasio_mc::Result<Option<Self>> {
        let (x0, z0) = (cx - size / 2, cz - size / 2);
        let mut regions: HashMap<(i32, i32), Region> = HashMap::new();
        let mut palette = vec![BlockState::new("minecraft:air")];
        let mut index: HashMap<BlockState, u32> = HashMap::new();
        let mut blocks: Vec<([i32; 3], u32)> = Vec::new();
        for chunk_x in x0.div_euclid(16)..=(x0 + size - 1).div_euclid(16) {
            for chunk_z in z0.div_euclid(16)..=(z0 + size - 1).div_euclid(16) {
                let key = (chunk_x.div_euclid(32), chunk_z.div_euclid(32));
                let path = dim.region_dir.join(format!("r.{}.{}.mca", key.0, key.1));
                if !path.is_file() {
                    continue;
                }
                let region = match regions.entry(key) {
                    Entry::Occupied(e) => e.into_mut(),
                    Entry::Vacant(e) => e.insert(Region::open(&path)?),
                };
                let Some(nbt) = region.read_chunk_nbt(ChunkLocation {
                    x: chunk_x,
                    z: chunk_z,
                })?
                else {
                    continue;
                };
                let chunk = Chunk::from_nbt(&nbt)?;
                for section in &chunk.sections {
                    for i in 0..4096 {
                        let (lx, ly, lz) = (i & 15, i >> 8, (i >> 4) & 15);
                        let (x, z) = (chunk_x * 16 + lx as i32, chunk_z * 16 + lz as i32);
                        if !(x0..x0 + size).contains(&x) || !(z0..z0 + size).contains(&z) {
                            continue;
                        }
                        let state = section.block(lx, ly, lz);
                        if state.is_air() {
                            continue;
                        }
                        let id = *index.entry(state.clone()).or_insert_with(|| {
                            palette.push(state.clone());
                            (palette.len() - 1) as u32
                        });
                        blocks.push(([x, section.y as i32 * 16 + ly as i32, z], id));
                    }
                }
            }
        }
        let Some(top) = blocks.iter().map(|(p, _)| p[1]).max() else {
            return Ok(None);
        };
        let mut surface: HashMap<(i32, i32), i32> = HashMap::new();
        for (p, _) in &blocks {
            let e = surface.entry((p[0], p[2])).or_insert(p[1]);
            *e = (*e).max(p[1]);
        }
        let lowest = surface.values().min().copied().unwrap_or(top);
        let bottom = (lowest - depth).min(top);
        let mut scene = Scene::new(size as usize, (top - bottom + 1) as usize, size as usize);
        for (p, id) in blocks {
            if p[1] >= bottom {
                scene.set(
                    (p[0] - x0) as usize,
                    (p[1] - bottom) as usize,
                    (p[2] - z0) as usize,
                    id,
                );
            }
        }
        Ok(Some(Self {
            x0,
            z0,
            bottom,
            scene,
            palette,
        }))
    }
}
