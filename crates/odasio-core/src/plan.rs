//! Plan de conversion : ce que deviendra chaque bloc présent dans le monde.
//!
//! C'est le modèle de l'assistant de mapping : seuls les blocs réellement présents comptent,
//! regroupés par qualité puis par catégorie et triés par volume.

use std::collections::BTreeMap;

use odasio_mc::BlockState;

use crate::inventory::Inventory;
use crate::mapping::{Category, Mapping, Quality, Resolved};

#[derive(Debug, Clone)]
pub struct PlanEntry {
    pub state: BlockState,
    pub count: u64,
    pub resolved: Resolved,
}

#[derive(Debug, Clone, Default)]
pub struct Group {
    pub entries: Vec<PlanEntry>,
    pub blocks: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ConversionPlan {
    pub groups: BTreeMap<(Quality, Category), Group>,
    pub total_blocks: u64,
}

impl ConversionPlan {
    pub fn build(mapping: &Mapping, inventory: &Inventory) -> Self {
        let mut plan = ConversionPlan::default();
        for (state, count) in inventory.sorted() {
            let resolved = mapping.resolve(state);
            let group = plan
                .groups
                .entry((resolved.quality, resolved.category))
                .or_default();
            group.blocks += count;
            group.entries.push(PlanEntry {
                state: state.clone(),
                count,
                resolved,
            });
            plan.total_blocks += count;
        }
        plan
    }

    /// Volume de blocs par qualité.
    pub fn blocks_by_quality(&self) -> BTreeMap<Quality, u64> {
        let mut out = BTreeMap::new();
        for ((quality, _), group) in &self.groups {
            *out.entry(*quality).or_default() += group.blocks;
        }
        out
    }

    /// Nombre d'états distincts par qualité.
    pub fn states_by_quality(&self) -> BTreeMap<Quality, usize> {
        let mut out = BTreeMap::new();
        for ((quality, _), group) in &self.groups {
            *out.entry(*quality).or_default() += group.entries.len();
        }
        out
    }

    /// Entrées qui méritent une décision humaine : ni exactes ni équivalentes,
    /// et non tranchées par une politique ou une surcharge.
    pub fn needs_review(&self) -> impl Iterator<Item = &PlanEntry> {
        self.groups
            .iter()
            .filter(|((q, _), _)| *q > Quality::Equivalent)
            .flat_map(|(_, g)| &g.entries)
            .filter(|e| !e.resolved.policy_applied)
    }
}
