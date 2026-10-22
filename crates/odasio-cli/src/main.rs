use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use odasio_core::mc::World;
use odasio_core::{ConversionPlan, ConvertOptions, Inventory, Mapping, Progress, Quality};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "odasio",
    version,
    about = "Convertisseur de mondes Minecraft vers Vintage Story"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args)]
struct WorldArgs {
    /// Dossier du monde (contient level.dat)
    world: PathBuf,
    /// Dimension à analyser
    #[arg(long, default_value = "minecraft:overworld")]
    dimension: String,
    /// Limite le nombre de fichiers région analysés
    #[arg(long)]
    regions: Option<usize>,
}

#[derive(Subcommand)]
enum Command {
    /// Affiche la version et les formats supportés
    Info,
    /// Inventaire des blocs d'un monde Minecraft
    Scan {
        #[command(flatten)]
        world: WorldArgs,
        /// Nombre de blocs affichés
        #[arg(long, default_value_t = 40)]
        top: usize,
        /// Écrit l'inventaire complet (TSV : nombre, état de bloc)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Plan de conversion : ce que deviendra chaque bloc présent, par qualité
    Plan {
        #[command(flatten)]
        world: WorldArgs,
        /// Roche VS utilisée pour la pierre Minecraft
        #[arg(long)]
        rock: Option<String>,
        /// Nombre d'états affichés par groupe
        #[arg(long, default_value_t = 8)]
        top: usize,
    },
    /// Inspecte une colonne d'une sauvegarde Vintage Story
    Inspect {
        /// Fichier .vcdbs
        save: PathBuf,
        /// Position X d'un bloc de la colonne
        x: u32,
        /// Position Z d'un bloc de la colonne
        z: u32,
    },
    /// Diagnostic d'une sauvegarde Vintage Story : passes de génération et éclairage
    Audit {
        /// Fichier .vcdbs
        save: PathBuf,
    },
    /// Convertit une dimension Minecraft en monde Vintage Story
    Convert {
        /// Dossier du monde Minecraft (contient level.dat)
        world: PathBuf,
        /// Nom du monde VS (et du fichier dans le dossier des sauvegardes)
        #[arg(long, default_value = "Odasio")]
        name: String,
        /// Fichier .vcdbs de sortie (par défaut : dossier des sauvegardes de Vintage Story)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Écrase la sauvegarde si elle existe
        #[arg(long)]
        force: bool,
        /// Monde modèle à utiliser au lieu d'en générer un avec le serveur VS
        #[arg(long)]
        template: Option<PathBuf>,
        /// Dimension à convertir
        #[arg(long, default_value = "minecraft:overworld")]
        dimension: String,
        /// Roche VS utilisée pour la pierre Minecraft
        #[arg(long)]
        rock: Option<String>,
        /// Hauteur du monde VS (multiple de 32)
        #[arg(long, default_value_t = 384)]
        height: u32,
        /// Décalage vertical ajouté au Y Minecraft
        #[arg(long, default_value_t = 64)]
        y_offset: i32,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    match Cli::parse().command {
        Command::Info => {
            println!("{} {}", odasio_core::APP_NAME, odasio_core::VERSION);
            println!("Source : Minecraft Java Edition (formats avant 1.13, 1.13 à 1.17, 1.18+)");
            println!(
                "Cible  : Vintage Story {}",
                odasio_core::vs::TARGET_GAME_VERSION
            );
        }
        Command::Scan { world, top, out } => scan(&world, top, out.as_deref())?,
        Command::Plan { world, rock, top } => plan(&world, rock.as_deref(), top)?,
        Command::Inspect { save, x, z } => inspect(&save, x / 32, z / 32)?,
        Command::Audit { save } => audit(&save)?,
        Command::Convert {
            world,
            name,
            out,
            force,
            template,
            dimension,
            rock,
            height,
            y_offset,
        } => {
            let output = match out {
                Some(out) => out,
                None => odasio_core::paths::vs_saves_dir()
                    .context("dossier des sauvegardes VS introuvable, utilisez --out")?
                    .join(format!("{name}.vcdbs")),
            };
            let mut opts = ConvertOptions::new(output);
            opts.world_name = name;
            opts.overwrite = force;
            opts.template = template;
            opts.dimension = dimension;
            opts.map_size_y = height;
            opts.y_offset = y_offset;
            convert(&world, rock.as_deref(), &opts)?;
        }
    }
    Ok(())
}

fn load_inventory(args: &WorldArgs) -> Result<Inventory> {
    let world = World::open(&args.world)?;
    let level = &world.level;
    println!("Monde       : {}", level.name);
    println!(
        "Version     : {} (DataVersion {})",
        level.version_name.as_deref().unwrap_or("?"),
        level.data_version
    );
    if !level.server_brands.is_empty() {
        println!("Serveur     : {}", level.server_brands.join(", "));
    }
    for dim in &world.dimensions {
        println!(
            "Dimension   : {} ({} régions)",
            dim.id,
            dim.region_files().map(|f| f.len()).unwrap_or(0)
        );
    }

    let dim = world
        .dimension(&args.dimension)
        .with_context(|| format!("dimension introuvable : {}", args.dimension))?;
    let mut files = dim.region_files()?;
    if let Some(n) = args.regions {
        files.truncate(n);
    }

    let started = Instant::now();
    let inventory = Inventory::scan(&files, |_| {});
    println!(
        "\n{} régions, {} chunks ({} incomplets ignorés), {} états distincts, {} erreurs, {:.1?}",
        inventory.regions,
        inventory.chunks,
        inventory.incomplete_chunks,
        inventory.counts.len(),
        inventory.errors,
        started.elapsed()
    );
    for (v, n) in &inventory.unsupported {
        println!("  format non supporté : DataVersion {v}, {n} chunks");
    }
    Ok(inventory)
}

fn scan(args: &WorldArgs, top: usize, out: Option<&Path>) -> Result<()> {
    let inventory = load_inventory(args)?;
    let sorted = inventory.sorted();

    let mut namespaces: Vec<(&str, usize, u64)> = Vec::new();
    for (state, n) in &sorted {
        match namespaces.iter_mut().find(|e| e.0 == state.namespace()) {
            Some(e) => {
                e.1 += 1;
                e.2 += n;
            }
            None => namespaces.push((state.namespace(), 1, *n)),
        }
    }
    namespaces.sort_by_key(|n| std::cmp::Reverse(n.2));
    for (ns, states, blocks) in namespaces {
        println!("  {ns:<20} {states:>6} états {blocks:>14} blocs");
    }
    println!();
    for (state, n) in sorted.iter().filter(|(s, _)| !s.is_air()).take(top) {
        println!("{n:>14}  {state}");
    }

    if let Some(out) = out {
        let tsv: String = sorted.iter().map(|(s, n)| format!("{n}\t{s}\n")).collect();
        std::fs::write(out, tsv)?;
        println!("\nInventaire complet écrit dans {}", out.display());
    }
    Ok(())
}

fn plan(args: &WorldArgs, rock: Option<&str>, top: usize) -> Result<()> {
    let mut mapping = Mapping::builtin();
    if let Some(rock) = rock {
        mapping.set_rock(rock)?;
    }
    let inventory = load_inventory(args)?;
    let plan = ConversionPlan::build(&mapping, &inventory);

    // L'air n'a pas d'intérêt dans les pourcentages.
    let air: u64 = inventory
        .counts
        .iter()
        .filter(|(s, _)| s.is_air())
        .map(|(_, n)| n)
        .sum();
    let solid = (plan.total_blocks - air).max(1);
    let states = plan.states_by_quality();
    println!(
        "\nRépartition (hors air, roche : {}) :",
        mapping.settings.rock
    );
    for (quality, blocks) in plan.blocks_by_quality() {
        let blocks = if quality == Quality::Exact {
            blocks - air
        } else {
            blocks
        };
        println!(
            "  {:<16} {:>5} états {:>14} blocs  {:>7.3} %",
            quality.label(),
            states.get(&quality).copied().unwrap_or(0),
            blocks,
            blocks as f64 * 100.0 / solid as f64
        );
    }

    for ((quality, category), group) in &plan.groups {
        if *quality <= Quality::Equivalent {
            continue;
        }
        println!(
            "\n[{} / {}] {} états, {} blocs",
            quality.label(),
            category.label(),
            group.entries.len(),
            group.blocks
        );
        for entry in group.entries.iter().take(top) {
            println!(
                "  {:>12}  {}  ->  {}",
                entry.count, entry.state, entry.resolved
            );
        }
    }
    Ok(())
}

fn convert(path: &Path, rock: Option<&str>, opts: &ConvertOptions) -> Result<()> {
    let world = World::open(path)?;
    let mut mapping = Mapping::builtin();
    if let Some(rock) = rock {
        mapping.set_rock(rock)?;
    }
    println!(
        "Conversion de « {} » vers {}",
        world.level.name,
        opts.output.display()
    );
    let report = odasio_core::convert(&world, &mapping, opts, |p| match p {
        Progress::GeneratingTemplate => {
            println!("Génération du monde modèle par le serveur Vintage Story...")
        }
        Progress::Converting { done, total } => {
            if done == total || done % 10 == 0 {
                println!("  régions : {done}/{total}");
            }
        }
        Progress::Finishing => println!("Finalisation..."),
    })?;

    println!(
        "\nTerminé en {:.1?} : {} régions, {} chunks MC, {} colonnes VS, {} chunks ignorés, {} blocs hors hauteur",
        report.elapsed,
        report.regions,
        report.mc_chunks,
        report.columns,
        report.skipped_chunks,
        report.clipped_blocks
    );
    println!(
        "Block entities : {} (dont {} panneaux avec texte)",
        report.block_entities.values().sum::<usize>(),
        report.signs
    );
    for (class, n) in &report.block_entities {
        println!("  {class:<24} {n}");
    }
    println!(
        "Spawn : {} {} {}",
        report.spawn[0], report.spawn[1], report.spawn[2]
    );
    for (code, n) in &report.missing_codes {
        println!("  code absent du jeu, remplacé par de l'air : {code} ({n} états)");
    }
    Ok(())
}

fn inspect(save: &Path, cx: u32, cz: u32) -> Result<()> {
    use odasio_core::vs::{VsDatabase, layer, proto::ProtoReader};

    let db = VsDatabase::open(save)?;
    let ids = db.save.block_ids()?;
    let names: std::collections::HashMap<u32, &str> =
        ids.iter().map(|(code, id)| (*id, code.as_str())).collect();
    println!(
        "{} : monde « {} » ({}), {} x {} x {}",
        save.display(),
        db.save.world_name,
        db.save.world_type,
        db.save.map_size[0],
        db.save.map_size[1],
        db.save.map_size[2]
    );

    let map = db
        .read_map_chunk(cx, cz)?
        .with_context(|| format!("pas de map chunk en {cx},{cz}"))?;
    let mut pass = None;
    let mut y_max = None;
    let mut light_sources = 0;
    for field in ProtoReader::new(&map) {
        let field = field?;
        match field.number {
            4 => pass = field.value.as_u64(),
            10 => y_max = field.value.as_u64(),
            17 => light_sources += 1,
            _ => {}
        }
    }
    println!(
        "Colonne {cx},{cz} : passe {pass:?} (6 = terminée), YMax {y_max:?}, {light_sources} sources lumineuses en attente"
    );

    for cy in 0..db.save.map_size[1] / 32 {
        let Some(blob) = db.read_chunk(cx, cy, cz)? else {
            println!("  cy {cy:>2} : absent");
            continue;
        };
        let mut blocks = Vec::new();
        let (mut planes, mut palette): (&[u8], &[u8]) = (&[], &[]);
        for field in ProtoReader::new(&blob) {
            let field = field?;
            match field.number {
                1 => blocks = layer::decode(field.value.as_bytes().unwrap_or_default())?,
                2 => planes = field.value.as_bytes().unwrap_or_default(),
                3 => palette = field.value.as_bytes().unwrap_or_default(),
                _ => {}
            }
        }
        // Lumière : bits 0-4 soleil, 5-9 intensité de la lumière de bloc.
        let light = match layer::decode_light(planes, palette)? {
            None => "non calculée".to_string(),
            Some(cells) => {
                let sun = cells.iter().map(|v| v & 31).max().unwrap_or(0);
                let block = cells.iter().map(|v| (v >> 5) & 31).max().unwrap_or(0);
                format!("soleil {sun:>2}, bloc {block:>2}")
            }
        };
        let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for id in blocks.iter().filter(|&&id| id != 0) {
            *counts.entry(*id).or_default() += 1;
        }
        let mut top: Vec<_> = counts.into_iter().collect();
        top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let summary: Vec<String> = top
            .iter()
            .take(4)
            .map(|(id, n)| format!("{} x{n}", names.get(id).unwrap_or(&"?")))
            .collect();
        println!(
            "  cy {cy:>2} : lumière {light:<17} {}",
            if summary.is_empty() {
                "air".to_string()
            } else {
                summary.join(", ")
            }
        );
    }
    Ok(())
}

fn audit(save: &Path) -> Result<()> {
    use odasio_core::vs::{VsDatabase, layer, proto::ProtoReader};
    use std::collections::BTreeMap;

    let db = VsDatabase::open(save)?;
    let mut passes: BTreeMap<u64, usize> = BTreeMap::new();
    // Colonnes déjà éclairées par le jeu dont le chunk de surface n'a pas de lumière.
    let mut dark = Vec::new();
    let columns = db.map_chunks()?;
    for (cx, cz, map) in &columns {
        let mut pass = 0;
        let mut y_max = 0;
        for field in ProtoReader::new(map) {
            let field = field?;
            match field.number {
                4 => pass = field.value.as_u64().unwrap_or(0),
                10 => y_max = field.value.as_u64().unwrap_or(0),
                _ => {}
            }
        }
        *passes.entry(pass).or_default() += 1;
        // À partir de la passe 4, l'éclairage de la passe Végétation doit être enregistré.
        if pass < 4 {
            continue;
        }
        // Chunk juste au-dessus de la surface : il doit recevoir le soleil.
        let cy = ((y_max + 1) / 32) as u32;
        let Some(blob) = db.read_chunk(*cx, cy.min(db.save.map_size[1] / 32 - 1), *cz)? else {
            continue;
        };
        let (mut planes, mut palette): (&[u8], &[u8]) = (&[], &[]);
        for field in ProtoReader::new(&blob) {
            let field = field?;
            match field.number {
                2 => planes = field.value.as_bytes().unwrap_or_default(),
                3 => palette = field.value.as_bytes().unwrap_or_default(),
                _ => {}
            }
        }
        let sun = layer::decode_light(planes, palette)?
            .map(|cells| cells.iter().map(|v| v & 31).max().unwrap_or(0));
        if sun.unwrap_or(0) == 0 {
            dark.push((*cx, *cz, sun.is_some()));
        }
    }
    println!("{} colonnes", columns.len());
    for (pass, n) in &passes {
        let label = match pass {
            3 => "à éclairer par le jeu",
            6 => "terminée",
            _ => "intermédiaire",
        };
        println!("  passe {pass} ({label}) : {n}");
    }
    println!(
        "{} colonnes éclairées par le jeu (passe 4 ou plus) sans soleil en surface",
        dark.len()
    );
    let unlit = dark.iter().filter(|d| !d.2).count();
    println!("  dont {unlit} sans aucune couche de lumière");
    for (cx, cz, has_layer) in dark.iter().take(15) {
        println!(
            "  colonne {cx},{cz} (bloc {} {}) {}",
            cx * 32,
            cz * 32,
            if *has_layer {
                "lumière à 0"
            } else {
                "pas de lumière"
            }
        );
    }
    Ok(())
}
