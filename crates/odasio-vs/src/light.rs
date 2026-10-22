//! Éclairage calculé par Odasio, fidèle à `ChunkIlluminator` du jeu :
//! soleil vertical, diffusion du soleil, puis lumière des blocs.
//!
//! Une valeur de lumière occupe un `u32` : soleil (bits 0-4), intensité de la lumière de bloc
//! (5-9), teinte (10-15), saturation (16-18).

use std::collections::VecDeque;

use crate::column::{Column, index3d};
use crate::{CHUNK_CELLS, CHUNK_SIZE};

/// Intensité du soleil avec le mod de survie (`SurvivalConfig.SunBrightness`).
pub const SUN_BRIGHTNESS: u32 = 22;

/// Propriétés lumineuses des blocs, indexées par ID numérique du monde.
#[derive(Debug, Clone, Default)]
pub struct LightTables {
    /// `LightAbsorption` : 0 transparent, 99 opaque (valeur par défaut du jeu).
    pub absorption: Vec<u8>,
    /// `lightHsv` : teinte, saturation, intensité (0 = n'éclaire pas).
    pub emission: Vec<[u8; 3]>,
}

impl LightTables {
    fn absorption(&self, id: u32) -> u32 {
        self.absorption.get(id as usize).copied().unwrap_or(99) as u32
    }

    pub fn emission(&self, id: u32) -> Option<[u8; 3]> {
        self.emission
            .get(id as usize)
            .copied()
            .filter(|hsv| hsv[2] > 0)
    }
}

/// Lumière d'un chunk de 32x32x32.
#[derive(Debug, Clone)]
pub enum ChunkLight {
    /// Aucune lumière (roche profonde).
    Dark,
    /// Entièrement au-dessus du terrain : plein soleil, sans lumière de bloc.
    FullSun,
    Cells(Box<[u32]>),
}

impl ChunkLight {
    pub fn get(&self, index: usize) -> u32 {
        match self {
            ChunkLight::Dark => 0,
            ChunkLight::FullSun => SUN_BRIGHTNESS,
            ChunkLight::Cells(cells) => cells[index],
        }
    }

    fn cells(&mut self) -> &mut [u32] {
        if !matches!(self, ChunkLight::Cells(_)) {
            let fill = self.get(0);
            *self = ChunkLight::Cells(vec![fill; CHUNK_CELLS].into_boxed_slice());
        }
        match self {
            ChunkLight::Cells(cells) => cells,
            _ => unreachable!(),
        }
    }
}

const SUN_MASK: u32 = 0x1f;

/// Vue d'une région : colonnes voisines adressées en coordonnées locales à la région.
struct Region<'a> {
    columns: &'a mut [Column],
    /// Index dans `columns` par position relative au coin de la région (ligne par ligne).
    grid: Vec<Option<usize>>,
    grid_width: i64,
    grid_depth: i64,
    origin: (u32, u32),
    height: usize,
    tables: &'a LightTables,
    /// Surface (plus haut bloc occupé) par colonne, ordre `z * 32 + x`.
    surfaces: Vec<Vec<Option<u16>>>,
    /// Plus bas Y atteint par la descente verticale du soleil, par colonne et (x, z).
    sun_floor: Vec<Vec<u16>>,
}

