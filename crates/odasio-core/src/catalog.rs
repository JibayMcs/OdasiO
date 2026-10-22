//! Catalogue des codes de blocs Vintage Story existants (`assets/vs_block_codes.txt`).

use std::collections::BTreeSet;
use std::sync::OnceLock;

static CODES_TXT: &str = include_str!("../../../assets/vs_block_codes.txt");
static LIGHT_TSV: &str = include_str!("../../../assets/vs_block_light.tsv");

/// Ensemble trié des codes VS 1.22.7 (`game:rock-granite`...).
pub fn vs_codes() -> &'static BTreeSet<String> {
    static CODES: OnceLock<BTreeSet<String>> = OnceLock::new();
    CODES.get_or_init(|| {
        CODES_TXT
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    })
}

pub fn vs_code_exists(code: &str) -> bool {
    vs_codes().contains(code)
}

/// Lumière émise par un bloc VS (`lightHsv` : teinte, saturation, intensité).
pub fn vs_light(code: &str) -> Option<[u8; 3]> {
    static LIGHTS: OnceLock<std::collections::HashMap<String, [u8; 3]>> = OnceLock::new();
    LIGHTS
        .get_or_init(|| {
            LIGHT_TSV
                .lines()
                .filter(|l| !l.starts_with('#'))
                .filter_map(|l| {
                    let mut cols = l.split('\t');
                    let code = cols.next()?.to_string();
                    let mut hsv = [0u8; 3];
                    for v in &mut hsv {
                        *v = cols.next()?.parse().ok()?;
                    }
                    Some((code, hsv))
                })
                .collect()
        })
        .get(code)
        .copied()
}

/// Codes contenant tous les mots de la requête, pour l'autocomplétion.
pub fn search(query: &str, limit: usize) -> Vec<&'static str> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    vs_codes()
        .iter()
        .filter(|code| words.iter().all(|w| code.contains(w.as_str())))
        .take(limit)
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_core_blocks() {
        assert!(vs_codes().len() > 10_000);
        assert!(vs_code_exists("game:air"));
        assert!(vs_code_exists("game:rock-granite"));
        assert!(!vs_code_exists("game:rock-cheese"));
    }

    #[test]
    fn light_sources_are_known() {
        assert_eq!(vs_light("game:torch-basic-lit-up"), Some([4, 4, 14]));
        assert!(vs_light("game:lava-still-7").is_some());
        assert_eq!(vs_light("game:rock-granite"), None);
    }

    #[test]
    fn search_matches_all_words() {
        let hits = search("rock granite", 50);
        assert!(hits.contains(&"game:rock-granite"));
        assert!(
            hits.iter()
                .all(|c| c.contains("rock") && c.contains("granite"))
        );
    }
}
