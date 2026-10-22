//! Inventaire des blocs présents dans une dimension d'un monde Minecraft.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use odasio_mc::{BlockState, Chunk, McError, Region};
use rayon::prelude::*;

#[derive(Debug, Clone, Default)]
pub struct Inventory {
    /// Nombre de blocs par état.
    pub counts: HashMap<BlockState, u64>,
    pub regions: usize,
    pub chunks: usize,
    /// Chunks ignorés car incomplets (bordure de zone générée).
    pub incomplete_chunks: usize,
    /// Chunks dans un format non supporté, par DataVersion.
    pub unsupported: BTreeMap<i32, usize>,
    pub errors: usize,
    /// Plus bas et plus haut Y Minecraft occupés par un bloc autre que l'air.
    pub y_range: Option<(i32, i32)>,
}

impl Inventory {
    /// États triés par volume décroissant.
    pub fn sorted(&self) -> Vec<(&BlockState, u64)> {
        let mut sorted: Vec<_> = self.counts.iter().map(|(s, n)| (s, *n)).collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        sorted
    }

    pub fn total_blocks(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Analyse des fichiers région en parallèle. `progress` reçoit le nombre de régions traitées.
    pub fn scan(region_files: &[impl AsRef<Path> + Sync], progress: impl Fn(usize) + Sync) -> Self {
        let done = AtomicUsize::new(0);
        let unsupported: Mutex<BTreeMap<i32, usize>> = Mutex::default();
        let mut inventory = region_files
            .par_iter()
            .map(|file| {
                let mut inv = Inventory {
                    regions: 1,
                    ..Default::default()
                };
                match Region::open(file.as_ref()) {
                    Ok(mut region) => {
                        let locations: Vec<_> = region.chunks().collect();
                        for loc in locations {
                            let chunk = region
                                .read_chunk_nbt(loc)
                                .and_then(|nbt| nbt.map(|b| Chunk::from_nbt(&b)).transpose());
                            match chunk {
                                Ok(Some(chunk)) if chunk.complete => {
                                    inv.chunks += 1;
                                    count_blocks(&chunk, &mut inv.counts);
                                    inv.y_range = merge_range(inv.y_range, y_range(&chunk));
                                }
                                Ok(Some(_)) => inv.incomplete_chunks += 1,
                                Ok(None) => {}
                                Err(McError::UnsupportedVersion(v)) => {
                                    *unsupported.lock().unwrap().entry(v).or_default() += 1;
                                }
                                Err(e) => {
                                    if inv.errors < 3 {
                                        tracing::warn!("chunk {},{} : {e}", loc.x, loc.z);
                                    }
                                    inv.errors += 1;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("{} : {e}", file.as_ref().display());
                        inv.errors += 1;
                    }
                }
                progress(done.fetch_add(1, Ordering::Relaxed) + 1);
                inv
            })
            .reduce(Inventory::default, Inventory::merge);
        inventory.unsupported = unsupported.into_inner().unwrap();
        inventory
    }

    fn merge(mut self, other: Inventory) -> Inventory {
        for (k, v) in other.counts {
            *self.counts.entry(k).or_default() += v;
        }
        self.regions += other.regions;
        self.chunks += other.chunks;
        self.incomplete_chunks += other.incomplete_chunks;
        self.errors += other.errors;
        self.y_range = merge_range(self.y_range, other.y_range);
        self
    }
}

fn merge_range(a: Option<(i32, i32)>, b: Option<(i32, i32)>) -> Option<(i32, i32)> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
        (a, b) => a.or(b),
    }
}

/// Plage verticale occupée d'un chunk, au bloc près.
fn y_range(chunk: &Chunk) -> Option<(i32, i32)> {
    let occupied = |section: &odasio_mc::Section, layer: usize| {
        (0..256).any(|i| !section.block(i % 16, layer, i / 16).is_air())
    };
    let sections: Vec<_> = chunk.sections.iter().filter(|s| !s.is_empty()).collect();
    let low = sections.iter().find_map(|s| {
        (0..16)
            .find(|&layer| occupied(s, layer))
            .map(|layer| s.y as i32 * 16 + layer as i32)
    })?;
    let high = sections.iter().rev().find_map(|s| {
        (0..16)
            .rev()
            .find(|&layer| occupied(s, layer))
            .map(|layer| s.y as i32 * 16 + layer as i32)
    })?;
    Some((low, high))
}

fn count_blocks(chunk: &Chunk, counts: &mut HashMap<BlockState, u64>) {
    for section in &chunk.sections {
        match &section.indices {
            None => *counts.entry(section.palette[0].clone()).or_default() += 4096,
            Some(indices) => {
                let mut local = vec![0u64; section.palette.len()];
                for &i in indices.iter() {
                    local[i as usize] += 1;
                }
                for (state, n) in section.palette.iter().zip(local) {
                    if n > 0 {
                        *counts.entry(state.clone()).or_default() += n;
                    }
                }
            }
        }
    }
}