impl Region<'_> {
    #[inline]
    fn column_at(&self, x: i64, z: i64) -> Option<usize> {
        if x < 0 || z < 0 {
            return None;
        }
        let (gx, gz) = (x / CHUNK_SIZE as i64, z / CHUNK_SIZE as i64);
        if gx >= self.grid_width || gz >= self.grid_depth {
            return None;
        }
        self.grid[(gz * self.grid_width + gx) as usize]
    }

    /// Cellule (colonne, chunk, index) d'une position locale, `None` hors région ou hauteur.
    fn locate(&self, x: i64, y: i64, z: i64) -> Option<(usize, usize, usize)> {
        if y < 0 || y >= self.height as i64 {
            return None;
        }
        let column = self.column_at(x, z)?;
        let (lx, ly, lz) = (
            (x % CHUNK_SIZE as i64) as usize,
            y as usize,
            (z % CHUNK_SIZE as i64) as usize,
        );
        Some((column, ly / CHUNK_SIZE, index3d(lx, ly % CHUNK_SIZE, lz)))
    }

    /// Surface à une position locale, `None` hors région.
    fn surface_at(&self, x: i64, z: i64) -> Option<i64> {
        let column = self.column_at(x, z)?;
        let local =
            (z % CHUNK_SIZE as i64) as usize * CHUNK_SIZE + (x % CHUNK_SIZE as i64) as usize;
        Some(self.surfaces[column][local].map_or(-1, |h| h as i64))
    }

    fn absorption(&self, (column, cy, index): (usize, usize, usize)) -> u32 {
        let col = &self.columns[column];
        let (block, fluid) = col.cell(cy, index);
        let solid = self.tables.absorption(block);
        if block == 0 {
            return self.tables.absorption(fluid);
        }
        if fluid == 0 {
            solid
        } else {
            solid.max(self.tables.absorption(fluid))
        }
    }

    fn light(&self, (column, cy, index): (usize, usize, usize)) -> u32 {
        self.columns[column].light[cy].get(index)
    }

    fn set_light(&mut self, (column, cy, index): (usize, usize, usize), value: u32) {
        self.columns[column].light[cy].cells()[index] = value;
    }
}

const NEIGHBOURS: [(i64, i64, i64); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Éclaire un ensemble de colonnes voisines (typiquement une région Minecraft) : la lumière
/// circule entre elles, pas au-delà.
pub fn light_columns(columns: &mut [Column], tables: &LightTables) {
    if columns.is_empty() {
        return;
    }
    let height = columns[0].height();
    let origin = (
        columns.iter().map(|c| c.cx).min().unwrap(),
        columns.iter().map(|c| c.cz).min().unwrap(),
    );
    let grid_width = (columns.iter().map(|c| c.cx).max().unwrap() - origin.0 + 1) as i64;
    let grid_depth = (columns.iter().map(|c| c.cz).max().unwrap() - origin.1 + 1) as i64;
    let mut grid = vec![None; (grid_width * grid_depth) as usize];
    for (i, c) in columns.iter().enumerate() {
        let (gx, gz) = ((c.cx - origin.0) as i64, (c.cz - origin.1) as i64);
        grid[(gz * grid_width + gx) as usize] = Some(i);
    }

    // Chunks au-dessus du terrain : plein soleil ; les autres sont calculés.
    for column in columns.iter_mut() {
        let top_chunk = column.top_y().map_or(0, |y| y / CHUNK_SIZE + 1);
        column.light = (0..height / CHUNK_SIZE)
            .map(|cy| {
                if cy >= top_chunk {
                    ChunkLight::FullSun
                } else {
                    ChunkLight::Dark
                }
            })
            .collect();
    }

    let surfaces = columns.iter().map(Column::surface).collect();
    let sun_floor = vec![vec![0; CHUNK_SIZE * CHUNK_SIZE]; columns.len()];
    let mut region = Region {
        sun_floor,
        surfaces,
        columns,
        grid,
        grid_width,
        grid_depth,
        origin,
        height,
        tables,
    };
    sunlight(&mut region);
    sunlight_flood(&mut region);
    block_light(&mut region);
}

/// Descente verticale du soleil, colonne par colonne.
fn sunlight(region: &mut Region) {
    for c in 0..region.columns.len() {
        let top = region.columns[c]
            .light
            .iter()
            .position(|l| matches!(l, ChunkLight::FullSun))
            .unwrap_or(region.height / CHUNK_SIZE)
            * CHUNK_SIZE;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let mut light = SUN_BRIGHTNESS;
                let mut floor = 0;
                for y in (0..top).rev() {
                    let cell = (c, y / CHUNK_SIZE, index3d(x, y % CHUNK_SIZE, z));
                    let absorption = region.absorption(cell);
                    // Le bloc reçoit la lumière avant d'en absorber une partie.
                    region.set_light(cell, light);
                    floor = y;
                    if absorption > light {
                        break;
                    }
                    light -= absorption;
                }
                region.sun_floor[c][z * CHUNK_SIZE + x] = floor as u16;
            }
        }
    }
}

