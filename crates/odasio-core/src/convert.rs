//! Conversion d'une dimension Minecraft en monde Vintage Story.
//!
//! Le monde VS de sortie est une copie d'un monde modèle généré par le jeu (table des IDs de
//! blocs, configuration), dont on remplace le terrain par les colonnes converties.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use odasio_mc::{BlockState, Chunk, Region, SignText, World};
use odasio_vs::{
    CHUNK_SIZE, Column, DEFAULT_MAP_SIZE_XZ, EncodedColumn, LightSource, LightTables,
    TemplateOptions, VsDatabase, VsInstall, WorldGenPass, light_columns, template,
};
use rayon::prelude::*;
use thiserror::Error;

use crate::block_entity::{self, BeTemplate};
use crate::catalog;
use crate::mapping::Mapping;
use crate::ocean;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error(transparent)]
    Minecraft(#[from] odasio_mc::McError),
    #[error(transparent)]
    VintageStory(#[from] odasio_vs::VsError),
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
    #[error("dimension introuvable : {0}")]
    MissingDimension(String),
    #[error("la sauvegarde existe déjà : {0}")]
    OutputExists(PathBuf),
    #[error("Vintage Story introuvable : installez le jeu ou indiquez un monde modèle")]
    NoVintageStory,
    #[error("l'éclairage par le jeu n'est possible qu'en monde superflat")]
    GameLightingNeedsSuperflat,
    #[error("hauteur du monde modèle ({template}) différente de celle demandée ({wanted})")]
    HeightMismatch { template: u32, wanted: u32 },
}

pub type Result<T> = std::result::Result<T, ConvertError>;

/// Type du monde Vintage Story produit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldType {
    /// Plaine plate autour de la zone convertie.
    Superflat,
    /// Terrain, climat et végétation de Vintage Story autour de la zone convertie.
    Standard,
    /// Océan à perte de vue autour de la zone convertie (monde standard sans terres).
    Ocean,
}

/// Saison du monde VS (`seasons`), modifiable ensuite en jeu avec `/worldconfig seasons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Cycle,
    Spring,
    Summer,
    Fall,
    Winter,
}

impl Season {
    pub const ALL: [Season; 5] = [
        Season::Cycle,
        Season::Spring,
        Season::Summer,
        Season::Fall,
        Season::Winter,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Season::Cycle => "enabled",
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Fall => "fall",
            Season::Winter => "winter",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Season::Cycle => "Saisons actives",
            Season::Spring => "Toujours le printemps",
            Season::Summer => "Toujours l'été",
            Season::Fall => "Toujours l'automne",
            Season::Winter => "Toujours l'hiver",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    Survival,
    Creative,
}

impl GameMode {
    /// Style de jeu VS correspondant.
    fn play_style(self) -> &'static str {
        match self {
            GameMode::Survival => "surviveandbuild",
            GameMode::Creative => "creativebuilding",
        }
    }
}

/// Niveau de la mer d'un monde VS standard (`TerraGenConfig.seaLevel`).
pub fn vs_sea_level(map_size_y: u32) -> i32 {
    (0.4313725490196078 * map_size_y as f64) as i32
}

/// Niveau de la mer de Minecraft.
pub const MC_SEA_LEVEL: i32 = 63;

/// Hauteur et décalage d'un monde standard : les mers Minecraft et Vintage Story sont au même
/// niveau, et la plus petite hauteur qui contient toute la plage verticale occupée est retenue.
pub fn standard_layout(y_range: Option<(i32, i32)>) -> (u32, i32) {
    let (low, high) = y_range.unwrap_or((-64, 319));
    for height in [320u32, 384, 448, 512] {
        let offset = vs_sea_level(height) - MC_SEA_LEVEL;
        if low + offset >= 0 && high + offset < height as i32 {
            return (height, offset);
        }
    }
    (512, vs_sea_level(512) - MC_SEA_LEVEL)
}

