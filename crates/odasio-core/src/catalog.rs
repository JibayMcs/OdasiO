//! Catalogue des codes de blocs Vintage Story existants (`assets/vs_block_codes.txt`).

use std::collections::BTreeSet;
use std::sync::OnceLock;

static CODES_TXT: &str = include_str!("../../../assets/vs_block_codes.txt");

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
    fn search_matches_all_words() {
        let hits = search("rock granite", 50);
        assert!(hits.contains(&"game:rock-granite"));
        assert!(
            hits.iter()
                .all(|c| c.contains("rock") && c.contains("granite"))
        );
    }
}
