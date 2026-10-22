//! Statuts de génération des chunks d'un monde, et nombre de sections non vides par statut.
//! Usage : cargo run --release -p odasio-mc --example statuses -- <monde>

use std::collections::BTreeMap;

use odasio_mc::{Region, World};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let world = World::open(std::env::args().nth(1).ok_or("monde ?")?.as_ref())?;
    let dim = world.overworld().ok_or("pas d'Overworld")?;
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for file in dim.region_files()? {
        let mut region = Region::open(&file)?;
        let locations: Vec<_> = region.chunks().collect();
        for loc in locations {
            let Some(nbt) = region.read_chunk_nbt(loc)? else {
                continue;
            };
            let value: fastnbt::Value = fastnbt::from_bytes(&nbt)?;
            let status = match &value {
                fastnbt::Value::Compound(c) => match c.get("Status") {
                    Some(fastnbt::Value::String(s)) => s.clone(),
                    _ => "?".into(),
                },
                _ => "?".into(),
            };
            let chunk = odasio_mc::Chunk::from_nbt(&nbt)?;
            let solid = chunk.sections.iter().filter(|s| !s.is_empty()).count();
            let e = stats.entry(status).or_default();
            e.0 += 1;
            e.1 += usize::from(solid > 0);
        }
    }
    for (status, (n, with_blocks)) in stats {
        println!("{status:30} {n:8} chunks, {with_blocks:8} avec des blocs");
    }
    Ok(())
}
