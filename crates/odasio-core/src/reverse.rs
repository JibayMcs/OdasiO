//! Correspondance inverse Vintage Story vers Minecraft (`assets/mapping_vs_to_mc.toml`).
//!
//! Règles évaluées dans l'ordre du fichier, la première qui correspond l'emporte. `vs` est un
//! code exact ou un motif avec `*`.

use std::collections::BTreeMap;

use odasio_mc::BlockState;
use serde::Deserialize;

use crate::catalog;
use crate::mapping::{MappingError, Quality};

/// Table livrée avec OdasiO.
pub const DEFAULT_REVERSE_MAPPING: &str = include_str!("../../../assets/mapping_vs_to_mc.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct ReverseDefaults {
    pub solid: String,
    pub non_solid: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReverseRule {
    pub vs: String,
    pub mc: String,
    #[serde(default)]
    pub props: BTreeMap<String, String>,
    #[serde(default = "exact")]
    pub quality: Quality,
    pub note: Option<String>,
}

fn exact() -> Quality {
    Quality::Exact
}

#[derive(Deserialize)]
struct ReverseFile {
    default: ReverseDefaults,
    #[serde(rename = "block", default)]
    blocks: Vec<ReverseRule>,
}

/// Motif à `*` (zéro ou plusieurs caractères).
fn glob_matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text.ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    true
}

#[derive(Debug, Clone)]
pub struct ReverseResolved {
    pub state: BlockState,
    pub quality: Quality,
}

#[derive(Debug, Clone)]
pub struct ReverseMapping {
    pub defaults: ReverseDefaults,
    exact: std::collections::HashMap<String, usize>,
    /// Motifs indexés par premier segment du code (`rock`, `ore`...), dans l'ordre du fichier.
    globs: std::collections::HashMap<String, Vec<usize>>,
    /// Motifs commençant par `*`, à essayer pour tout code.
    wild_globs: Vec<usize>,
    rules: Vec<ReverseRule>,
}

/// Premier segment d'un code ou d'un motif, après le domaine : `game:rock-granite` -> `rock`.
fn family(code: &str) -> &str {
    let path = code.split_once(':').map_or(code, |(_, p)| p);
    path.split(['-', '*']).next().unwrap_or("")
}

impl ReverseMapping {
    pub fn parse(text: &str) -> Result<Self, MappingError> {
        let file: ReverseFile = toml::from_str(text)?;
        let mut exact = std::collections::HashMap::new();
        let mut globs: std::collections::HashMap<String, Vec<usize>> = Default::default();
        let mut wild_globs = Vec::new();
        for (i, rule) in file.blocks.iter().enumerate() {
            if !rule.vs.contains('*') {
                exact.entry(rule.vs.clone()).or_insert(i);
                continue;
            }
            let path = rule.vs.split_once(':').map_or(rule.vs.as_str(), |(_, p)| p);
            // Un motif dont le premier segment contient `*` peut viser plusieurs familles.
            let first = path.split('-').next().unwrap_or("");
            if first.contains('*') {
                wild_globs.push(i);
            } else {
                globs
                    .entry(family(&rule.vs).to_string())
                    .or_default()
                    .push(i);
            }
        }
        Ok(Self {
            defaults: file.default,
            exact,
            globs,
            wild_globs,
            rules: file.blocks,
        })
    }

    pub fn builtin() -> Self {
        Self::parse(DEFAULT_REVERSE_MAPPING).expect("mapping inverse embarqué valide")
    }

    pub fn rules(&self) -> &[ReverseRule] {
        &self.rules
    }

    /// Bloc Minecraft pour un code VS complet (`game:rock-granite`).
    pub fn resolve(&self, vs_code: &str) -> ReverseResolved {
        let candidates = self
            .globs
            .get(family(vs_code))
            .into_iter()
            .flatten()
            .chain(&self.wild_globs);
        let first_glob = candidates
            .copied()
            .filter(|&i| glob_matches(&self.rules[i].vs, vs_code))
            .min();
        // La première règle du fichier l'emporte, qu'elle soit exacte ou motif.
        let index = match (self.exact.get(vs_code).copied(), first_glob) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        match index {
            Some(i) => {
                let rule = &self.rules[i];
                ReverseResolved {
                    state: BlockState {
                        name: rule.mc.clone(),
                        properties: rule.props.clone(),
                    },
                    quality: rule.quality,
                }
            }
            None => {
                let solid = catalog::vs_absorption(vs_code) >= 99;
                let name = if solid {
                    &self.defaults.solid
                } else {
                    &self.defaults.non_solid
                };
                ReverseResolved {
                    state: BlockState::new(name.clone()),
                    quality: Quality::Unmapped,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_like_the_game() {
        assert!(glob_matches("game:rock-*", "game:rock-granite"));
        assert!(glob_matches(
            "game:ore-*-nativecopper-*",
            "game:ore-poor-nativecopper-granite"
        ));
        assert!(!glob_matches(
            "game:ore-*-nativecopper-*",
            "game:ore-poor-cassiterite-granite"
        ));
        assert!(glob_matches("game:air", "game:air"));
        assert!(!glob_matches("game:rock-*", "game:rockpolished-granite"));
    }

    #[test]
    fn builtin_reverse_mapping_covers_common_blocks() {
        let mapping = ReverseMapping::builtin();
        assert_eq!(mapping.resolve("game:air").state.name, "minecraft:air");
        assert_eq!(
            mapping.resolve("game:mantle").state.name,
            "minecraft:bedrock"
        );
        let log = mapping.resolve("game:log-placed-oak-ud").state;
        assert_eq!(log.name, "minecraft:oak_log");
        assert_eq!(log.property("axis"), Some("y"));
        assert_ne!(
            mapping.resolve("game:rock-granite").quality,
            Quality::Unmapped
        );
    }

    #[test]
    fn every_vs_code_resolves() {
        let mapping = ReverseMapping::builtin();
        let unmapped = catalog::vs_codes()
            .iter()
            .filter(|c| mapping.resolve(c).quality == Quality::Unmapped)
            .count();
        let examples: Vec<_> = catalog::vs_codes()
            .iter()
            .filter(|c| mapping.resolve(c).quality == Quality::Unmapped)
            .take(8)
            .collect();
        // Moins de 3 % des codes VS tombent sur le repli.
        assert!(
            unmapped * 100 < catalog::vs_codes().len() * 3,
            "{unmapped} : {examples:?}"
        );
    }
}
