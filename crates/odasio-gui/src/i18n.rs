//! Traductions de l'interface : français (référence) et anglais.
//!
//! Les textes vivent dans `assets/i18n/{fr,en}.toml`, embarqués à la compilation. Une clé
//! absente de la langue courante retombe sur le français, puis sur la clé elle-même.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use odasio_core::{Category, Quality, Season};

use crate::recent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    fn from_code(code: &str) -> Option<Lang> {
        let code = code.trim().to_ascii_lowercase();
        if code.starts_with("fr") {
            Some(Lang::Fr)
        } else if code.starts_with("en") {
            Some(Lang::En)
        } else {
            None
        }
    }

    fn source(self) -> &'static str {
        match self {
            Lang::Fr => include_str!("../../../assets/i18n/fr.toml"),
            Lang::En => include_str!("../../../assets/i18n/en.toml"),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Fr,
    }
}

fn store(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// Langue au démarrage : choix mémorisé, sinon celle du système (fr* en français, le reste
/// en anglais).
pub fn init() {
    let saved = recent::config_file("language.txt")
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|text| Lang::from_code(&text));
    let lang = saved.unwrap_or_else(|| match sys_locale::get_locale() {
        Some(locale) if locale.to_ascii_lowercase().starts_with("fr") => Lang::Fr,
        _ => Lang::En,
    });
    store(lang);
}

/// Change la langue et mémorise le choix.
pub fn set(lang: Lang) {
    store(lang);
    if let Some(f) = recent::config_file("language.txt") {
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(std::path::Path::new(".")));
        let _ = std::fs::write(f, lang.code());
    }
}

/// Clés à plat (`world.title`) d'un fichier TOML.
fn parse(source: &str) -> HashMap<String, String> {
    fn flatten(prefix: &str, table: &toml::Table, out: &mut HashMap<String, String>) {
        for (key, value) in table {
            let key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            match value {
                toml::Value::Table(t) => flatten(&key, t, out),
                toml::Value::String(s) => {
                    out.insert(key, s.clone());
                }
                other => {
                    out.insert(key, other.to_string());
                }
            }
        }
    }
    let table: toml::Table = source.parse().expect("fichier de traduction valide");
    let mut out = HashMap::new();
    flatten("", &table, &mut out);
    out
}

fn table(lang: Lang) -> &'static HashMap<String, String> {
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    let cell = match lang {
        Lang::Fr => &FR,
        Lang::En => &EN,
    };
    cell.get_or_init(|| parse(lang.source()))
}

/// Texte traduit de `key`.
pub fn tr(key: &'static str) -> &'static str {
    let found = table(current())
        .get(key)
        .or_else(|| table(Lang::Fr).get(key));
    debug_assert!(found.is_some(), "clé de traduction absente : {key}");
    found.map_or(key, String::as_str)
}

/// Texte traduit avec remplacement des `{nom}` par leurs valeurs.
pub fn trf(key: &'static str, args: &[(&str, &dyn Display)]) -> String {
    let mut text = tr(key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}

/// Accord en nombre : `key.one` ou `key.other`, avec `{n}` remplacé par le nombre lisible.
/// En français, 0 et 1 sont au singulier ; en anglais, seul 1 l'est.
pub fn plural(n: u64, key_one: &'static str, key_other: &'static str) -> String {
    let one = match current() {
        Lang::Fr => n <= 1,
        Lang::En => n == 1,
    };
    trf(
        if one { key_one } else { key_other },
        &[("n", &crate::theme::thousands(n))],
    )
}

fn quality_key(q: Quality) -> &'static str {
    match q {
        Quality::Exact => "quality.exact",
        Quality::Equivalent => "quality.equivalent",
        Quality::Lossy => "quality.lossy",
        Quality::Substitute => "quality.substitute",
        Quality::Removed => "quality.removed",
        Quality::Unmapped => "quality.unmapped",
    }
}

fn category_key(c: Category) -> &'static str {
    match c {
        Category::Terrain => "category.terrain",
        Category::Ore => "category.ore",
        Category::Wood => "category.wood",
        Category::Plant => "category.plant",
        Category::Liquid => "category.liquid",
        Category::Building => "category.building",
        Category::Colored => "category.colored",
        Category::Light => "category.light",
        Category::Sign => "category.sign",
        Category::Door => "category.door",
        Category::Container => "category.container",
        Category::Redstone => "category.redstone",
        Category::Decoration => "category.decoration",
        Category::Other => "category.other",
    }
}

fn season_key(s: Season) -> &'static str {
    match s {
        Season::Cycle => "season.cycle",
        Season::Spring => "season.spring",
        Season::Summer => "season.summer",
        Season::Fall => "season.fall",
        Season::Winter => "season.winter",
    }
}

/// Libellé d'une qualité de correspondance (celui d'odasio-core est en français).
pub fn quality(q: Quality) -> &'static str {
    tr(quality_key(q))
}

pub fn category(c: Category) -> &'static str {
    tr(category_key(c))
}

pub fn season(s: Season) -> &'static str {
    tr(season_key(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const CATEGORIES: [Category; 14] = [
        Category::Terrain,
        Category::Ore,
        Category::Wood,
        Category::Plant,
        Category::Liquid,
        Category::Building,
        Category::Colored,
        Category::Light,
        Category::Sign,
        Category::Door,
        Category::Container,
        Category::Redstone,
        Category::Decoration,
        Category::Other,
    ];

    fn placeholders(text: &str) -> BTreeSet<String> {
        text.split('{')
            .skip(1)
            .filter_map(|s| s.split_once('}').map(|(name, _)| name.to_string()))
            .collect()
    }

    #[test]
    fn both_languages_have_the_same_keys() {
        let fr: BTreeSet<_> = table(Lang::Fr).keys().collect();
        let en: BTreeSet<_> = table(Lang::En).keys().collect();
        let only_fr: Vec<_> = fr.difference(&en).collect();
        let only_en: Vec<_> = en.difference(&fr).collect();
        assert!(
            only_fr.is_empty() && only_en.is_empty(),
            "seulement en français : {only_fr:?}, seulement en anglais : {only_en:?}"
        );
    }

    #[test]
    fn placeholders_match_between_languages() {
        for (key, fr) in table(Lang::Fr) {
            let en = &table(Lang::En)[key];
            assert_eq!(placeholders(fr), placeholders(en), "clé {key}");
        }
    }

    #[test]
    fn no_typographic_dashes() {
        for lang in Lang::ALL {
            for (key, text) in table(lang) {
                assert!(
                    !text.contains(['\u{2014}', '\u{2013}']),
                    "tiret long dans {key}"
                );
            }
        }
    }

    #[test]
    fn enum_labels_are_translated() {
        let keys = Quality::ALL
            .into_iter()
            .map(quality_key)
            .chain(Season::ALL.into_iter().map(season_key))
            .chain(CATEGORIES.into_iter().map(category_key));
        for key in keys {
            assert!(table(Lang::Fr).contains_key(key), "clé {key}");
        }
    }

    #[test]
    fn french_labels_match_odasio_core() {
        for q in Quality::ALL {
            assert_eq!(table(Lang::Fr)[quality_key(q)], q.label());
        }
        for s in Season::ALL {
            assert_eq!(table(Lang::Fr)[season_key(s)], s.label());
        }
        for c in CATEGORIES {
            assert_eq!(table(Lang::Fr)[category_key(c)], c.label());
        }
    }
}
