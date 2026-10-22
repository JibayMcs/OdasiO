//! Couverture des aspects VS : codes connus du catalogue sans aspect.

use odasio_render::vs_assets::{VsAssets, detect_vs_assets};

fn main() {
    let root = detect_vs_assets().expect("Vintage Story introuvable");
    let start = std::time::Instant::now();
    let assets = VsAssets::open(&root).unwrap();
    println!("{} codes en {:?}", assets.known_codes(), start.elapsed());
    let filter = std::env::args().nth(1);
    if let Some(code) = std::env::args().nth(2) {
        let json = assets.blocktype(&code);
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
        return;
    }
    let codes = include_str!("../../../assets/vs_block_codes.txt");
    let (mut ok, mut missing) = (0, Vec::new());
    for code in codes.lines().filter(|l| !l.is_empty()) {
        match assets.look(code) {
            Some(_) => ok += 1,
            None => missing.push(code),
        }
    }
    println!("{ok} aspects, {} manquants", missing.len());
    if let Some(filter) = &filter {
        for code in missing
            .iter()
            .filter(|c| c.contains(filter.as_str()))
            .take(20)
        {
            println!("  {code}");
        }
    }
    let mut families: std::collections::BTreeMap<&str, usize> = Default::default();
    for code in &missing {
        let family = code.trim_start_matches("game:").split('-').next().unwrap();
        *families.entry(family).or_default() += 1;
    }
    let mut families: Vec<_> = families.into_iter().collect();
    families.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (family, n) in families.iter().take(40) {
        println!("{n:6} {family}");
    }
}
