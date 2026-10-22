//! Liste les block entities d'une dimension : nombre par type et textes de panneaux.
//! Usage : cargo run --release -p odasio-mc --example block_entities -- <monde> [panneaux]

use std::collections::BTreeMap;

use odasio_mc::{Chunk, Region, World};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let world = World::open(args.next().ok_or("chemin du monde attendu")?.as_ref())?;
    let show_signs: usize = args.next().map_or(Ok(10), |n| n.parse())?;
    let dim = world.overworld().ok_or("pas d'Overworld")?;

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut signs = Vec::new();
    for file in dim.region_files()? {
        let mut region = Region::open(&file)?;
        let locations: Vec<_> = region.chunks().collect();
        for loc in locations {
            let Some(nbt) = region.read_chunk_nbt(loc)? else {
                continue;
            };
            let Ok(chunk) = Chunk::from_nbt(&nbt) else {
                continue;
            };
            for be in chunk.block_entities {
                *counts.entry(be.id.clone()).or_default() += 1;
                if let Some(sign) = be.sign.filter(|s| !s.joined().is_empty()) {
                    signs.push((be.pos, sign));
                }
            }
        }
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (id, n) in sorted {
        println!("{n:>7}  {id}");
    }
    println!("\n{} panneaux avec du texte", signs.len());
    for (pos, sign) in signs.iter().take(show_signs) {
        println!(
            "{pos:?} [{}] {:?}",
            sign.color.as_deref().unwrap_or("black"),
            sign.joined()
        );
    }
    Ok(())
}