impl WorldType {
    /// Terrain VS autour de la zone : les mers doivent être alignées.
    pub fn aligns_sea(self) -> bool {
        self != WorldType::Superflat
    }

    pub fn code(self) -> &'static str {
        match self {
            WorldType::Superflat => "superflat",
            WorldType::Standard | WorldType::Ocean => "standard",
        }
    }
}

/// Qui calcule l'éclairage des colonnes converties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lighting {
    /// Odasio : colonnes terminées dès l'écriture, quel que soit le type de monde.
    Odasio,
    /// Le jeu, au premier chargement (monde superflat uniquement : en monde standard, la
    /// passe qui éclaire ferait aussi pousser arbres et structures sur le terrain converti).
    Game,
}

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub dimension: String,
    pub output: PathBuf,
    pub overwrite: bool,
    pub world_name: String,
    /// Monde modèle existant ; sinon généré avec le serveur VS installé.
    pub template: Option<PathBuf>,
    /// Hauteur du monde VS, multiple de 32.
    pub map_size_y: u32,
    /// Ajouté au Y Minecraft (64 : le fond du monde 1.18+, Y = -64, devient Y = 0).
    pub y_offset: i32,
    pub world_type: WorldType,
    pub lighting: Lighting,
    pub season: Season,
    pub game_mode: GameMode,
}

