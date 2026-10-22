//! Rend en PNG une zone d'un monde Minecraft avec les textures du client installé.
//! Usage : cargo run --release -p odasio-render --example mc_area -- <monde> <x> <z> <taille> <sortie.png> [rotation] [vs]
//! Avec `vs`, la zone est rendue telle que convertie, avec les textures de Vintage Story.

use std::collections::HashMap;

use odasio_mc::{BlockState, Chunk, ChunkLocation, Region, World};
use odasio_render::iso::{Scene, render};
use odasio_render::look::BlockLook;
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

type LookFn = Box<dyn Fn(&BlockState) -> Option<BlockLook>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world = World::open(args[0].as_ref())?;
    let (cx, cz, size): (i32, i32, i32) = (args[1].parse()?, args[2].parse()?, args[3].parse()?);
    let rotation: u8 = args.get(5).map_or(Ok(0), |r| r.parse())?;
    let dim = world.overworld().ok_or("pas d'Overworld")?;

    // États de la zone, colonne par colonne.
    let (x0, z0) = (cx - size / 2, cz - size / 2);
    let mut states: HashMap<(i32, i32, i32), BlockState> = HashMap::new();
    let mut regions: HashMap<(i32, i32), Region> = HashMap::new();
    for chunk_x in x0.div_euclid(16)..=(x0 + size - 1).div_euclid(16) {
        for chunk_z in z0.div_euclid(16)..=(z0 + size - 1).div_euclid(16) {
            let key = (chunk_x.div_euclid(32), chunk_z.div_euclid(32));
            let path = dim.region_dir.join(format!("r.{}.{}.mca", key.0, key.1));
            if !path.is_file() {
                continue;
            }
            let region = match regions.entry(key) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => e.insert(Region::open(&path)?),
            };
            let Some(nbt) = region.read_chunk_nbt(ChunkLocation {
                x: chunk_x,
                z: chunk_z,
            })?
            else {
                continue;
            };
            let chunk = Chunk::from_nbt(&nbt)?;
            for section in &chunk.sections {
                for i in 0..4096 {
                    let (lx, ly, lz) = (i & 15, i >> 8, (i >> 4) & 15);
                    let state = section.block(lx, ly, lz);
                    if state.is_air() {
                        continue;
                    }
                    let (x, y, z) = (
                        chunk_x * 16 + lx as i32,
                        section.y as i32 * 16 + ly as i32,
                        chunk_z * 16 + lz as i32,
                    );
                    if (x0..x0 + size).contains(&x) && (z0..z0 + size).contains(&z) {
                        states.insert((x, y, z), state.clone());
                    }
                }
            }
        }
    }
    // Hauteur utile : de 24 blocs sous la surface la plus basse au plus haut bloc.
    let top = states.keys().map(|k| k.1).max().ok_or("zone vide")?;
    let mut surface = HashMap::new();
    for &(x, y, z) in states.keys() {
        let e = surface.entry((x, z)).or_insert(y);
        *e = (*e).max(y);
    }
    let bottom = surface.values().min().copied().unwrap_or(0) - 24;

    let vs = args.get(6).is_some_and(|a| a == "vs");
    let look: LookFn = if vs {
        let assets = VsAssets::open(&detect_vs_assets().ok_or("Vintage Story introuvable")?)?;
        let mapping = odasio_core::Mapping::builtin();
        Box::new(move |state| {
            let resolved = mapping.resolve(state);
            let look = match &resolved.fluid {
                Some(fluid) if resolved.block == "game:air" => assets.look(fluid),
                _ => assets.look(&resolved.block),
            };
            if look.is_none() && resolved.block != "game:air" {
                println!("  sans aspect VS : {}", resolved.block);
            }
            look
        })
    } else {
        let assets = McAssets::open(&detect_client_jar().ok_or("aucun client Minecraft")?)?;
        Box::new(move |state| assets.look(state))
    };
    let mut index: HashMap<BlockState, u32> = HashMap::new();
    let mut looks = vec![None];
    let mut scene = Scene::new(size as usize, (top - bottom + 1) as usize, size as usize);
    for ((x, y, z), state) in &states {
        if *y < bottom {
            continue;
        }
        let id = *index.entry(state.clone()).or_insert_with(|| {
            looks.push(look(state));
            (looks.len() - 1) as u32
        });
        scene.set(
            (x - x0) as usize,
            (y - bottom) as usize,
            (z - z0) as usize,
            id,
        );
    }
    let missing: Vec<_> = index
        .iter()
        .filter(|(_, i)| looks[**i as usize].is_none())
        .map(|(s, _)| s.name.clone())
        .collect();
    println!(
        "{} états, {} sans aspect : {:?}",
        index.len(),
        missing.len(),
        &missing[..missing.len().min(10)]
    );

    let started = std::time::Instant::now();
    let image = render(&scene.rotated(rotation), &looks, 8.0);
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
