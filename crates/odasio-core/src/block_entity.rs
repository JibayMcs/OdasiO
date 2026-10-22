//! Block entities Vintage Story à créer pour les blocs convertis.
//!
//! Le jeu ne crée jamais une block entity manquante au chargement : un coffre, une porte ou
//! un panneau sans elle reste inerte. Les attributs minimaux par classe et les conventions
//! d'orientation viennent de `docs/vs-block-entities.md`.

use std::f32::consts::{FRAC_PI_2, PI};

use odasio_mc::{BlockState, SignText};
use odasio_vs::{Attr, BlockEntity, Tree};

use crate::catalog;

/// Block entity déduite d'un état de bloc, avant de connaître sa position.
#[derive(Debug, Clone)]
pub struct BeTemplate {
    pub class: String,
    pub block_code: String,
    pub attrs: Tree,
    /// À déclarer dans `NewBlockEntities` : le jeu appelle `OnBlockPlaced` au premier chargement.
    pub placed_on_load: bool,
    /// Reçoit le texte du panneau Minecraft à la même position.
    pub takes_sign_text: bool,
}

impl BeTemplate {
    pub fn instantiate(&self, pos: [i32; 3], sign: Option<&SignText>) -> BlockEntity {
        let mut be = BlockEntity::new(&self.class, pos, &self.block_code);
        be.attrs = self.attrs.clone();
        if self.takes_sign_text
            && let Some(sign) = sign
        {
            be.attrs.set("text", Attr::Str(sign.joined()));
            if let Some(color) = sign.color.as_deref().and_then(dye_argb) {
                be.attrs.set("color", Attr::Int(color));
            }
        }
        be
    }
}

/// Angle VS (radians) d'un objet dont la face avant regarde `facing` :
/// sud 0, est pi/2, nord pi, ouest -pi/2.
fn front_angle(facing: &str) -> Option<f32> {
    Some(match facing {
        "south" => 0.0,
        "east" => FRAC_PI_2,
        "north" => PI,
        "west" => -FRAC_PI_2,
        _ => return None,
    })
}

/// Couleur de teinture Minecraft en ARGB (couleur de texte des panneaux).
fn dye_argb(color: &str) -> Option<i32> {
    let rgb: u32 = match color {
        "white" => 0xFFFFFF,
        "orange" => 0xFF681F,
        "magenta" => 0xFF00FF,
        "light_blue" => 0x9AC0CD,
        "yellow" => 0xFFFF00,
        "lime" => 0xBFFF00,
        "pink" => 0xFF69B4,
        "gray" => 0x808080,
        "light_gray" => 0xD3D3D3,
        "cyan" => 0x00FFFF,
        "purple" => 0xA020F0,
        "blue" => 0x0000FF,
        "brown" => 0x8B4513,
        "green" => 0x00FF00,
        "red" => 0xFF0000,
        // Noir : couleur par défaut du jeu, inutile de l'écrire.
        _ => return None,
    };
    Some((0xFF00_0000 | rgb) as i32)
}

/// Rotation d'un panneau sur pied : MC `rotation` 0..15 (0 = texte vers le sud, sens horaire).
fn ground_sign_angle(rotation: u32) -> f32 {
    let angle = -(rotation as f32) * PI / 8.0;
    // Ramené dans ]-pi, pi], comme les valeurs écrites par le jeu.
    if angle <= -PI {
        angle + 2.0 * PI
    } else {
        angle
    }
}

