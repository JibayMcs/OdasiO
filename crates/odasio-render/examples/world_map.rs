//! Carte vue de dessus d'un monde, en PNG, côté Minecraft et côté Vintage Story.
//! Usage : cargo run --release -p odasio-render --example world_map -- <monde> <sortie-sans-extension> [blocs par pixel]

use odasio_mc::World;
use odasio_render::mc_assets::{McAssets, detect_client_jar};
use odasio_render::topmap::TopMap;
use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world = World::open(args[0].as_ref())?;
    let step: i32 = args.get(2).map_or(Ok(4), |s| s.parse())?;
    let dim = world.overworld().ok_or("pas d'Overworld")?;
    let started = std::time::Instant::now();
    let map = TopMap::build(dim, |_, _| {})?;
    println!(
        "{} régions, {} états, bornes {:?}, {:.1?}",
        map.regions.len(),
        map.palette.len(),
        map.bounds,
        started.elapsed()
    );

    let mc = McAssets::open(&detect_client_jar().ok_or("pas de client Minecraft")?)?;
    let vs = VsAssets::open(&detect_vs_assets().ok_or("pas de Vintage Story")?)?;
    let mapping = odasio_core::Mapping::builtin();
    let started = std::time::Instant::now();
    let mc_colors: Vec<Option<[u8; 3]>> = map
        .palette
        .iter()
        .map(|s| mc.look(s).map(|l| l.map_color()))
        .collect();
    let vs_colors: Vec<Option<[u8; 3]>> = map
        .palette
        .iter()
        .map(|s| {
            let r = mapping.resolve(s);
            let code = match &r.fluid {
                Some(f) if r.block == "game:air" => f.clone(),
                _ => r.block.clone(),
            };
            vs.look(&code).map(|l| l.map_color())
        })
        .collect();
    println!("couleurs en {:.1?}", started.elapsed());

    let (x0, z0, x1, z1) = map.bounds;
    let (w, h) = (((x1 - x0) / step) as u32, ((z1 - z0) / step) as u32);
    for (name, colors, water) in [("mc", &mc_colors, false), ("vs", &vs_colors, true)] {
        let mut image = image::RgbImage::new(w, h);
        for py in 0..h {
            for px in 0..w {
                let (x, z) = (x0 + px as i32 * step, z0 + py as i32 * step);
                let color = match map.column(x, z) {
                    Some((id, incomplete)) if !(water && incomplete) => {
                        let region = &map.regions[&(x.div_euclid(512), z.div_euclid(512))];
                        let (lx, lz) = (x.rem_euclid(512) as usize, z.rem_euclid(512) as usize);
                        let shade = [0.82, 1.0, 1.12][region.shade(lx, lz) as usize];
                        colors[id as usize]
                            .unwrap_or([255, 0, 255])
                            .map(|c| (c as f32 * shade).min(255.0) as u8)
                    }
                    _ if water => [44, 72, 128],
                    _ => [20, 20, 20],
                };
                image.put_pixel(px, py, image::Rgb(color));
            }
        }
        image.save(format!("{}_{name}.png", args[1]))?;
    }
    Ok(())
}
