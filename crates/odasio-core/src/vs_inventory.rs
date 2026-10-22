//! Inventaire des blocs d'une sauvegarde Vintage Story (sens VS vers Minecraft).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use odasio_vs::VsDatabase;
use rayon::prelude::*;

use crate::convert::Result;
use crate::mapping::Quality;
use crate::reverse::{ReverseMapping, ReverseResolved};

#[derive(Debug, Clone, Default)]
pub struct VsInventory {
    pub world_name: String,
    pub world_type: String,
    pub map_size_y: u32,
    pub columns: usize,
    /// Nombre de blocs (couches solide et fluide) par code VS.
    pub counts: HashMap<String, u64>,
}

impl VsInventory {
    /// Analyse en parallèle ; `progress` reçoit (colonnes traitées, total).
    pub fn scan(save: &Path, progress: impl Fn(usize, usize) + Sync) -> Result<Self> {
        let db = VsDatabase::open(save)?;
        let ids = db.save.block_ids()?;
        let size = ids.values().copied().max().unwrap_or(0) as usize + 1;
        let mut codes = vec![String::new(); size];
        for (code, &id) in &ids {
            codes[id as usize] = code.clone();
        }
        let columns: Vec<(u32, u32)> = db
            .map_chunks()?
            .into_iter()
            .map(|(cx, cz, _)| (cx, cz))
            .collect();
        let total = columns.len();
        let done = AtomicUsize::new(0);

        let counts = columns
            .par_chunks(256)
            .map(|batch| -> Result<Vec<u64>> {
                let db = VsDatabase::open(save)?;
                let mut counts = vec![0u64; size];
                for &(cx, cz) in batch {
                    if let Some(column) = db.read_column(cx, cz)? {
                        for layer in column.blocks.iter().chain(&column.fluids).flatten() {
                            for &id in layer {
                                if let Some(n) = counts.get_mut(id as usize) {
                                    *n += 1;
                                }
                            }
                        }
                    }
                }
                let done = done.fetch_add(batch.len(), Ordering::Relaxed) + batch.len();
                progress(done, total);
                Ok(counts)
            })
            .try_reduce(
                || vec![0u64; size],
                |mut a, b| {
                    a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
                    Ok(a)
                },
            )?;

        Ok(Self {
            world_name: db.save.world_name.clone(),
            world_type: db.save.world_type.clone(),
            map_size_y: db.save.map_size[1],
            columns: total,
            counts: counts
                .into_iter()
                .enumerate()
                // L'ID 0 est l'air : inutile dans l'inventaire.
                .filter(|&(id, n)| id != 0 && n > 0)
                .map(|(id, n)| (codes[id].clone(), n))
                .collect(),
        })
    }

    /// Correspondances des blocs présents, groupées par qualité et triées par volume.
    pub fn plan(
        &self,
        mapping: &ReverseMapping,
    ) -> BTreeMap<Quality, Vec<(String, u64, ReverseResolved)>> {
        let mut plan: BTreeMap<Quality, Vec<_>> = BTreeMap::new();
        for (code, &n) in &self.counts {
            let resolved = mapping.resolve(code);
            plan.entry(resolved.quality)
                .or_default()
                .push((code.clone(), n, resolved));
        }
        for entries in plan.values_mut() {
            entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        }
        plan
    }

    pub fn total_blocks(&self) -> u64 {
        self.counts.values().sum()
    }
}
