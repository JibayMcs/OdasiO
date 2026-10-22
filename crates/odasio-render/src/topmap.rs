//! Carte vue de dessus d'un monde Minecraft : pour chaque colonne, le bloc visible du ciel
//! et un ombrage de relief (comme les cartes du jeu). Sert à la vue d'ensemble de l'aperçu.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use odasio_mc::{BlockState, Chunk, Dimension, Region};
use rayon::prelude::*;

/// Côté d'une région, en blocs.
pub const REGION: usize = 512;
/// Bit de `RegionMap::top` marquant un chunk incomplet (ignoré par la conversion).
pub const INCOMPLETE: u16 = 0x8000;

/// Une région : index de palette du bloc du dessus (0 = rien) et altitude de ce bloc
/// (`i16::MIN` = rien), par colonne, ordre `z * 512 + x`.
pub struct RegionMap {
    pub top: Vec<u16>,
    pub height: Vec<i16>,
}

impl RegionMap {
    /// Relief d'une colonne par rapport au voisin nord (comme les cartes de Minecraft) :
    /// 0 plus bas, 1 à niveau, 2 plus haut.
    pub fn shade(&self, x: usize, z: usize) -> u8 {
        if z == 0 {
            return 1;
        }
        let (h, north) = (
            self.height[z * REGION + x],
            self.height[(z - 1) * REGION + x],
        );
        if h == i16::MIN || north == i16::MIN {
            return 1;
        }
        match h.cmp(&north) {
            std::cmp::Ordering::Less => 0,
            std::cmp::Ordering::Equal => 1,
            std::cmp::Ordering::Greater => 2,
        }
    }
}

pub struct TopMap {
    pub regions: HashMap<(i32, i32), RegionMap>,
    pub palette: Vec<BlockState>,
    /// Bornes des régions présentes, en blocs : (x min, z min, x max, z max) exclusifs.
    pub bounds: (i32, i32, i32, i32),
}

/// Petits blocs que la carte traverse pour montrer le sol (herbes, fleurs, torches...).
fn is_thin(name: &str) -> bool {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    const EXACT: [&str; 30] = [
        "grass",
        "short_grass",
        "tall_grass",
        "fern",
        "large_fern",
        "dead_bush",
        "dandelion",
        "poppy",
        "blue_orchid",
        "allium",
        "azure_bluet",
        "oxeye_daisy",
        "cornflower",
        "lily_of_the_valley",
        "sunflower",
        "lilac",
        "rose_bush",
        "peony",
        "torchflower",
        "pink_petals",
        "brown_mushroom",
        "red_mushroom",
        "torch",
        "wall_torch",
        "soul_torch",
        "lever",
        "tripwire",
        "string",
        "seagrass",
        "tall_seagrass",
    ];
    EXACT.contains(&name)
        || name.ends_with("_tulip")
        || name.ends_with("_sapling")
        || name.ends_with("_button")
        || name.ends_with("_pressure_plate")
        || name.ends_with("rail")
}

impl TopMap {
    /// Lit toutes les régions de la dimension, en parallèle. `progress(fait, total)`.
    pub fn build(
        dim: &Dimension,
        progress: impl Fn(usize, usize) + Sync,
    ) -> odasio_mc::Result<Self> {
        let files: Vec<PathBuf> = dim.region_files()?;
        let total = files.len();
        let done = AtomicUsize::new(0);
        let palette: Mutex<(Vec<BlockState>, HashMap<BlockState, u16>)> =
            Mutex::new((vec![BlockState::new("minecraft:air")], HashMap::new()));

        let regions: Vec<((i32, i32), RegionMap)> = files
            .par_iter()
            .filter_map(|file| {
                let coords = Region::coords_from_name(file)?;
                let map = read_region(file, &palette).ok().flatten();
                progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
                Some((coords, map?))
            })
            .collect();

        let bounds = regions.iter().fold(
            (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
            |b, ((rx, rz), _)| {
                let (x, z) = (rx * REGION as i32, rz * REGION as i32);
                (b.0.min(x), b.1.min(z), b.2.max(x + 512), b.3.max(z + 512))
            },
        );
        Ok(Self {
            regions: regions.into_iter().collect(),
            palette: palette.into_inner().unwrap().0,
            bounds: if bounds.0 > bounds.2 {
                (0, 0, 0, 0)
            } else {
                bounds
            },
        })
    }

    /// Altitude du bloc du dessus de la colonne (`x`, `z`).
    pub fn height(&self, x: i32, z: i32) -> Option<i32> {
        let region = self
            .regions
            .get(&(x.div_euclid(REGION as i32), z.div_euclid(REGION as i32)))?;
        let i =
            z.rem_euclid(REGION as i32) as usize * REGION + x.rem_euclid(REGION as i32) as usize;
        let h = region.height[i];
        (h != i16::MIN).then_some(h as i32)
    }

    /// Index de palette (et chunk incomplet) de la colonne (`x`, `z`).
    pub fn column(&self, x: i32, z: i32) -> Option<(u16, bool)> {
        let region = self
            .regions
            .get(&(x.div_euclid(REGION as i32), z.div_euclid(REGION as i32)))?;
        let i =
            z.rem_euclid(REGION as i32) as usize * REGION + x.rem_euclid(REGION as i32) as usize;
        let top = region.top[i];
        (top & !INCOMPLETE != 0).then_some((top & !INCOMPLETE, top & INCOMPLETE != 0))
    }
}

fn read_region(
    file: &std::path::Path,
    palette: &Mutex<(Vec<BlockState>, HashMap<BlockState, u16>)>,
) -> odasio_mc::Result<Option<RegionMap>> {
    let mut region = Region::open(file)?;
    let mut top = vec![0u16; REGION * REGION];
    let mut height = vec![i16::MIN; REGION * REGION];
    let mut any = false;
    // Palette locale à la région, fusionnée dans la palette commune à la fin d'un chunk.
    let mut local: HashMap<BlockState, u16> = HashMap::new();
    let locations: Vec<_> = region.chunks().collect();
    for location in locations {
        let Some(nbt) = region.read_chunk_nbt(location)? else {
            continue;
        };
        let Ok(chunk) = Chunk::from_nbt(&nbt) else {
            continue;
        };
        let flag = if chunk.complete { 0 } else { INCOMPLETE };
        let (cx, cz) = (
            chunk.x.rem_euclid(32) as usize * 16,
            chunk.z.rem_euclid(32) as usize * 16,
        );
        for lz in 0..16 {
            for lx in 0..16 {
                'column: for section in chunk.sections.iter().rev() {
                    if section.is_empty() {
                        continue;
                    }
                    for ly in (0..16).rev() {
                        let state = section.block(lx, ly, lz);
                        if state.is_air() || is_thin(&state.name) {
                            continue;
                        }
                        let id = match local.get(state) {
                            Some(&id) => id,
                            None => {
                                let mut shared = palette.lock().unwrap();
                                let (list, index) = &mut *shared;
                                let id = *index.entry(state.clone()).or_insert_with(|| {
                                    list.push(state.clone());
                                    (list.len() - 1) as u16
                                });
                                local.insert(state.clone(), id);
                                id
                            }
                        };
                        let i = (cz + lz) * REGION + cx + lx;
                        top[i] = id | flag;
                        height[i] = section.y as i16 * 16 + ly as i16;
                        any = true;
                        break 'column;
                    }
                }
            }
        }
    }
    if !any {
        return Ok(None);
    }
    Ok(Some(RegionMap { top, height }))
}
