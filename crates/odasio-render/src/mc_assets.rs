//! Aspects des blocs Minecraft, lus dans le `.jar` du client installé (rien n'est téléchargé
//! ni redistribué) : blockstates, modèles avec héritage, textures.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use odasio_mc::BlockState;
use serde_json::Value;
use zip::ZipArchive;

use crate::look::{BlockLook, Face, Shape, Texture};

const GRASS: [u8; 3] = [0x91, 0xbd, 0x59];
const FOLIAGE: [u8; 3] = [0x48, 0xb5, 0x18];
const WATER: [u8; 3] = [0x3f, 0x76, 0xe4];

/// Client Minecraft le plus récent installé (version stable, hors snapshots).
pub fn detect_client_jar() -> Option<PathBuf> {
    let roots = [
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(".minecraft")),
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".minecraft")),
    ];
    let versions = roots
        .into_iter()
        .flatten()
        .map(|r| r.join("versions"))
        .find(|p| p.is_dir())?;
    let mut releases: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(versions)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // Versions stables uniquement : chiffres et points (1.21.1, 26.2).
            let numbers: Option<Vec<u32>> = name.split('.').map(|p| p.parse().ok()).collect();
            let jar = e.path().join(format!("{name}.jar"));
            Some((numbers?, jar)).filter(|(_, jar)| jar.is_file())
        })
        .collect();
    releases.sort();
    releases.pop().map(|(_, jar)| jar)
}

pub struct McAssets {
    jar: Mutex<ZipArchive<File>>,
    textures: Mutex<HashMap<String, Arc<Texture>>>,
}

/// Faces d'un cube, dans l'ordre des normales : x+, x-, y+, y-, z+, z-.
pub(crate) const FACES: [&str; 6] = ["east", "west", "up", "down", "south", "north"];
pub(crate) const NORMALS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// Rotation d'un vecteur par les rotations de blockstate (x puis y, par pas de 90 degrés).
pub(crate) fn rotate(v: [i32; 3], rx: i32, ry: i32) -> [i32; 3] {
    let mut v = v;
    for _ in 0..(rx / 90).rem_euclid(4) {
        // Rotation autour de x : y -> -z (le haut bascule vers le nord).
        v = [v[0], v[2], -v[1]];
    }
    for _ in 0..(ry / 90).rem_euclid(4) {
        // Rotation autour de y, sens horaire vu de dessus : nord -> est.
        v = [-v[2], v[1], v[0]];
    }
    v
}

/// Bornes d'un modèle (en seizièmes) tournées comme lui, ramenées dans 0..1.
pub(crate) fn rotate_bounds(
    min: [f32; 3],
    max: [f32; 3],
    rx: i32,
    ry: i32,
) -> ([f32; 3], [f32; 3]) {
    let corners = [min, max].map(|c| {
        let mut v = [c[0] - 8.0, c[1] - 8.0, c[2] - 8.0];
        for _ in 0..(rx / 90).rem_euclid(4) {
            v = [v[0], v[2], -v[1]];
        }
        for _ in 0..(ry / 90).rem_euclid(4) {
            v = [-v[2], v[1], v[0]];
        }
        v.map(|x| (x + 8.0) / 16.0)
    });
    let lo = [0, 1, 2].map(|i| corners[0][i].min(corners[1][i]).clamp(0.0, 1.0));
    let hi = [0, 1, 2].map(|i| corners[0][i].max(corners[1][i]).clamp(0.0, 1.0));
    (lo, hi)
}

pub(crate) fn face_index(normal: [i32; 3]) -> usize {
    NORMALS.iter().position(|n| *n == normal).unwrap_or(2)
}

struct Model {
    textures: HashMap<String, String>,
    elements: Option<Vec<Value>>,
    cross: bool,
}

impl McAssets {
    pub fn open(jar: &Path) -> std::io::Result<Self> {
        let archive = ZipArchive::new(File::open(jar)?).map_err(std::io::Error::other)?;
        Ok(Self {
            jar: Mutex::new(archive),
            textures: Mutex::new(HashMap::new()),
        })
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut jar = self.jar.lock().unwrap();
        let mut entry = jar.by_name(path).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }

    fn json(&self, path: &str) -> Option<Value> {
        serde_json::from_slice(&self.read(path)?).ok()
    }

    fn texture(&self, id: &str) -> Option<Arc<Texture>> {
        let path = id.strip_prefix("minecraft:").unwrap_or(id);
        if let Some(t) = self.textures.lock().unwrap().get(path) {
            return Some(t.clone());
        }
        let texture = Arc::new(Texture::from_png(
            &self.read(&format!("assets/minecraft/textures/{path}.png"))?,
        )?);
        self.textures
            .lock()
            .unwrap()
            .insert(path.to_string(), texture.clone());
        Some(texture)
    }

