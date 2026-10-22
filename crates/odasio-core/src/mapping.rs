//! Table de correspondance des blocs Minecraft vers Vintage Story (`assets/mapping.toml`).
//!
//! Résolution d'un état de bloc, par ordre de priorité :
//! 1. surcharge utilisateur ([`Policy::overrides`], produite par l'assistant de mapping) ;
//! 2. première règle du fichier dont l'identifiant et le filtre `when` correspondent ;
//! 3. repli `[default]` pour les blocs sans règle (mods, versions futures).
//!
//! L'action de la politique associée à la qualité et à la catégorie s'applique ensuite
//! (conserver, remplacer par de l'air, ou par un bloc choisi).

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use odasio_mc::BlockState;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog;

/// Table livrée avec Odasio.
pub const DEFAULT_MAPPING: &str = include_str!("../../../assets/mapping.toml");

pub const VS_AIR: &str = "game:air";
const VS_WATER: &str = "game:water-still-7";

#[derive(Debug, Error)]
pub enum MappingError {
    #[error("mapping TOML invalide : {0}")]
    Toml(#[from] toml::de::Error),
    #[error("roche inconnue « {0} » (valeurs possibles : {1})")]
    UnknownRock(String, String),
    #[error("code Vintage Story inconnu : {0}")]
    UnknownVsCode(String),
}

/// Fidélité d'une correspondance, du meilleur au pire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Même bloc.
    Exact,
    /// Même rôle, variante VS (essence de bois, couleur, espèce de fleur).
    Equivalent,
    /// Bon bloc, mais une donnée est perdue (texte, contenu, orientation en block entity).
    Lossy,
    /// Un autre bloc tient lieu de remplaçant.
    Substitute,
    /// Aucun équivalent : air par défaut.
    Removed,
    /// Aucune règle (blocs de mods, versions futures).
    Unmapped,
}

