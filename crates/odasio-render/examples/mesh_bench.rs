//! Mesure : chargement et maillage des chunks autour d'un point (côté Minecraft).
//! Usage : cargo run --release -p odasio-render --example mesh_bench -- <monde> <x> <z> <rayon en chunks>

use std::sync::{Arc, Mutex};

use odasio_mc::World;
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::mesh::{Atlas, mesh_chunk};
use odasio_render::voxel::ChunkSource;
use rayon::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world = World::open(args[0].as_ref())?;
    let (x, z, r): (i32, i32, i32) = (args[1].parse()?, args[2].parse()?, args[3].parse()?);
    let source = ChunkSource::new(
        world
            .overworld()
            .ok_or("pas d'Overworld")?
            .region_dir
            .clone(),
    );
    let assets = McAssets::open(&detect_client_jar().ok_or("pas de client")?)?;
    let atlas = Mutex::new(Atlas::new(2048));

    let started = std::time::Instant::now();
    let coords: Vec<(i32, i32)> = (-r..=r)
        .flat_map(|dz| (-r..=r).map(move |dx| (x.div_euclid(16) + dx, z.div_euclid(16) + dz)))
        .collect();
    let chunks: Vec<_> = coords
        .par_iter()
        .filter_map(|&(cx, cz)| source.load(cx, cz))
        .collect();
    let loaded = started.elapsed();
    let registry = source.registry.read().unwrap();
    let looks: Vec<_> = (0..registry.len() as u16)
        .map(|id| registry.get(id).and_then(|s| assets.look(s)).map(Arc::new))
        .collect();
    drop(registry);
    let started = std::time::Instant::now();
    let by_pos: std::collections::HashMap<_, _> =
        chunks.iter().map(|c| ((c.cx, c.cz), c)).collect();
    let meshes: Vec<_> = chunks
        .par_iter()
        .map(|c| {
            let n = |dx: i32, dz: i32| by_pos.get(&(c.cx + dx, c.cz + dz)).copied();
            mesh_chunk(c, [n(0, 1), n(0, -1), n(1, 0), n(-1, 0)], &looks, &atlas)
        })
        .collect();
    let quads: usize = meshes
        .iter()
        .map(|m| m.opaque.indices.len() / 6 + m.translucent.indices.len() / 6)
        .sum();
    println!(
        "{} chunks lus en {loaded:.0?}, maillés en {:.0?} : {quads} quads, atlas {} versions",
        chunks.len(),
        started.elapsed(),
        atlas.lock().unwrap().version
    );
    Ok(())
}