    /// Modèle avec ses parents : textures fusionnées, premiers éléments trouvés.
    fn model(&self, id: &str) -> Option<Model> {
        let mut model = Model {
            textures: HashMap::new(),
            elements: None,
            cross: false,
        };
        let mut current = Some(id.to_string());
        let mut depth = 0;
        while let Some(name) = current.take() {
            depth += 1;
            if depth > 16 {
                break;
            }
            let path = name.strip_prefix("minecraft:").unwrap_or(&name);
            if path.contains("cross") {
                model.cross = true;
            }
            let Some(json) = self.json(&format!("assets/minecraft/models/{path}.json")) else {
                break;
            };
            if let Some(Value::Object(textures)) = json.get("textures") {
                for (k, v) in textures {
                    // Chaîne, ou objet `{ "sprite": ... }` depuis Minecraft 26.
                    let sprite = match v {
                        Value::String(v) => Some(v.clone()),
                        Value::Object(o) => {
                            o.get("sprite").and_then(Value::as_str).map(str::to_string)
                        }
                        _ => None,
                    };
                    if let Some(sprite) = sprite {
                        model.textures.entry(k.clone()).or_insert(sprite);
                    }
                }
            }
            if model.elements.is_none()
                && let Some(Value::Array(elements)) = json.get("elements")
            {
                model.elements = Some(elements.clone());
            }
            current = json
                .get("parent")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        Some(model)
    }

    /// Résout `#cle` à travers la table des textures du modèle.
    fn resolve(&self, model: &Model, reference: &str) -> Option<Arc<Texture>> {
        let mut name = reference.to_string();
        for _ in 0..8 {
            match name.strip_prefix('#') {
                Some(key) => name = model.textures.get(key)?.clone(),
                None => return self.texture(&name),
            }
        }
        None
    }

    /// Modèle et rotations de la variante correspondant à l'état.
    fn variant(&self, state: &BlockState) -> Option<(String, i32, i32)> {
        let path = state.name.strip_prefix("minecraft:").unwrap_or(&state.name);
        let json = self.json(&format!("assets/minecraft/blockstates/{path}.json"))?;
        let pick = |v: &Value| -> Option<(String, i32, i32)> {
            let v = if let Value::Array(list) = v {
                list.first()?
            } else {
                v
            };
            Some((
                v.get("model")?.as_str()?.to_string(),
                v.get("x").and_then(Value::as_i64).unwrap_or(0) as i32,
                v.get("y").and_then(Value::as_i64).unwrap_or(0) as i32,
            ))
        };
        if let Some(Value::Object(variants)) = json.get("variants") {
            let matches = |key: &str| {
                key.is_empty()
                    || key.split(',').all(|kv| {
                        kv.split_once('=')
                            .is_some_and(|(k, v)| state.property(k).is_none_or(|p| p == v))
                    })
            };
            return variants
                .iter()
                .find(|(k, _)| matches(k))
                .or_else(|| variants.iter().next())
                .and_then(|(_, v)| pick(v));
        }
        // Multipart (barrières, murs...) : la partie sans condition sert de base.
        let parts = json.get("multipart")?.as_array()?;
        parts
            .iter()
            .find(|p| p.get("when").is_none())
            .or_else(|| parts.first())
            .and_then(|p| pick(p.get("apply")?))
    }

    fn tint_for(name: &str, texture_id: &str) -> [u8; 3] {
        if name.contains("water") || texture_id.contains("water") {
            WATER
        } else if name.contains("leaves") || name.contains("vine") {
            match name {
                "minecraft:birch_leaves" => [0x80, 0xa7, 0x55],
                "minecraft:spruce_leaves" => [0x61, 0x99, 0x61],
                _ => FOLIAGE,
            }
        } else {
            GRASS
        }
    }

    pub fn look(&self, state: &BlockState) -> Option<BlockLook> {
        match state.name.as_str() {
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" => return None,
            "minecraft:water" | "minecraft:bubble_column" => {
                let texture = self.texture("block/water_still")?;
                return Some(BlockLook::single(
                    Shape::Liquid,
                    Face {
                        texture,
                        tint: Some(WATER),
                    },
                ));
            }
            "minecraft:lava" => {
                let texture = self.texture("block/lava_still")?;
                return Some(BlockLook::single(
                    Shape::Liquid,
                    Face {
                        texture,
                        tint: None,
                    },
                ));
            }
            _ => {}
        }
        let (model_id, rx, ry) = self.variant(state)?;
        let model = self.model(&model_id)?;

        if model.cross || model.elements.is_none() {
            let key = ["#cross", "#plant", "#texture", "#particle", "#all"]
                .into_iter()
                .find(|k| model.textures.contains_key(&k[1..]))?;
            let tinted = model_id.contains("tinted")
                || state.name.contains("grass")
                || state.name.contains("fern");
            let texture = self.resolve(&model, key)?;
            return Some(BlockLook::single(
                Shape::Cross,
                Face {
                    texture,
                    tint: tinted.then(|| Self::tint_for(&state.name, key)),
                },
            ));
        }

        // Boîte englobante des éléments, et pour chaque face visible la texture de l'élément
        // le plus avancé dans cette direction.
        let elements = model.elements.as_ref()?;
        let mut min = [16.0f32; 3];
        let mut max = [0.0f32; 3];
        let mut faces: [Option<(f32, String, bool)>; 6] = Default::default();
        for element in elements {
            let read = |key: &str| -> Option<[f32; 3]> {
                let a = element.get(key)?.as_array()?;
                Some([
                    a[0].as_f64()? as f32,
                    a[1].as_f64()? as f32,
                    a[2].as_f64()? as f32,
                ])
            };
            let (Some(from), Some(to)) = (read("from"), read("to")) else {
                continue;
            };
            for i in 0..3 {
                min[i] = min[i].min(from[i]);
                max[i] = max[i].max(to[i]);
            }
            if let Some(Value::Object(element_faces)) = element.get("faces") {
                for (f, name) in FACES.iter().enumerate() {
                    let Some(face) = element_faces.get(*name) else {
                        continue;
                    };
                    let reach = match f {
                        0 => to[0],
                        1 => -from[0],
                        2 => to[1],
                        3 => -from[1],
                        4 => to[2],
                        _ => -from[2],
                    };
                    let Some(texture) = face.get("texture").and_then(Value::as_str) else {
                        continue;
                    };
                    let tinted = face.get("tintindex").is_some();
                    if faces[f].as_ref().is_none_or(|(r, _, _)| reach > *r) {
                        faces[f] = Some((reach, texture.to_string(), tinted));
                    }
                }
            }
        }
        if min[0] > max[0] {
            return None;
        }

        // Rotation de blockstate : quelle face du modèle se retrouve en haut, au sud, à l'est.
        let source = |target: [i32; 3]| -> usize {
            (0..6)
                .find(|&f| rotate(NORMALS[f], rx, ry) == target)
                .unwrap_or_else(|| face_index(target))
        };
        let face = |target: [i32; 3]| -> Option<Face> {
            let f = source(target);
            let (_, texture_ref, tinted) = faces[f]
                .clone()
                .or_else(|| faces.iter().flatten().next().cloned())?;
            let texture = self.resolve(&model, &texture_ref)?;
            Some(Face {
                texture,
                tint: tinted.then(|| Self::tint_for(&state.name, &texture_ref)),
            })
        };
        let top = face([0, 1, 0])?;
        let south = face([0, 0, 1])?;
        let east = face([1, 0, 0])?;

        let (lo, hi) = rotate_bounds(min, max, rx, ry);
        Some(BlockLook::cuboid(lo, hi, top, south, east))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_move_faces_like_minecraft() {
        // y = 90 : la face nord du modèle regarde l'est.
        assert_eq!(rotate([0, 0, -1], 0, 90), [1, 0, 0]);
        // x = 90 : le haut du modèle regarde le nord ; puis y = 90 l'envoie à l'est.
        assert_eq!(rotate([0, 1, 0], 90, 0), [0, 0, -1]);
        assert_eq!(rotate([0, 1, 0], 90, 90), [1, 0, 0]);
    }

    #[test]
    fn reads_looks_from_the_installed_client() {
        let Some(jar) = detect_client_jar() else {
            eprintln!("aucun client Minecraft installé, test ignoré");
            return;
        };
        let assets = McAssets::open(&jar).unwrap();
        let stone = assets.look(&BlockState::new("minecraft:stone")).unwrap();
        assert!(stone.opaque);
        let grass = assets
            .look(&BlockState::parse("minecraft:grass_block[snowy=false]"))
            .unwrap();
        assert!(grass.top.tint.is_some(), "le dessus de l'herbe est teinté");
        let slab = assets
            .look(&BlockState::parse("minecraft:oak_slab[type=bottom]"))
            .unwrap();
        assert!(matches!(slab.shape, Shape::Cuboid { max, .. } if max[1] == 0.5));
        let poppy = assets.look(&BlockState::new("minecraft:poppy")).unwrap();
        assert_eq!(poppy.shape, Shape::Cross);
        let log_x = assets
            .look(&BlockState::parse("minecraft:oak_log[axis=x]"))
            .unwrap();
        let log_y = assets
            .look(&BlockState::parse("minecraft:oak_log[axis=y]"))
            .unwrap();
        // Bûche couchée selon x : l'extrémité (cerne) est à l'est, comme le dessus d'une
        // bûche debout.
        assert_eq!(log_x.east.texture, log_y.top.texture);
    }
}
