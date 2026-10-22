use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use odasio_core::mc::World;
use odasio_core::{ConversionPlan, Inventory, Mapping, Quality};
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
