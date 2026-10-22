//! Compatibilité avec les anciennes versions : table des identifiants numériques
//! (avant 1.13) et renommages de blocs survenus depuis.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::block::BlockState;

/// Table "id:meta" vers état de bloc 1.13, issue de PrismarineJS minecraft-data (MIT).
static LEGACY_JSON: &str = include_str!("../data/legacy.json");

fn legacy_table() -> &'static HashMap<(u16, u16), BlockState> {
    static TABLE: OnceLock<HashMap<(u16, u16), BlockState>> = OnceLock::new();
    TABLE.get_or_init(|| {
        #[derive(serde::Deserialize)]
        struct Legacy {
            blocks: HashMap<String, String>,
        }
        let legacy: Legacy = serde_json::from_str(LEGACY_JSON).expect("legacy.json embarqué");
        legacy
            .blocks
            .into_iter()
            .filter_map(|(key, state)| {
                let (id, meta) = key.split_once(':')?;
                Some((
                    (id.parse().ok()?, meta.parse().ok()?),
                    BlockState::parse(&state),
                ))
            })
            .collect()
    })
}

/// État de bloc correspondant à un couple (id, meta) d'avant 1.13.
/// Un meta inconnu retombe sur la variante 0 ; un id inconnu (bloc de mod) donne
/// `legacy:<id>` pour rester visible dans les rapports.
pub fn block_state(id: u16, meta: u16) -> BlockState {
    let table = legacy_table();
    table
        .get(&(id, meta))
        .or_else(|| table.get(&(id, 0)))
        .cloned()
        .unwrap_or_else(|| BlockState::new(format!("legacy:{id}")))
}

/// Renommages : (ancien nom, DataVersion du renommage, nouveau nom).
/// Ne s'appliquent qu'aux chunks enregistrés avant le renommage.
const RENAMES: &[(&str, i32, &str)] = &[
    // 1.14 : les panneaux prennent une essence, l'ancienne dalle de pierre devient lisse.
    ("minecraft:sign", 1901, "minecraft:oak_sign"),
    ("minecraft:wall_sign", 1901, "minecraft:oak_wall_sign"),
    ("minecraft:stone_slab", 1901, "minecraft:smooth_stone_slab"),
    // 1.17
    ("minecraft:grass_path", 2724, "minecraft:dirt_path"),
    // 1.20.3
    ("minecraft:grass", 3679, "minecraft:short_grass"),
];

/// 20w06a (1.16) : les côtés des murs passent de `true/false` à `none/low/tall`.
const WALL_SIDES_SINCE: i32 = 2529;

/// Ramène un état de bloc au vocabulaire de la dernière version de Minecraft.
pub fn normalize(state: &mut BlockState, data_version: i32) {
    if let Some((old, _, new)) = RENAMES
        .iter()
        .find(|(old, since, _)| data_version < *since && state.name == *old)
    {
        debug_assert_eq!(state.name, *old);
        state.name = (*new).to_string();
    }
    if data_version < WALL_SIDES_SINCE && state.name.ends_with("_wall") {
        for side in ["north", "east", "south", "west"] {
            if let Some(value) = state.properties.get_mut(side) {
                *value = if value == "true" { "low" } else { "none" }.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_numeric_ids() {
        assert_eq!(block_state(1, 0).name, "minecraft:stone");
        assert_eq!(block_state(1, 1).name, "minecraft:granite");
        let log = block_state(17, 4);
        assert_eq!(log.name, "minecraft:oak_log");
        assert_eq!(log.property("axis"), Some("x"));
    }

    #[test]
    fn unknown_ids_stay_identifiable() {
        assert_eq!(block_state(3000, 0).name, "legacy:3000");
    }

    #[test]
    fn renames_only_older_chunks() {
        let mut grass = BlockState::new("minecraft:grass");
        normalize(&mut grass, 1343);
        assert_eq!(grass.name, "minecraft:short_grass");

        let mut wall =
            BlockState::parse("minecraft:cobblestone_wall[east=true,north=false,up=true]");
        normalize(&mut wall, 1343);
        assert_eq!(wall.property("east"), Some("low"));
        assert_eq!(wall.property("north"), Some("none"));
        assert_eq!(wall.property("up"), Some("true"));

        let mut slab = BlockState::new("minecraft:stone_slab");
        normalize(&mut slab, 3839);
        assert_eq!(slab.name, "minecraft:stone_slab");
    }
}