/// Diffusion du soleil dans les six directions, d'une colonne à l'autre.
fn sunlight_flood(region: &mut Region) {
    let mut queue: VecDeque<(i64, i64, i64)> = VecDeque::new();
    for c in 0..region.columns.len() {
        let base = (
            (region.columns[c].cx - region.origin.0) as i64 * CHUNK_SIZE as i64,
            (region.columns[c].cz - region.origin.1) as i64 * CHUNK_SIZE as i64,
        );
        for z in 0..CHUNK_SIZE as i64 {
            for x in 0..CHUNK_SIZE as i64 {
                let (gx, gz) = (base.0 + x, base.1 + z);
                // Au-dessus de sa surface et de celles de ses voisines, tout est en plein
                // soleil : rien à diffuser. La ligne juste au-dessus alimente les cavités.
                let limit = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .filter_map(|(dx, dz)| region.surface_at(gx + dx, gz + dz))
                    .max()
                    .unwrap_or(-1)
                    + 1;
                // Sous le plancher du soleil, rien n'est encore éclairé : aucune graine.
                let floor = region.sun_floor[c][z as usize * CHUNK_SIZE + x as usize] as i64;
                for y in floor..=limit.min(region.height as i64 - 1) {
                    let y = y as usize;
                    let cell = region.locate(gx, y as i64, gz).unwrap();
                    let sun = (region.light(cell) & SUN_MASK) as i64;
                    if sun <= 1 {
                        continue;
                    }
                    let value = sun - region.absorption(cell) as i64 - 1;
                    if value <= 0 {
                        continue;
                    }
                    let lower_neighbour = NEIGHBOURS.iter().any(|(dx, dy, dz)| {
                        region
                            .locate(gx + dx, y as i64 + dy, gz + dz)
                            .is_some_and(|n| ((region.light(n) & SUN_MASK) as i64) < value)
                    });
                    if lower_neighbour {
                        queue.push_back((gx, y as i64, gz));
                    }
                }
            }
        }
    }

    while let Some((x, y, z)) = queue.pop_front() {
        let Some(cell) = region.locate(x, y, z) else {
            continue;
        };
        let value = (region.light(cell) & SUN_MASK) as i64 - region.absorption(cell) as i64 - 1;
        if value <= 0 {
            continue;
        }
        for (dx, dy, dz) in NEIGHBOURS {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            let Some(n) = region.locate(nx, ny, nz) else {
                continue;
            };
            let current = region.light(n);
            if ((current & SUN_MASK) as i64) < value {
                region.set_light(n, (current & !SUN_MASK) | value as u32);
                queue.push_back((nx, ny, nz));
            }
        }
    }
}

fn block_value(light: u32) -> u32 {
    (light >> 5) & 0x1f
}

fn with_block_light(light: u32, value: u32, hsv: [u8; 3]) -> u32 {
    (light & SUN_MASK)
        | (value << 5)
        | ((hsv[0] as u32 & 0x3f) << 10)
        | ((hsv[1] as u32 & 0x7) << 16)
}