impl ConvertOptions {
    pub fn new(output: PathBuf) -> Self {
        Self {
            dimension: "minecraft:overworld".into(),
            output,
            overwrite: false,
            world_name: "Odasio".into(),
            template: None,
            map_size_y: 384,
            y_offset: 64,
            world_type: WorldType::Superflat,
            lighting: Lighting::Odasio,
            season: Season::Cycle,
            game_mode: GameMode::Creative,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Progress {
    GeneratingTemplate,
    Converting { done: usize, total: usize },
    Finishing,
}

#[derive(Debug, Clone, Default)]
pub struct ConvertReport {
    pub regions: usize,
    pub mc_chunks: usize,
    pub skipped_chunks: usize,
    pub columns: usize,
    /// Codes VS absents de la table des IDs du monde (blocs remplacés par de l'air).
    pub missing_codes: BTreeMap<String, u64>,
    /// Blocs coupés car hors de la hauteur du monde VS.
    pub clipped_blocks: u64,
    /// Block entities écrites, par classe VS (coffres, portes, panneaux...).
    pub block_entities: BTreeMap<String, usize>,
    /// Panneaux dont le texte a été repris.
    pub signs: usize,
    /// Colonnes d'océan générées par Odasio (monde océan).
    pub ocean_columns: usize,
    pub spawn: [i32; 3],
    pub elapsed: Duration,
}

const MAP_CENTER: i64 = (DEFAULT_MAP_SIZE_XZ / 2) as i64;

/// Position VS du bloc Minecraft (0, 0). Le spawn Minecraft (ou le centre de la zone
/// convertie) est placé au milieu de la carte VS : c'est là que le serveur précharge les
/// chunks de spawn et que les distances au centre du monde (climat) sont calculées.
/// L'alignement sur 512 blocs fait correspondre une région MC à 16 x 16 colonnes VS.
fn origin(world: &World, region_files: &[PathBuf]) -> [i64; 2] {
    let center = match world.level.spawn {
        Some([x, _, z]) => [x as i64, z as i64],
        None => {
            let coords: Vec<(i32, i32)> = region_files
                .iter()
                .filter_map(|f| Region::coords_from_name(f))
                .collect();
            if coords.is_empty() {
                [0, 0]
            } else {
                let span = |pick: fn(&(i32, i32)) -> i32| {
                    let min = coords.iter().map(pick).min().unwrap() as i64;
                    let max = coords.iter().map(pick).max().unwrap() as i64;
                    (min + max + 1) * 512 / 2
                };
                [span(|c| c.0), span(|c| c.1)]
            }
        }
    };
    center.map(|c| MAP_CENTER - c.div_euclid(512) * 512)
}

/// Bloc et fluide VS d'un état Minecraft, en IDs numériques du monde.
#[derive(Debug, Clone, Copy, Default)]
struct VsIds {
    block: u32,
    fluid: u32,
    block_emits_light: bool,
    fluid_emits_light: bool,
    /// Index dans [`IdResolver::templates`] si le bloc a besoin d'une block entity.
    block_entity: Option<u32>,
}

struct IdResolver<'a> {
    mapping: &'a Mapping,
    ids: &'a HashMap<String, u32>,
    missing: &'a Mutex<BTreeMap<String, u64>>,
    cache: HashMap<BlockState, VsIds>,
    templates: Vec<BeTemplate>,
}

impl IdResolver<'_> {
    fn id(&self, code: &str, weight: u64) -> u32 {
        match self.ids.get(code) {
            Some(&id) => id,
            None => {
                *self
                    .missing
                    .lock()
                    .unwrap()
                    .entry(code.to_string())
                    .or_default() += weight;
                0
            }
        }
    }

    fn resolve(&mut self, state: &BlockState) -> VsIds {
        if let Some(&ids) = self.cache.get(state) {
            return ids;
        }
        let resolved = self.mapping.resolve(state);
        let block = self.id(&resolved.block, 1);
        // Pas de block entity pour un bloc absent du jeu (remplacé par de l'air).
        let block_entity = (block != 0)
            .then(|| block_entity::template(state, &resolved.block))
            .flatten()
            .map(|t| {
                self.templates.push(t);
                (self.templates.len() - 1) as u32
            });
        let ids = VsIds {
            block,
            fluid: resolved.fluid.as_deref().map_or(0, |f| self.id(f, 1)),
            block_emits_light: catalog::vs_light(&resolved.block).is_some(),
            fluid_emits_light: resolved
                .fluid
                .as_deref()
                .is_some_and(|f| catalog::vs_light(f).is_some()),
            block_entity,
        };
        self.cache.insert(state.clone(), ids);
        ids
    }
}

/// Remplissage d'océan des quarts de colonne vides.
#[derive(Debug, Clone, Copy)]
struct OceanFill {
    surface: usize,
    blocks: ocean::OceanBlocks,
}

/// Fluide lumineux posé dans une colonne : x, y, z locaux et ID du fluide.
type FluidLight = (usize, usize, usize, u32);

struct RegionResult {
    columns: Vec<EncodedColumn>,
    mc_chunks: usize,
    skipped: usize,
    clipped: u64,
    block_entities: BTreeMap<String, usize>,
    signs: usize,
    /// Hauteur du sol par colonne VS au centre de la colonne (pour choisir le spawn).
    surface: Vec<((u32, u32), u16)>,
}

fn convert_region(
    path: &Path,
    resolver: &mut IdResolver,
    opts: &ConvertOptions,
    origin: [i64; 2],
    top_rock: u32,
    light: Option<&LightTables>,
    ocean: Option<OceanFill>,
) -> Result<RegionResult> {
    let mut region = Region::open(path)?;
    let height_chunks = (opts.map_size_y as usize) / CHUNK_SIZE;
    let height = opts.map_size_y as i32;
    let mut columns: HashMap<(u32, u32), Column> = HashMap::new();
    // Chunks Minecraft présents dans chaque colonne : un bit par quart (2 x 2 chunks).
    let mut quarters: HashMap<(u32, u32), u8> = HashMap::new();
    // Fluides lumineux (lave) : seule leur surface est déclarée au jeu, la lumière se
    // propage ensuite d'elle-même.
    let mut fluid_lights: HashMap<(u32, u32), Vec<FluidLight>> = HashMap::new();
    let mut result = RegionResult {
        columns: Vec::new(),
        mc_chunks: 0,
        skipped: 0,
        clipped: 0,
        block_entities: BTreeMap::new(),
        signs: 0,
        surface: Vec::new(),
    };

    let locations: Vec<_> = region.chunks().collect();
    for loc in locations {
        let Some(nbt) = region.read_chunk_nbt(loc)? else {
            continue;
        };
        let chunk = match Chunk::from_nbt(&nbt) {
            Ok(chunk) if chunk.complete => chunk,
            Ok(_) => {
                result.skipped += 1;
                continue;
            }
            Err(e) => {
                tracing::warn!("chunk {},{} ignoré : {e}", loc.x, loc.z);
                result.skipped += 1;
                continue;
            }
        };
        result.mc_chunks += 1;

        let vs_x = origin[0] + chunk.x as i64 * 16;
        let vs_z = origin[1] + chunk.z as i64 * 16;
        if vs_x < 0 || vs_z < 0 || vs_x >= 2 * MAP_CENTER || vs_z >= 2 * MAP_CENTER {
            result.skipped += 1;
            continue;
        }
        let key = ((vs_x / 32) as u32, (vs_z / 32) as u32);
        let (ox, oz) = ((vs_x % 32) as usize, (vs_z % 32) as usize);
        *quarters.entry(key).or_default() |= 1 << ((ox / 16) + 2 * (oz / 16));
        let column = columns
            .entry(key)
            .or_insert_with(|| Column::new(key.0, key.1, height_chunks));
        // Texte des panneaux Minecraft, par position absolue.
        let signs: HashMap<[i32; 3], &SignText> = chunk
            .block_entities
            .iter()
            .filter_map(|be| be.sign.as_ref().map(|s| (be.pos, s)))
            .collect();

        for section in &chunk.sections {
            if section.is_empty() {
                continue;
            }
            let ids: Vec<VsIds> = section
                .palette
                .iter()
                .map(|s| resolver.resolve(s))
                .collect();
            let base_y = section.y as i32 * 16 + opts.y_offset;
            for i in 0..4096usize {
                let p = section.indices.as_ref().map_or(0, |idx| idx[i] as usize);
                let VsIds {
                    block,
                    fluid,
                    block_emits_light,
                    fluid_emits_light,
                    block_entity,
                } = ids[p];
                if block == 0 && fluid == 0 {
                    continue;
                }
                let y = base_y + (i >> 8) as i32;
                if !(0..height).contains(&y) {
                    result.clipped += 1;
                    continue;
                }
                let (x, z) = (ox + (i & 15), oz + ((i >> 4) & 15));
                column.set_block(x, y as usize, z, block);
                column.set_fluid(x, y as usize, z, fluid);
                if block_emits_light {
                    column.light_sources.push(LightSource {
                        pos: [
                            key.0 as i32 * 32 + x as i32,
                            y,
                            key.1 as i32 * 32 + z as i32,
                        ],
                        block_id: block,
                    });
                }
                if let Some(index) = block_entity {
                    let template = &resolver.templates[index as usize];
                    let mc_y = section.y as i32 * 16 + (i >> 8) as i32;
                    let mc_pos = [
                        chunk.x * 16 + (i & 15) as i32,
                        mc_y,
                        chunk.z * 16 + ((i >> 4) & 15) as i32,
                    ];
                    let pos = [
                        key.0 as i32 * 32 + x as i32,
                        y,
                        key.1 as i32 * 32 + z as i32,
                    ];
                    let sign = signs.get(&mc_pos).copied();
                    if template.takes_sign_text && sign.is_some_and(|s| !s.joined().is_empty()) {
                        result.signs += 1;
                    }
                    column.block_entities.push(template.instantiate(pos, sign));
                    if template.placed_on_load {
                        column.placed_on_load.push(pos);
                    }
                    *result
                        .block_entities
                        .entry(template.class.clone())
                        .or_default() += 1;
                }
                if fluid_emits_light {
                    fluid_lights
                        .entry(key)
                        .or_default()
                        .push((x, y as usize, z, fluid));
                }
            }
        }
    }

    let mut columns: Vec<Column> = columns.into_values().collect();
    // Les chunks Minecraft absents d'une colonne deviennent de l'océan.
    if let Some(fill) = ocean {
        for column in &mut columns {
            let mask = quarters.get(&(column.cx, column.cz)).copied().unwrap_or(0);
            for quarter in (0..4).filter(|q| mask & (1 << q) == 0) {
                let (x0, z0) = ((quarter % 2) * 16, (quarter / 2) * 16);
                ocean::fill_ocean(
                    column,
                    (x0, x0 + 16),
                    (z0, z0 + 16),
                    fill.surface,
                    fill.blocks,
                );
            }
        }
    }
    for column in &mut columns {
        let key = (column.cx, column.cz);
        for (x, y, z, id) in fluid_lights.remove(&key).unwrap_or_default() {
            let covered = y + 1 < column.height() && column.fluid(x, y + 1, z) == id;
            if !covered {
                column.light_sources.push(LightSource {
                    pos: [
                        key.0 as i32 * 32 + x as i32,
                        y as i32,
                        key.1 as i32 * 32 + z as i32,
                    ],
                    block_id: id,
                });
            }
        }
        let surface = (0..column.height())
            .rev()
            .find(|&y| column.block(16, y, 16) != 0 || column.fluid(16, y, 16) != 0)
            .unwrap_or(0) as u16;
        result.surface.push((key, surface));
    }
    // La région entière est éclairée d'un coup : la lumière passe d'une colonne à l'autre.
    let pass = match light {
        Some(tables) => {
            light_columns(&mut columns, tables);
            WorldGenPass::Done
        }
        None => WorldGenPass::Vegetation,
    };
    for column in &columns {
        result.columns.push(column.encode(pass, top_rock)?);
    }
    Ok(result)
}

/// Réglages du monde (`WorldConfiguration`) transmis au serveur à la création du modèle.
fn world_config(opts: &ConvertOptions) -> BTreeMap<String, String> {
    let mut config = BTreeMap::new();
    if opts.world_type != WorldType::Superflat {
        // Le style créatif impose un climat uniforme ; on garde le climat réaliste.
        config.insert("worldClimate".into(), "realistic".into());
    }
    if opts.world_type == WorldType::Ocean {
        config.insert("landcover".into(), "0".into());
    }
    if opts.season != Season::Cycle {
        config.insert("seasons".into(), opts.season.code().into());
    }
    config
}

fn prepare_output(opts: &ConvertOptions, progress: &(impl Fn(Progress) + Sync)) -> Result<()> {
    if opts.output.exists() {
        if !opts.overwrite {
            return Err(ConvertError::OutputExists(opts.output.clone()));
        }
        std::fs::remove_file(&opts.output)?;
    }
    if let Some(dir) = opts.output.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let template = match &opts.template {
        Some(t) => t.clone(),
        None => {
            progress(Progress::GeneratingTemplate);
            let install = VsInstall::detect().ok_or(ConvertError::NoVintageStory)?;
            let work = std::env::temp_dir().join(format!("odasio-template-{}", std::process::id()));
            let template_opts = TemplateOptions {
                world_name: opts.world_name.clone(),
                map_size_y: opts.map_size_y,
                world_type: opts.world_type.code().into(),
                play_style: opts.game_mode.play_style().into(),
                world_config: world_config(opts),
                ..Default::default()
            };
            let generated =
                template::generate(&install, &work, &template_opts, Duration::from_secs(300))?;
            std::fs::copy(&generated, &opts.output)?;
            let _ = std::fs::remove_dir_all(&work);
            return Ok(());
        }
    };
    std::fs::copy(&template, &opts.output)?;
    Ok(())
}

pub fn convert(
    world: &World,
    mapping: &Mapping,
    opts: &ConvertOptions,
    progress: impl Fn(Progress) + Sync,
) -> Result<ConvertReport> {
    let started = Instant::now();
    let dimension = world
        .dimension(&opts.dimension)
        .ok_or_else(|| ConvertError::MissingDimension(opts.dimension.clone()))?;
    let files = dimension.region_files()?;
    if opts.lighting == Lighting::Game && opts.world_type != WorldType::Superflat {
        return Err(ConvertError::GameLightingNeedsSuperflat);
    }

    prepare_output(opts, &progress)?;
    let mut db = VsDatabase::open(&opts.output)?;
    let template_height = db.save.map_size[1];
    if template_height != opts.map_size_y {
        return Err(ConvertError::HeightMismatch {
            template: template_height,
            wanted: opts.map_size_y,
        });
    }
    let ids = db.save.block_ids()?;
    let origin = origin(world, &files);
    let light_tables = (opts.lighting == Lighting::Odasio).then(|| light_tables(&ids));
    let top_rock = ids
        .get(&format!("game:rock-{}", mapping.settings.rock))
        .copied()
        .unwrap_or(0);
    // Dernière couche d'eau : celle de la mer Minecraft (Y = 62) une fois décalée.
    let ocean_surface =
        (MC_SEA_LEVEL - 1 + opts.y_offset).clamp(1, opts.map_size_y as i32 - 1) as usize;
    // Les quarts de colonne sans chunk Minecraft (bordure de la zone générée) seraient des
    // puits jusqu'au fond du monde : ils deviennent de l'océan, quel que soit le type de monde.
    let ocean_fill = Some(OceanFill {
        surface: ocean_surface,
        blocks: ocean::OceanBlocks::from_ids(&ids, top_rock),
    });
    db.clear_terrain()?;

    let missing = Mutex::new(BTreeMap::new());
    let done = AtomicUsize::new(0);
    let total = files.len();
    progress(Progress::Converting { done: 0, total });

    let mut report = ConvertReport::default();
    let mut surfaces: Vec<((u32, u32), u16)> = Vec::new();
    let (tx, rx) = mpsc::sync_channel::<Result<RegionResult>>(4);
    std::thread::scope(|scope| -> Result<()> {
        scope.spawn(|| {
            files.par_iter().for_each_init(
                || IdResolver {
                    mapping,
                    ids: &ids,
                    missing: &missing,
                    cache: HashMap::new(),
                    templates: Vec::new(),
                },
                |resolver, file| {
                    let _ = tx.send(convert_region(
                        file,
                        resolver,
                        opts,
                        origin,
                        top_rock,
                        light_tables.as_ref(),
                        ocean_fill,
                    ));
                },
            );
            drop(tx);
        });
        for result in rx {
            let result = result?;
            db.write_columns(&result.columns)?;
            report.regions += 1;
            report.mc_chunks += result.mc_chunks;
            report.skipped_chunks += result.skipped;
            report.columns += result.columns.len();
            report.clipped_blocks += result.clipped;
            for (class, n) in result.block_entities {
                *report.block_entities.entry(class).or_default() += n;
            }
            report.signs += result.signs;
            surfaces.extend(result.surface);
            let done = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(Progress::Converting { done, total });
        }
        Ok(())
    })?;

    if opts.world_type == WorldType::Ocean {
        let written: std::collections::HashSet<(u32, u32)> =
            surfaces.iter().map(|(key, _)| *key).collect();
        let center = (MAP_CENTER / CHUNK_SIZE as i64) as u32;
        let holes = ocean::holes(&written, (center, center));
        let columns = ocean::ocean_columns(
            &holes,
            opts.map_size_y as usize / CHUNK_SIZE,
            ocean_surface,
            ocean::OceanBlocks::from_ids(&ids, top_rock),
            light_tables.as_ref(),
            top_rock,
        )?;
        for batch in columns.chunks(1024) {
            db.write_columns(batch)?;
        }
        report.ocean_columns = holes.len();
    }

    progress(Progress::Finishing);
    report.spawn = choose_spawn(world, opts, origin, &surfaces);
    db.save
        .set_default_spawn(report.spawn[0], report.spawn[1], report.spawn[2])?;
    db.save.set_world_name(&opts.world_name)?;
    db.save_gamedata()?;
    db.close()?;

    report.missing_codes = missing.into_inner().unwrap();
    report.elapsed = started.elapsed();
    Ok(report)
}

/// Absorption et émission de lumière des blocs, indexées par ID du monde modèle.
fn light_tables(ids: &HashMap<String, u32>) -> LightTables {
    let size = ids.values().copied().max().unwrap_or(0) as usize + 1;
    let mut tables = LightTables {
        absorption: vec![99; size],
        emission: vec![[0; 3]; size],
    };
    for (code, &id) in ids {
        tables.absorption[id as usize] = catalog::vs_absorption(code);
        if let Some(hsv) = catalog::vs_light(code) {
            tables.emission[id as usize] = hsv;
        }
    }
    tables
}

/// Spawn Minecraft s'il est connu, sinon centre de la zone convertie, posé sur le sol.
fn choose_spawn(
    world: &World,
    opts: &ConvertOptions,
    origin: [i64; 2],
    surfaces: &[((u32, u32), u16)],
) -> [i32; 3] {
    let column_center = |(cx, cz): (u32, u32)| (cx as i64 * 32 + 16, cz as i64 * 32 + 16);
    let target = match world.level.spawn {
        Some([x, _, z]) => (origin[0] + x as i64, origin[1] + z as i64),
        None if !surfaces.is_empty() => {
            let n = surfaces.len() as i64;
            let (sx, sz) = surfaces.iter().fold((0i64, 0i64), |acc, (k, _)| {
                let (x, z) = column_center(*k);
                (acc.0 + x, acc.1 + z)
            });
            (sx / n, sz / n)
        }
        None => (MAP_CENTER, MAP_CENTER),
    };
    let nearest = surfaces.iter().min_by_key(|(k, _)| {
        let (x, z) = column_center(*k);
        (x - target.0).pow(2) + (z - target.1).pow(2)
    });
    match nearest {
        Some((k, surface)) => {
            let (x, z) = column_center(*k);
            [
                x as i32,
                (*surface as i32 + 2).min(opts.map_size_y as i32 - 1),
                z as i32,
            ]
        }
        None => [target.0 as i32, opts.y_offset + 80, target.1 as i32],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_config_follows_the_options() {
        let mut opts = ConvertOptions::new(PathBuf::from("x.vcdbs"));
        assert!(world_config(&opts).is_empty());
        opts.world_type = WorldType::Ocean;
        opts.season = Season::Summer;
        let config = world_config(&opts);
        assert_eq!(config["landcover"], "0");
        assert_eq!(config["seasons"], "summer");
        assert_eq!(config["worldClimate"], "realistic");
    }

    #[test]
    fn sea_levels_match_the_game() {
        assert_eq!(vs_sea_level(256), 110);
        assert_eq!(vs_sea_level(384), 165);
    }

    #[test]
    fn standard_layout_aligns_seas_and_fits_the_world() {
        // riveciel : de -64 à 207.
        let (height, offset) = standard_layout(Some((-64, 207)));
        assert_eq!((height, offset), (320, 75));
        assert_eq!(MC_SEA_LEVEL + offset, vs_sea_level(height));
        // Monde utilisant toute la hauteur 1.18+ : il faut monter plus haut.
        let (height, offset) = standard_layout(Some((-64, 319)));
        assert!(319 + offset < height as i32 && -64 + offset >= 0);
    }
}
