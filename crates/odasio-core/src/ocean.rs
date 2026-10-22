//! Océan généré par Odasio autour du monde converti (monde « océan »).
//!
//! Vintage Story garde toujours une île au point d'apparition, même sans terres : partout où
//! le monde Minecraft a des trous (chunks jamais générés), elle réapparaîtrait. Odasio remplit
//! donc lui-même les trous de la zone convertie et une marge autour avec de l'océan.

use std::collections::{HashMap, HashSet};

use odasio_vs::{CHUNK_SIZE, Column, EncodedColumn, LightTables, WorldGenPass, light_columns};

/// Marge d'océan autour de la zone convertie, en colonnes de 32 blocs.
pub const MARGIN_COLUMNS: u32 = 4;
/// Profondeur du fond marin sous la surface de l'eau.
const DEPTH: usize = 24;

/// Blocs d'une colonne d'océan, en IDs du monde.
#[derive(Debug, Clone, Copy)]
pub struct OceanBlocks {
    pub mantle: u32,
    pub rock: u32,
    pub seabed: u32,
    pub water: u32,
}

impl OceanBlocks {
    pub fn from_ids(ids: &HashMap<String, u32>, rock: u32) -> Self {
        let id = |code: &str| ids.get(code).copied();
        Self {
            mantle: id("game:mantle").unwrap_or(rock),
            rock,
            seabed: id("game:muddygravel").unwrap_or(rock),
            // L'eau salée raccorde avec les océans de Vintage Story.
            water: id("game:saltwater-still-7")
                .or_else(|| id("game:water-still-7"))
                .unwrap_or(0),
        }
    }
}

/// Remplit d'océan un rectangle de la colonne (`x0..x1`, `z0..z1` locaux).
pub fn fill_ocean(
    column: &mut Column,
    (x0, x1): (usize, usize),
    (z0, z1): (usize, usize),
    surface: usize,
    blocks: OceanBlocks,
) {
    let floor = surface.saturating_sub(DEPTH);
    for z in z0..z1 {
        for x in x0..x1 {
            column.set_block(x, 0, z, blocks.mantle);
            for y in 1..floor.saturating_sub(1) {
                column.set_block(x, y, z, blocks.rock);
            }
            for y in floor.saturating_sub(1)..=floor {
                column.set_block(x, y, z, blocks.seabed);
            }
            for y in floor + 1..=surface {
                column.set_fluid(x, y, z, blocks.water);
            }
        }
    }
}

fn ocean_column(
    cx: u32,
    cz: u32,
    height_chunks: usize,
    surface: usize,
    blocks: OceanBlocks,
) -> Column {
    let mut column = Column::new(cx, cz, height_chunks);
    fill_ocean(
        &mut column,
        (0, CHUNK_SIZE),
        (0, CHUNK_SIZE),
        surface,
        blocks,
    );
    column
}

/// Rayon, en colonnes, de l'îlot que Vintage Story garde au centre de la carte.
const SPAWN_ISLAND_RADIUS: i64 = 12;

/// Colonnes à remplir : trous et marge à moins de [`MARGIN_COLUMNS`] d'une colonne convertie,
/// plus les abords du centre de la carte où VS poserait son îlot. Plus loin, l'océan de
/// Vintage Story prend le relais.
pub fn holes(written: &HashSet<(u32, u32)>, center: (u32, u32)) -> Vec<(u32, u32)> {
    let margin = MARGIN_COLUMNS as i64;
    let mut holes = HashSet::new();
    for &(cx, cz) in written {
        for dz in -margin..=margin {
            for dx in -margin..=margin {
                let (x, z) = (cx as i64 + dx, cz as i64 + dz);
                if x >= 0 && z >= 0 && !written.contains(&(x as u32, z as u32)) {
                    holes.insert((x as u32, z as u32));
                }
            }
        }
    }
    let r = SPAWN_ISLAND_RADIUS;
    for dz in -r..=r {
        for dx in -r..=r {
            let key = ((center.0 as i64 + dx) as u32, (center.1 as i64 + dz) as u32);
            if !written.contains(&key) {
                holes.insert(key);
            }
        }
    }
    let mut holes: Vec<_> = holes.into_iter().collect();
    holes.sort_unstable();
    holes
}

/// Colonnes d'océan encodées. Elles sont toutes identiques à leur position près : une seule
/// est construite, éclairée et encodée, puis ses blobs sont réutilisés pour chaque trou.
/// `surface` : Y de la dernière couche d'eau.
pub fn ocean_columns(
    holes: &[(u32, u32)],
    height_chunks: usize,
    surface: usize,
    blocks: OceanBlocks,
    light: Option<&LightTables>,
    top_rock: u32,
) -> odasio_vs::Result<Vec<EncodedColumn>> {
    let mut template = vec![ocean_column(0, 0, height_chunks, surface, blocks)];
    let pass = match light {
        Some(tables) => {
            light_columns(&mut template, tables);
            WorldGenPass::Done
        }
        None => WorldGenPass::Vegetation,
    };
    let encoded = template[0].encode(pass, top_rock)?;
    Ok(holes
        .iter()
        .map(|&(cx, cz)| EncodedColumn {
            cx,
            cz,
            chunks: encoded.chunks.clone(),
            map_chunk: encoded.map_chunk.clone(),
            non_air: encoded.non_air,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_cover_gaps_margin_and_spawn_island() {
        let written: HashSet<_> = [(100, 100), (102, 100)].into_iter().collect();
        let holes = holes(&written, (300, 300));
        assert!(holes.contains(&(101, 100)), "trou entre deux colonnes");
        assert!(holes.contains(&(96, 96)), "coin de la marge");
        assert!(!holes.contains(&(100, 100)));
        assert!(!holes.contains(&(120, 100)), "trop loin de toute terre");
        assert!(holes.contains(&(300, 300)), "centre de la carte");
    }

    #[test]
    fn ocean_column_has_water_up_to_the_surface() {
        let blocks = OceanBlocks {
            mantle: 1,
            rock: 2,
            seabed: 3,
            water: 4,
        };
        let column = ocean_column(0, 0, 10, 137, blocks);
        assert_eq!(column.block(5, 0, 5), 1);
        assert_eq!(column.block(5, 137 - DEPTH, 5), 3);
        assert_eq!(column.fluid(5, 137, 5), 4);
        assert_eq!(column.fluid(5, 138, 5), 0);
        assert_eq!(column.top_y(), Some(137));
    }
}