/// Lumière des blocs : chaque cellule garde l'intensité maximale reçue, avec la teinte de la
/// source correspondante (le jeu mélange les teintes ; l'intensité est identique).
fn block_light(region: &mut Region) {
    let mut sources = Vec::new();
    for c in 0..region.columns.len() {
        let col = &region.columns[c];
        for source in &col.light_sources {
            if let Some(hsv) = region.tables.emission(source.block_id) {
                let x = source.pos[0] as i64 - region.origin.0 as i64 * CHUNK_SIZE as i64;
                let z = source.pos[2] as i64 - region.origin.1 as i64 * CHUNK_SIZE as i64;
                sources.push(((x, source.pos[1] as i64, z), hsv));
            }
        }
    }

    let mut queue: VecDeque<((i64, i64, i64), u32)> = VecDeque::new();
    for (pos, hsv) in sources {
        let Some(cell) = region.locate(pos.0, pos.1, pos.2) else {
            continue;
        };
        let strength = hsv[2] as u32;
        let current = region.light(cell);
        if block_value(current) < strength {
            region.set_light(cell, with_block_light(current, strength, hsv));
        }
        queue.push_back((pos, strength));
        while let Some(((x, y, z), value)) = queue.pop_front() {
            let Some(cell) = region.locate(x, y, z) else {
                continue;
            };
            let next = value as i64 - region.absorption(cell) as i64 - 1;
            if next <= 0 {
                continue;
            }
            for (dx, dy, dz) in NEIGHBOURS {
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                let Some(n) = region.locate(nx, ny, nz) else {
                    continue;
                };
                let current = region.light(n);
                if (block_value(current) as i64) < next {
                    region.set_light(n, with_block_light(current, next as u32, hsv));
                    queue.push_back(((nx, ny, nz), next as u32));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LightSource;

    const STONE: u32 = 1;
    const TORCH: u32 = 2;

    fn tables() -> LightTables {
        LightTables {
            absorption: vec![0, 99, 0],
            emission: vec![[0, 0, 0], [0, 0, 0], [4, 4, 14]],
        }
    }

    fn sun(column: &Column, x: usize, y: usize, z: usize) -> u32 {
        column.light[y / CHUNK_SIZE].get(index3d(x, y % CHUNK_SIZE, z)) & SUN_MASK
    }

    #[test]
    fn sun_reaches_the_surface_and_stops_under_a_roof() {
        let mut column = Column::new(0, 0, 4);
        for x in 0..32 {
            for z in 0..32 {
                column.set_block(x, 10, z, STONE);
            }
        }
        // Toit au-dessus d'une partie du sol.
        for x in 0..10 {
            for z in 0..10 {
                column.set_block(x, 20, z, STONE);
            }
        }
        let mut columns = vec![column];
        light_columns(&mut columns, &tables());
        let c = &columns[0];
        assert_eq!(sun(c, 20, 11, 20), SUN_BRIGHTNESS);
        assert_eq!(sun(c, 20, 100, 20), SUN_BRIGHTNESS);
        assert_eq!(sun(c, 20, 5, 20), 0, "sous la roche");
        // Sous le toit, le soleil arrive de côté en s'affaiblissant.
        let under = sun(c, 2, 11, 2);
        assert!(under > 0 && under < SUN_BRIGHTNESS, "{under}");
    }

    #[test]
    fn sunlight_crosses_column_borders() {
        let mut left = Column::new(0, 0, 4);
        let mut right = Column::new(1, 0, 4);
        for x in 0..32 {
            for z in 0..32 {
                left.set_block(x, 10, z, STONE);
                left.set_block(x, 12, z, STONE);
                right.set_block(x, 10, z, STONE);
            }
        }
        let mut columns = vec![left, right];
        light_columns(&mut columns, &tables());
        // Le bord de la galerie couverte reçoit le soleil de la colonne voisine.
        assert_eq!(sun(&columns[0], 31, 11, 5), SUN_BRIGHTNESS - 1);
    }

    #[test]
    fn torches_light_their_surroundings() {
        let mut column = Column::new(0, 0, 4);
        for x in 0..32 {
            for z in 0..32 {
                column.set_block(x, 30, z, STONE);
            }
        }
        column.set_block(16, 5, 16, TORCH);
        column.light_sources.push(LightSource {
            pos: [16, 5, 16],
            block_id: TORCH,
        });
        let mut columns = vec![column];
        light_columns(&mut columns, &tables());
        let at = |x, y, z| block_value(columns[0].light[0].get(index3d(x, y, z)));
        assert_eq!(at(16, 5, 16), 14);
        assert_eq!(at(17, 5, 16), 13);
        assert_eq!(at(16, 5, 20), 10);
    }
}