/// Block entity à créer pour `vs_code`, `None` si le bloc n'en a pas besoin.
pub fn template(state: &BlockState, vs_code: &str) -> Option<BeTemplate> {
    let class = catalog::vs_entity_class(vs_code)?;
    let short = vs_code.strip_prefix("game:").unwrap_or(vs_code);
    let mut attrs = Tree::new();
    let mut placed_on_load = false;
    let mut takes_sign_text = false;
    let facing = state.property("facing");

    match class {
        // Sans block entity, la torche reste allumée pour toujours, comme dans Minecraft ;
        // avec, elle se consumerait dès le premier chargement (rattrapage depuis le jour 0).
        "Torch" | "Transient" => return None,
        "Sign" => {
            takes_sign_text = true;
            if short.starts_with("sign-ground") {
                let rotation = state
                    .property("rotation")
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(0);
                attrs.set("meshAngle", Attr::Float(ground_sign_angle(rotation)));
            }
        }
        "Generic" if short.starts_with("door-") => {
            // Porte fermée du côté où se tenait le joueur qui l'a posée.
            let yaw = match facing {
                Some("west") => FRAC_PI_2,
                Some("south") => PI,
                Some("east") => -FRAC_PI_2,
                _ => 0.0,
            };
            attrs
                .set("rotateYRad", Attr::Float(yaw))
                .set("opened", Attr::Bool(state.property("open") == Some("true")))
                .set(
                    "invertHandles",
                    Attr::Bool(state.property("hinge") == Some("right")),
                );
        }
        "Generic" if short.starts_with("trapdoor-") => {
            let face = match facing {
                Some("east") => 1,
                Some("south") => 2,
                Some("west") => 3,
                _ => 0,
            };
            attrs
                .set("attachedFace", Attr::Int(face))
                .set(
                    "rotDeg",
                    Attr::Int(if state.property("half") == Some("top") {
                        180
                    } else {
                        0
                    }),
                )
                .set("opened", Attr::Bool(state.property("open") == Some("true")));
        }
        "GenericTypedContainer" => {
            // Sans `forBlockCode`, le jeu ne sait pas dimensionner l'inventaire et rejette la BE.
            attrs.set("forBlockCode", Attr::Str(short.to_string()));
            if short.starts_with("chest-") {
                attrs.set("type", Attr::Str("normal-generic".into()));
                if let Some(angle) = facing.and_then(front_angle) {
                    attrs.set("meshAngle", Attr::Float(angle));
                }
            }
        }
        "Lantern" => {
            attrs.set("material", Attr::Str("iron".into()));
        }
        "Farmland" => {
            for key in ["n", "p", "k"] {
                attrs.set(key, Attr::Float(50.0));
            }
            attrs.set("originalFertility", Attr::Int(50));
        }
        "ItemFlow" => {
            attrs.set("lastReceivedFromDir", Attr::Int(-1));
        }
        // Le jeu plante sans cet arbre, même vide.
        "Beehive" => {
            attrs.set("emptyskeps", Attr::Tree(Tree::new()));
        }
        // `OnBlockPlaced` fixe la date de croissance par rapport au calendrier du monde.
        "Sapling" => placed_on_load = true,
        // Un tas de charbon sans inventaire n'a pas de sens ; le bloc reste inerte.
        "CoalPile" => return None,
        _ => {}
    }

    Some(BeTemplate {
        class: class.to_string(),
        block_code: short.to_string(),
        attrs,
        placed_on_load,
        takes_sign_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(text: &str) -> BlockState {
        BlockState::parse(text)
    }

    fn float(t: &BeTemplate, key: &str) -> f32 {
        match t.attrs.get(key) {
            Some(Attr::Float(v)) => *v,
            other => panic!("{key} : {other:?}"),
        }
    }

    #[test]
    fn doors_face_the_placer_and_keep_their_hinge() {
        let t = template(
            &state(
                "minecraft:oak_door[facing=east,half=lower,hinge=right,open=true,powered=false]",
            ),
            "game:door-solid-oak",
        )
        .unwrap();
        assert_eq!(t.class, "Generic");
        assert_eq!(float(&t, "rotateYRad"), -FRAC_PI_2);
        assert_eq!(t.attrs.get("invertHandles"), Some(&Attr::Bool(true)));
        assert_eq!(t.attrs.get("opened"), Some(&Attr::Bool(true)));
    }

    #[test]
    fn chests_know_their_inventory_and_orientation() {
        let t = template(
            &state("minecraft:chest[facing=north,type=single]"),
            "game:chest-north",
        )
        .unwrap();
        assert_eq!(
            t.attrs.get("forBlockCode"),
            Some(&Attr::Str("chest-north".into()))
        );
        assert_eq!(float(&t, "meshAngle"), PI);
    }

    #[test]
    fn ground_sign_angles_stay_in_range() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(close(ground_sign_angle(0), 0.0));
        assert!(close(ground_sign_angle(4), -FRAC_PI_2));
        assert!(close(ground_sign_angle(8), PI));
        assert!(close(ground_sign_angle(12), FRAC_PI_2));
        assert!((0..16).all(|r| ground_sign_angle(r) > -PI && ground_sign_angle(r) <= PI + 1e-5));
    }

    #[test]
    fn signs_receive_text_and_color() {
        let t = template(
            &state("minecraft:oak_wall_sign[facing=north]"),
            "game:sign-wall-south",
        )
        .unwrap();
        let sign = SignText {
            lines: vec!["Taverne".into(), "".into(), "".into(), "".into()],
            color: Some("red".into()),
        };
        let be = t.instantiate([1, 2, 3], Some(&sign));
        assert_eq!(be.attrs.get("text"), Some(&Attr::Str("Taverne".into())));
        assert_eq!(
            be.attrs.get("color"),
            Some(&Attr::Int(0xFFFF0000u32 as i32))
        );
        assert_eq!(be.block_code, "sign-wall-south");
    }

    #[test]
    fn torches_and_plain_blocks_get_no_block_entity() {
        assert!(template(&state("minecraft:torch"), "game:torch-basic-lit-up").is_none());
        assert!(template(&state("minecraft:stone"), "game:rock-granite").is_none());
    }
}