impl Quality {
    pub const ALL: [Quality; 6] = [
        Self::Exact,
        Self::Equivalent,
        Self::Lossy,
        Self::Substitute,
        Self::Removed,
        Self::Unmapped,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::Equivalent => "Équivalent",
            Self::Lossy => "Avec perte",
            Self::Substitute => "Remplaçant",
            Self::Removed => "Sans équivalent",
            Self::Unmapped => "Non mappé",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Terrain,
    Ore,
    Wood,
    Plant,
    Liquid,
    Building,
    Colored,
    Light,
    Sign,
    Door,
    Container,
    Redstone,
    Decoration,
    /// Blocs sans règle.
    Other,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Ore => "Minerais",
            Self::Wood => "Bois",
            Self::Plant => "Végétation",
            Self::Liquid => "Liquides",
            Self::Building => "Construction",
            Self::Colored => "Blocs colorés",
            Self::Light => "Éclairage",
            Self::Sign => "Panneaux et bannières",
            Self::Door => "Portes et trappes",
            Self::Container => "Conteneurs",
            Self::Redstone => "Redstone",
            Self::Decoration => "Décoration",
            Self::Other => "Autres",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub rock: String,
    #[serde(default)]
    pub safe_rocks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Defaults {
    pub solid: String,
    pub non_solid: String,
}

/// Valeur d'un filtre : une valeur exacte ou une liste de valeurs acceptées.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Accepted {
    One(String),
    Any(Vec<String>),
}

impl Accepted {
    fn accepts(&self, value: &str) -> bool {
        match self {
            Self::One(v) => v == value,
            Self::Any(vs) => vs.iter().any(|v| v == value),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub mc: String,
    #[serde(default)]
    pub when: BTreeMap<String, Accepted>,
    pub vs: String,
    pub quality: Quality,
    pub category: Category,
    /// Liquide à poser en plus dans la couche fluide (plantes aquatiques).
    pub fluid: Option<String>,
    /// Classe de block entity attendue par le bloc VS.
    pub be: Option<String>,
    pub note: Option<String>,
}

impl Rule {
    /// Une propriété absente de l'état (chunk d'une version qui ne la connaissait pas)
    /// ne bloque pas la règle.
    pub fn matches(&self, state: &BlockState) -> bool {
        self.when
            .iter()
            .all(|(key, accepted)| state.property(key).is_none_or(|v| accepted.accepts(v)))
    }
}

/// Décision appliquée à un groupe de blocs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "code", rename_all = "snake_case")]
pub enum Action {
    /// Garder la correspondance proposée par la table.
    Keep,
    Air,
    /// Remplacer par ce code VS.
    Block(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub quality: Quality,
    /// `None` : toutes les catégories de cette qualité.
    pub category: Option<Category>,
    pub action: Action,
}

/// Choix de l'utilisateur, sérialisable pour être réutilisé d'une conversion à l'autre.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Policy {
    /// Code VS imposé par état exact (`minecraft:oak_log[axis=y]`) ou par identifiant
    /// (`minecraft:oak_log`). L'état exact l'emporte.
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
    /// La règle portant sur (qualité, catégorie) l'emporte sur celle portant sur la qualité seule.
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

impl Policy {
    pub fn action_for(&self, quality: Quality, category: Category) -> &Action {
        self.rules
            .iter()
            .find(|r| r.quality == quality && r.category == Some(category))
            .or_else(|| {
                self.rules
                    .iter()
                    .find(|r| r.quality == quality && r.category.is_none())
            })
            .map_or(&Action::Keep, |r| &r.action)
    }

    pub fn set_action(&mut self, quality: Quality, category: Option<Category>, action: Action) {
        self.rules
            .retain(|r| !(r.quality == quality && r.category == category));
        if action != Action::Keep {
            self.rules.push(PolicyRule {
                quality,
                category,
                action,
            });
        }
    }

    pub fn validate(&self) -> Result<(), MappingError> {
        let codes =
            self.overrides
                .values()
                .chain(self.rules.iter().filter_map(|r| match &r.action {
                    Action::Block(code) => Some(code),
                    _ => None,
                }));
        for code in codes {
            if !catalog::vs_code_exists(code) {
                return Err(MappingError::UnknownVsCode(code.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct MappingFile {
    settings: Settings,
    default: Defaults,
    #[serde(rename = "block", default)]
    blocks: Vec<Rule>,
}

/// Origine d'une résolution, pour les rapports et l'éditeur de mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Index de la règle dans [`Mapping::rules`].
    Rule(usize),
    /// Aucune règle : repli `[default]`.
    Fallback,
    /// Surcharge de l'utilisateur.
    Override,
}

/// Résultat de la conversion d'un état de bloc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// Code VS de la couche solide (`game:air` si le bloc n'est qu'un liquide).
    pub block: String,
    /// Code VS de la couche fluide, le cas échéant.
    pub fluid: Option<String>,
    pub quality: Quality,
    pub category: Category,
    pub block_entity: Option<String>,
    pub source: Source,
    /// Une politique (et non la table) a décidé du bloc final.
    pub policy_applied: bool,
}

impl fmt::Display for Resolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.block)?;
        if let Some(fluid) = &self.fluid {
            write!(f, " + {fluid}")?;
        }
        Ok(())
    }
}

/// Les liquides vivent dans la couche fluide des chunks VS, pas dans la couche solide.
pub fn is_vs_liquid(code: &str) -> bool {
    [
        "game:water-",
        "game:saltwater-",
        "game:boilingwater-",
        "game:lava-",
    ]
    .iter()
    .any(|prefix| code.starts_with(prefix))
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub settings: Settings,
    pub defaults: Defaults,
    pub policy: Policy,
    rules: Vec<Rule>,
    by_name: HashMap<String, Vec<usize>>,
}

impl Mapping {
    pub fn parse(text: &str) -> Result<Self, MappingError> {
        let file: MappingFile = toml::from_str(text)?;
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, rule) in file.blocks.iter().enumerate() {
            by_name.entry(rule.mc.clone()).or_default().push(i);
        }
        Ok(Self {
            settings: file.settings,
            defaults: file.default,
            policy: Policy::default(),
            rules: file.blocks,
            by_name,
        })
    }

    pub fn builtin() -> Self {
        Self::parse(DEFAULT_MAPPING).expect("mapping embarqué valide")
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn has_rules_for(&self, mc_name: &str) -> bool {
        self.by_name.contains_key(mc_name)
    }

    pub fn set_rock(&mut self, rock: &str) -> Result<(), MappingError> {
        if !self.settings.safe_rocks.is_empty()
            && !self.settings.safe_rocks.iter().any(|r| r == rock)
        {
            return Err(MappingError::UnknownRock(
                rock.to_string(),
                self.settings.safe_rocks.join(", "),
            ));
        }
        self.settings.rock = rock.to_string();
        Ok(())
    }

    fn expand(&self, code: &str) -> String {
        code.replace("{rock}", &self.settings.rock)
    }

    fn rule_for(&self, state: &BlockState) -> Option<usize> {
        self.by_name.get(&state.name).and_then(|indices| {
            indices
                .iter()
                .copied()
                .find(|&i| self.rules[i].matches(state))
        })
    }

    /// Correspondance proposée par la table, avant politique et surcharges.
    pub fn resolve_table(&self, state: &BlockState) -> Resolved {
        let (code, fluid, quality, category, block_entity, source) = match self.rule_for(state) {
            Some(i) => {
                let r = &self.rules[i];
                (
                    self.expand(&r.vs),
                    r.fluid.as_deref().map(|f| self.expand(f)),
                    r.quality,
                    r.category,
                    r.be.clone(),
                    Source::Rule(i),
                )
            }
            None => {
                let fallback = if looks_solid(state) {
                    &self.defaults.solid
                } else {
                    &self.defaults.non_solid
                };
                (
                    self.expand(fallback),
                    None,
                    Quality::Unmapped,
                    Category::Other,
                    None,
                    Source::Fallback,
                )
            }
        };
        self.finish(
            state,
            code,
            fluid,
            quality,
            category,
            block_entity,
            source,
            false,
        )
    }

    /// Correspondance finale : surcharges, table, puis politique.
    pub fn resolve(&self, state: &BlockState) -> Resolved {
        let overridden = self
            .policy
            .overrides
            .get(&state.to_string())
            .or_else(|| self.policy.overrides.get(&state.name));
        let table = self.resolve_table(state);
        if let Some(code) = overridden {
            return self.finish(
                state,
                code.clone(),
                None,
                table.quality,
                table.category,
                None,
                Source::Override,
                true,
            );
        }
        match self.policy.action_for(table.quality, table.category) {
            Action::Keep => table,
            Action::Air => self.finish(
                state,
                VS_AIR.to_string(),
                None,
                table.quality,
                table.category,
                None,
                table.source,
                true,
            ),
            Action::Block(code) => self.finish(
                state,
                code.clone(),
                None,
                table.quality,
                table.category,
                None,
                table.source,
                true,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        state: &BlockState,
        code: String,
        mut fluid: Option<String>,
        quality: Quality,
        category: Category,
        block_entity: Option<String>,
        source: Source,
        policy_applied: bool,
    ) -> Resolved {
        let code = adapt_host_rock(code, &self.settings.rock);
        let block = if is_vs_liquid(&code) {
            fluid = Some(code);
            VS_AIR.to_string()
        } else {
            code
        };
        if fluid.is_none() && state.property("waterlogged") == Some("true") {
            fluid = Some(VS_WATER.to_string());
        }
        Resolved {
            block,
            fluid,
            quality,
            category,
            block_entity,
            source,
            policy_applied,
        }
    }
}

/// Les minerais VS existent par roche hôte (`ore-medium-nativecopper-granite`). On les place
/// dans la roche du terrain converti quand cette combinaison existe dans le jeu, pour éviter
/// des îlots d'une roche étrangère autour de chaque minerai.
fn adapt_host_rock(code: String, rock: &str) -> String {
    if !code.starts_with("game:ore-") || code.ends_with(&format!("-{rock}")) {
        return code;
    }
    let Some((base, _)) = code.rsplit_once('-') else {
        return code;
    };
    let candidate = format!("{base}-{rock}");
    if catalog::vs_code_exists(&candidate) {
        candidate
    } else {
        code
    }
}

/// Heuristique pour les blocs sans règle (mods, versions futures) : on ne remplace par de
/// la roche que ce qui ressemble à un bloc plein, le reste disparaît.
fn looks_solid(state: &BlockState) -> bool {
    const NON_SOLID: &[&str] = &[
        "air",
        "grass",
        "fern",
        "flower",
        "sapling",
        "torch",
        "button",
        "pressure_plate",
        "sign",
        "banner",
        "carpet",
        "rail",
        "vine",
        "lever",
        "door",
        "fence",
        "wall",
        "pane",
        "bars",
        "slab",
        "stairs",
        "ladder",
        "chain",
        "lantern",
        "candle",
        "head",
        "skull",
        "plant",
        "crop",
        "bush",
        "mushroom",
        "coral",
        "kelp",
        "seagrass",
        "bed",
        "frame",
        "pot",
        "rod",
        "wire",
        "repeater",
        "comparator",
        "leaves",
        "web",
        "snow",
        "water",
        "lava",
        "fire",
        "portal",
    ];
    let path = state.name.split_once(':').map_or(&*state.name, |(_, p)| p);
    !NON_SOLID.iter().any(|word| path.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(text: &str) -> BlockState {
        BlockState::parse(text)
    }

    #[test]
    fn builtin_mapping_loads() {
        let mapping = Mapping::builtin();
        assert!(mapping.rules().len() > 1000);
        assert!(mapping.has_rules_for("minecraft:stone"));
    }

    #[test]
    fn builtin_mapping_only_uses_existing_codes() {
        let mapping = Mapping::builtin();
        for rock in mapping.settings.safe_rocks.clone() {
            let mut m = mapping.clone();
            m.set_rock(&rock).unwrap();
            for rule in m.rules() {
                let code = m.expand(&rule.vs);
                assert!(catalog::vs_code_exists(&code), "{} -> {code}", rule.mc);
            }
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let mapping = Mapping::builtin();
        let log = mapping.resolve(&state("minecraft:oak_log[axis=x]"));
        assert!(log.block.ends_with("-we"), "{}", log.block);
        let log = mapping.resolve(&state("minecraft:oak_log[axis=y]"));
        assert!(log.block.ends_with("-ud"), "{}", log.block);
    }

    #[test]
    fn missing_properties_do_not_block_rules() {
        let mapping = Mapping::builtin();
        let wood = mapping.resolve(&state("minecraft:oak_wood"));
        assert_ne!(wood.quality, Quality::Unmapped);
        assert!(wood.block.starts_with("game:log-"), "{}", wood.block);
    }

    #[test]
    fn panes_follow_their_connections() {
        // VS : glasspane-ns s'étend d'est en ouest, comme une vitre MC reliée à l'est.
        let mapping = Mapping::builtin();
        let along_x = mapping.resolve(&state(
            "minecraft:glass_pane[east=true,north=false,south=false,waterlogged=false,west=true]",
        ));
        assert!(along_x.block.ends_with("-ns"), "{}", along_x.block);
        let along_z = mapping.resolve(&state(
            "minecraft:glass_pane[east=false,north=true,south=true,waterlogged=false,west=false]",
        ));
        assert!(along_z.block.ends_with("-ew"), "{}", along_z.block);
    }

    #[test]
    fn full_wool_blocks_stay_full_blocks() {
        // `game:wool-*` est un revêtement d'un voxel, pas un bloc plein.
        let mapping = Mapping::builtin();
        for color in ["white", "gray", "light_gray", "brown", "black", "red"] {
            let wool = mapping.resolve(&state(&format!("minecraft:{color}_wool")));
            assert!(
                !wool.block.starts_with("game:wool-"),
                "{color} -> {}",
                wool.block
            );
        }
    }

    #[test]
    fn rock_placeholder_follows_settings() {
        let mut mapping = Mapping::builtin();
        mapping.set_rock("basalt").unwrap();
        let stone = mapping.resolve(&state("minecraft:stone"));
        assert!(stone.block.contains("basalt"), "{}", stone.block);
        assert!(!stone.block.contains('{'));
        assert!(mapping.set_rock("cheese").is_err());
    }

    #[test]
    fn ores_follow_the_terrain_rock_when_possible() {
        let mut mapping = Mapping::builtin();
        mapping.set_rock("andesite").unwrap();
        let copper = mapping.resolve(&state("minecraft:copper_ore"));
        assert!(copper.block.ends_with("-andesite"), "{}", copper.block);
        assert!(catalog::vs_code_exists(&copper.block));
    }

    #[test]
    fn liquids_go_to_the_fluid_layer() {
        let mapping = Mapping::builtin();
        let water = mapping.resolve(&state("minecraft:water[level=0]"));
        assert_eq!(water.block, VS_AIR);
        assert_eq!(water.fluid.as_deref(), Some("game:water-still-7"));
    }

    #[test]
    fn waterlogged_blocks_get_water() {
        let mapping = Mapping::builtin();
        let slab = mapping.resolve(&state("minecraft:oak_slab[type=bottom,waterlogged=true]"));
        assert_ne!(slab.block, VS_AIR);
        assert_eq!(slab.fluid.as_deref(), Some(VS_WATER));
    }

    #[test]
    fn unknown_blocks_fall_back() {
        let mapping = Mapping::builtin();
        let solid = mapping.resolve(&state("monmod:machine_bizarre"));
        assert_eq!(solid.source, Source::Fallback);
        assert_eq!(solid.quality, Quality::Unmapped);
        assert!(solid.block.starts_with("game:rock-"));
        let plant = mapping.resolve(&state("monmod:blue_flower"));
        assert_eq!(plant.block, VS_AIR);
    }

    #[test]
    fn policy_replaces_by_quality_and_category() {
        let mut mapping = Mapping::builtin();
        let rail = state("minecraft:rail[shape=north_south]");
        assert_eq!(mapping.resolve(&rail).quality, Quality::Removed);

        mapping.policy.set_action(
            Quality::Removed,
            None,
            Action::Block("game:rock-granite".into()),
        );
        assert_eq!(mapping.resolve(&rail).block, "game:rock-granite");

        // La règle plus spécifique (qualité + catégorie) l'emporte.
        mapping
            .policy
            .set_action(Quality::Removed, Some(Category::Redstone), Action::Air);
        let resolved = mapping.resolve(&rail);
        assert_eq!(resolved.block, VS_AIR);
        assert!(resolved.policy_applied);
    }

    #[test]
    fn overrides_win_over_everything() {
        let mut mapping = Mapping::builtin();
        mapping
            .policy
            .overrides
            .insert("minecraft:glowstone".into(), "game:rock-basalt".into());
        let glow = mapping.resolve(&state("minecraft:glowstone"));
        assert_eq!(glow.block, "game:rock-basalt");
        assert_eq!(glow.source, Source::Override);
        assert!(mapping.policy.validate().is_ok());

        mapping
            .policy
            .overrides
            .insert("minecraft:dirt".into(), "game:nope".into());
        assert!(mapping.policy.validate().is_err());
    }
}
