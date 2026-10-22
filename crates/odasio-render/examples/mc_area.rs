//! Rend en PNG une zone d'un monde Minecraft avec les textures du client installé.
//! Usage : cargo run --release -p odasio-render --example mc_area -- <monde> <x> <z> <taille> <sortie.png> [rotation] [vs]
//! Avec `vs`, la zone est rendue telle que convertie, avec les textures de Vintage Story.

use odasio_mc::World;
use odasio_render::area::Area;
use odasio_render::iso::render;
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world = World::open(args[0].as_ref())?;
    let (cx, cz, size): (i32, i32, i32) = (args[1].parse()?, args[2].parse()?, args[3].parse()?);
    let rotation: u8 = args.get(5).map_or(Ok(0), |r| r.parse())?;
    let dim = world.overworld().ok_or("pas d'Overworld")?;
    let area = Area::load(dim, cx, cz, size, 24)?.ok_or("zone vide")?;

    let looks: Vec<_> = if args.get(6).is_some_and(|a| a == "vs") {
        let assets = VsAssets::open(&detect_vs_assets().ok_or("Vintage Story introuvable")?)?;
        let mapping = odasio_core::Mapping::builtin();
        area.palette
            .iter()
            .map(|state| {
                let resolved = mapping.resolve(state);
                match &resolved.fluid {
                    Some(fluid) if resolved.block == "game:air" => assets.look(fluid),
                    _ => assets.look(&resolved.block),
                }
            })
            .collect()
    } else {
        let assets = McAssets::open(&detect_client_jar().ok_or("aucun client Minecraft")?)?;
        area.palette
            .iter()
            .map(|state| assets.look(state))
            .collect()
    };
    let missing = looks.iter().skip(1).filter(|l| l.is_none()).count();
    println!("{} états, {missing} sans aspect", area.palette.len() - 1);

    let started = std::time::Instant::now();
    let image = render(&area.scene.rotated(rotation), &looks, 8.0);
    println!(
        "rendu {}x{} en {:.0?}",
        image.width,
        image.height,
        started.elapsed()
    );
    image::save_buffer(
        &args[4],
        &image.rgba,
        image.width as u32,
        image.height as u32,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(())
}
