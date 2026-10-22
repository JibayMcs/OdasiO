//! Aspects des blocs Vintage Story, lus dans les assets du jeu installé (rien n'est
//! téléchargé ni redistribué) : blocktypes JSON5, variantes, textures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use crate::look::{BlockLook, Element, ElementFace, Face, Shape, Texture};
use crate::mc_assets::{FACES, NORMALS, face_index, rotate, rotate_bounds};

const DOMAINS: [&str; 3] = ["game", "survival", "creative"];
const PLANT_TINT: [u8; 3] = [0x7f, 0xb2, 0x4c];
const FOLIAGE_TINT: [u8; 3] = [0x5f, 0x96, 0x34];
const WATER_TINT: [u8; 3] = [0x3a, 0x6c, 0xb0];

/// Dossier `assets` de l'installation de Vintage Story.
pub fn detect_vs_assets() -> Option<PathBuf> {
    [
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/vintagestory/assets")),
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Vintagestory/assets")),
    ]
    .into_iter()
    .flatten()
    .find(|p| p.join("survival/blocktypes").is_dir())
}

/// Accès insensible à la casse, comme le désérialiseur du jeu.
fn get<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    object
        .as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
        .filter(|v| !v.is_null())
}

/// Motif du jeu : `*` joker, ou expression régulière simple préfixée par `@` (non gérée).
fn wildcard(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern.eq_ignore_ascii_case(text);
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

/// Valeur d'un champ ou de sa variante `...ByType` (premier motif qui correspond au code).
fn by_type<'a>(object: &'a Value, key: &str, code: &str) -> Option<&'a Value> {
    if let Some(v) = get(object, key) {
        return Some(v);
    }
    get(object, &format!("{key}ByType"))?
        .as_object()?
        .iter()
        .find(|(pattern, _)| wildcard(pattern, code))
        .map(|(_, v)| v)
        .filter(|v| !v.is_null())
}

struct BlockType {
    json: Value,
    /// Code de base (`rock`) et groupes de variantes dans l'ordre.
    groups: Vec<String>,
}

pub struct VsAssets {
    root: PathBuf,
    types: Vec<BlockType>,
    /// Code complet sans domaine vers (blocktype, valeurs des groupes).
    codes: HashMap<String, (usize, Vec<String>)>,
    textures: Mutex<HashMap<String, Option<Arc<Texture>>>>,
    shapes: Mutex<HashMap<String, Option<Value>>>,
}

impl VsAssets {
    pub fn open(root: &Path) -> std::io::Result<Self> {
        let mut assets = Self {
            root: root.to_path_buf(),
            types: Vec::new(),
            codes: HashMap::new(),
            textures: Mutex::new(HashMap::new()),
            shapes: Mutex::new(HashMap::new()),
        };
        let mut properties: HashMap<String, Vec<String>> = HashMap::new();
        for domain in DOMAINS {
            let dir = root.join(domain).join("blocktypes");
            for path in walk(&dir) {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(json) = json5::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
                    tracing::debug!("blocktype illisible : {}", path.display());
                    continue;
                };
                let list = match json {
                    Value::Array(list) => list,
                    other => vec![other],
                };
                for mut blocktype in list {
                    // Héritage : les champs absents viennent du blocktype parent.
                    if let Some(parent) = get(&blocktype, "inheritFrom")
                        .and_then(Value::as_str)
                        .map(|p| root.join(domain).join(format!("{p}.json")))
                        .and_then(|p| std::fs::read_to_string(p).ok())
                        .and_then(|t| json5::from_str::<Value>(&t).ok())
                        && let (Value::Object(parent), Value::Object(child)) =
                            (parent, &mut blocktype)
                    {
                        for (key, value) in parent {
                            if !child.keys().any(|k| k.eq_ignore_ascii_case(&key)) {
                                child.insert(key, value);
                            }
                        }
                    }
                    assets.add_blocktype(blocktype, &mut properties);
                }
            }
        }
        Ok(assets)
    }

    fn property_states(&self, name: &str, cache: &mut HashMap<String, Vec<String>>) -> Vec<String> {
        let name = name.rsplit(':').next().unwrap_or(name).to_string();
        if let Some(states) = cache.get(&name) {
            return states.clone();
        }
        let states: Vec<String> = DOMAINS
            .iter()
            .map(|d| {
                self.root
                    .join(d)
                    .join("worldproperties")
                    .join(format!("{name}.json"))
            })
            .find(|p| p.is_file())
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| json5::from_str::<Value>(&t).ok())
            .and_then(|v| get(&v, "variants").and_then(Value::as_array).cloned())
            .map(|variants| {
                variants
                    .iter()
                    .filter_map(|v| get(v, "code").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        cache.insert(name, states.clone());
        states
    }

    /// Développe les variantes d'un blocktype, comme le jeu (multiply / add,
    /// allowedVariants, skipVariants).
    fn add_blocktype(&mut self, json: Value, cache: &mut HashMap<String, Vec<String>>) {
        if matches!(get(&json, "enabled"), Some(Value::Bool(false))) {
            return;
        }
        let Some(base) = get(&json, "code")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let mut multiply: Vec<(String, Vec<String>)> = Vec::new();
        let mut add: Vec<(String, Vec<String>)> = Vec::new();
        for group in get(&json, "variantgroups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            // Sans code, le groupe prend le nom de la propriété (`abstract/fertility`).
            let code = get(group, "code")
                .or_else(|| get(group, "loadFromProperties"))
                .and_then(Value::as_str)
                .map(|c| c.rsplit('/').next().unwrap_or(c).to_string())
                .unwrap_or_default();
            let mut states: Vec<String> = get(group, "states")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect();
            if let Some(prop) = get(group, "loadFromProperties").and_then(Value::as_str) {
                states.extend(self.property_states(prop, cache));
            }
            for prop in get(group, "loadFromPropertiesCombine")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(prop) = prop.as_str() {
                    states.extend(self.property_states(prop, cache));
                }
            }
            let combine = get(group, "combine")
                .and_then(Value::as_str)
                .unwrap_or("multiply");
            if combine.eq_ignore_ascii_case("add") {
                add.push((code, states));
            } else {
                multiply.push((code, states));
            }
        }

        let mut variants: Vec<(String, Vec<String>)> = Vec::new();
        if !multiply.is_empty() {
            let mut combos: Vec<Vec<String>> = vec![Vec::new()];
            for (_, states) in &multiply {
                combos = combos
                    .into_iter()
                    .flat_map(|c| {
                        states.iter().map(move |s| {
                            let mut c = c.clone();
                            c.push(s.clone());
                            c
                        })
                    })
                    .collect();
            }
            for combo in combos {
                variants.push((format!("{base}-{}", combo.join("-")), combo));
            }
        } else if add.is_empty() {
            variants.push((base.clone(), Vec::new()));
        }
        for (_, states) in &add {
            for s in states {
                variants.push((format!("{base}-{s}"), vec![s.clone()]));
            }
        }
        let patterns = |key: &str| -> Vec<String> {
            get(&json, key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        };
        let (allow, skip) = (patterns("allowedVariants"), patterns("skipVariants"));
        let mut groups: Vec<String> = multiply.iter().map(|(g, _)| g.clone()).collect();
        if multiply.is_empty() && !add.is_empty() {
            groups = vec![add[0].0.clone()];
        }
        let index = self.types.len();
        self.types.push(BlockType { json, groups });
        for (code, values) in variants {
            let code = code.to_lowercase();
            if !allow.is_empty() && !allow.iter().any(|p| wildcard(&p.to_lowercase(), &code)) {
                continue;
            }
            if skip.iter().any(|p| wildcard(&p.to_lowercase(), &code)) {
                continue;
            }
            self.codes.entry(code).or_insert((index, values));
        }
    }

    /// Blocktype JSON d'un code (diagnostic).
    pub fn blocktype(&self, code: &str) -> Option<&Value> {
        let code = code.strip_prefix("game:").unwrap_or(code);
        Some(&self.types[self.codes.get(code)?.0].json)
    }

    pub fn known_codes(&self) -> usize {
        self.codes.len()
    }

    /// Fichier d'un asset (`textures`, `shapes`) : `*` final = première alternative.
    fn asset_file(&self, kind: &str, base: &str, ext: &str) -> Option<PathBuf> {
        let (domain, path) = base.split_once(':').unwrap_or(("game", base));
        let domains: Vec<&str> = if domain == "game" {
            DOMAINS.to_vec()
        } else {
            vec![domain]
        };
        for domain in domains {
            let dir = self.root.join(domain).join(kind);
            if let Some(prefix) = path.strip_suffix('*') {
                let full = dir.join(prefix);
                let (Some(parent), Some(stem)) = (full.parent(), full.file_name()) else {
                    continue;
                };
                let stem = stem.to_string_lossy().into_owned();
                let mut files: Vec<PathBuf> = std::fs::read_dir(parent)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension().is_some_and(|e| e == ext)
                            && p.file_name()
                                .is_some_and(|n| n.to_string_lossy().starts_with(&stem))
                    })
                    .collect();
                files.sort();
                if let Some(first) = files.into_iter().next() {
                    return Some(first);
                }
                continue;
            }
            let file = dir.join(format!("{path}.{ext}"));
            if file.is_file() {
                return Some(file);
            }
        }
        None
    }

    /// Moitiés gauche (frange des côtés) et droite (dessus) d'une texture de couverture
    /// d'herbe ; une texture carrée sert aux deux.
    fn cover_halves(&self, base: &str) -> Option<(Arc<Texture>, Arc<Texture>)> {
        let bytes = std::fs::read(self.asset_file("textures", base, "png")?).ok()?;
        let image = image::load_from_memory(&bytes).ok()?;
        let (w, h) = (image.width(), image.height());
        if w >= 2 * h {
            Some((
                Arc::new(Texture::from_png_crop(&bytes, 0, 0, h)?),
                Arc::new(Texture::from_png_crop(&bytes, h, 0, h)?),
            ))
        } else {
            let whole = Arc::new(Texture::from_png(&bytes)?);
            Some((whole.clone(), whole))
        }
    }

    fn texture(&self, base: &str) -> Option<Arc<Texture>> {
        if let Some(t) = self.textures.lock().unwrap().get(base) {
            return t.clone();
        }
        let texture = self
            .asset_file("textures", base, "png")
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| Texture::from_png(&b))
            .map(Arc::new);
        self.textures
            .lock()
            .unwrap()
            .insert(base.to_string(), texture.clone());
        texture
    }

    /// Fichier de forme JSON (`shapes/<base>.json`), mis en cache.
    fn shape(&self, base: &str) -> Option<Value> {
        if let Some(s) = self.shapes.lock().unwrap().get(base) {
            return s.clone();
        }
        let shape = self
            .asset_file("shapes", base, "json")
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| json5::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok());
        self.shapes
            .lock()
            .unwrap()
            .insert(base.to_string(), shape.clone());
        shape
    }

    /// Boîte englobante des éléments (enfants décalés par leur parent) et, pour chaque
    /// direction, la texture de l'élément le plus avancé. Les rotations d'éléments sont
    /// ignorées : c'est un aperçu.
    fn shape_look(
        &self,
        shape: &Value,
        rx: i32,
        ry: i32,
        face: &dyn Fn(&str) -> Option<Face>,
    ) -> Option<BlockLook> {
        let mut min = [16.0f32; 3];
        let mut max = [0.0f32; 3];
        let mut faces: [Option<(f32, String)>; 6] = Default::default();
        let mut elements: Vec<Element> = Vec::new();
        let mut stack: Vec<(&Value, [f32; 3])> = get(shape, "elements")?
            .as_array()?
            .iter()
            .map(|e| (e, [0.0; 3]))
            .collect();
        while let Some((element, offset)) = stack.pop() {
            let read = |key: &str| -> Option<[f32; 3]> {
                let a = get(element, key)?.as_array()?;
                Some([0, 1, 2].map(|i| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32))
            };
            let (Some(from), Some(to)) = (read("from"), read("to")) else {
                continue;
            };
            let from = [0, 1, 2].map(|i| from[i] + offset[i]);
            let to = [0, 1, 2].map(|i| to[i] + offset[i]);
            if let Some(Value::Array(children)) = get(element, "children") {
                stack.extend(children.iter().map(|c| (c, from)));
            }
            let Some(Value::Object(element_faces)) = get(element, "faces") else {
                continue;
            };
            // Élément détaillé : faces dans l'ordre dessus, dessous, sud, nord, est, ouest,
            // UV en unités de texture de la forme.
            let names = ["up", "down", "south", "north", "east", "west"];
            let detailed: [Option<ElementFace>; 6] = std::array::from_fn(|d| {
                let f = element_faces
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(names[d]))
                    .map(|(_, v)| v)?;
                if f.get("enabled") == Some(&Value::Bool(false)) {
                    return None;
                }
                let key = f.get("texture").and_then(Value::as_str)?;
                if key == "#null" {
                    return None;
                }
                let resolved = face(key)?;
                let (w, h) = texture_size(shape, key.trim_start_matches('#'));
                let uv = f
                    .get("uv")
                    .and_then(Value::as_array)
                    .map(|a| {
                        let v = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                        [v(0) / w, v(1) / h, v(2) / w, v(3) / h]
                    })
                    .unwrap_or([0.0, 0.0, 1.0, 1.0]);
                Some(ElementFace {
                    texture: resolved.texture,
                    uv,
                    tint: resolved.tint,
                    cull: false,
                })
            });
            if detailed.iter().any(Option::is_some) {
                let mut element = Element {
                    from: from.map(|x| x / 16.0),
                    to: to.map(|x| x / 16.0),
                    faces: detailed,
                }
                .rotated(rx, ry);
                element.mark_cull();
                elements.push(element);
            }
            let mut visible = false;
            for (f, name) in FACES.iter().enumerate() {
                let Some(element_face) = element_faces
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v)
                else {
                    continue;
                };
                if element_face.get("enabled") == Some(&Value::Bool(false)) {
                    continue;
                }
                let Some(texture) = element_face.get("texture").and_then(Value::as_str) else {
                    continue;
                };
                if texture == "#null" {
                    continue;
                }
                visible = true;
                let reach = match f {
                    0 => to[0],
                    1 => -from[0],
                    2 => to[1],
                    3 => -from[1],
                    4 => to[2],
                    _ => -from[2],
                };
                if faces[f].as_ref().is_none_or(|(r, _)| reach > *r) {
                    faces[f] = Some((reach, texture.to_string()));
                }
            }
            if visible {
                for i in 0..3 {
                    min[i] = min[i].min(from[i].min(to[i]));
                    max[i] = max[i].max(from[i].max(to[i]));
                }
            }
        }
        if min[0] >= max[0] && min[1] >= max[1] {
            return None;
        }

        let source = |target: [i32; 3]| -> usize {
            (0..6)
                .find(|&f| rotate(NORMALS[f], rx, ry) == target)
                .unwrap_or_else(|| face_index(target))
        };
        let pick = |target: [i32; 3]| -> Option<Face> {
            let (_, key) = faces[source(target)]
                .clone()
                .or_else(|| faces.iter().flatten().next().cloned())?;
            face(&key)
        };
        let (top, south, east) = (pick([0, 1, 0])?, pick([0, 0, 1])?, pick([1, 0, 0])?);
        let (lo, hi) = rotate_bounds(min, max, rx, ry);
        Some(BlockLook::cuboid(lo, hi, top, south, east).with_elements(elements))
    }

    pub fn look(&self, full_code: &str) -> Option<BlockLook> {
        let code = full_code
            .strip_prefix("game:")
            .unwrap_or(full_code)
            .to_lowercase();
        if code == "air" {
            return None;
        }
        if code == "mantle" {
            let texture = self
                .texture("block/mantle")
                .or_else(|| self.texture("block/stone/rock/basalt*"))?;
            let face = Face {
                texture,
                tint: None,
            };
            return Some(BlockLook::cuboid(
                [0.0; 3],
                [1.0; 3],
                face.clone(),
                face.clone(),
                face,
            ));
        }
        let (index, values) = self.codes.get(&code)?;
        let blocktype = &self.types[*index];
        let json = &blocktype.json;
        let substitute = |text: &str| -> String {
            let mut out = text.to_string();
            for (group, value) in blocktype.groups.iter().zip(values) {
                out = out.replace(&format!("{{{group}}}"), value);
            }
            out
        };

        // Textures : table de base puis surcharges par type ; une entrée peut elle-même être
        // une table de motifs.
        let mut textures: Map<String, Value> = Map::new();
        if let Some(Value::Object(base)) = get(json, "textures") {
            textures.extend(base.clone());
        }
        if let Some(Value::Object(by_type)) = get(json, "texturesByType")
            && let Some((_, Value::Object(over))) = by_type
                .iter()
                .find(|(p, _)| wildcard(&p.to_lowercase(), &code))
        {
            textures.extend(over.clone());
        }
        // `cleByType` : même clé, la valeur est une table de motifs que `base_of` sait lire.
        let textures: Map<String, Value> = textures
            .into_iter()
            .map(|(k, v)| {
                let key = match k.len().checked_sub(6) {
                    Some(n) if k.is_char_boundary(n) && k[n..].eq_ignore_ascii_case("bytype") => {
                        k[..n].to_string()
                    }
                    _ => k,
                };
                (key, v)
            })
            .collect();
        let base_of = |entry: &Value| -> Option<String> {
            if let Some(base) = by_type(entry, "base", &code).and_then(Value::as_str) {
                return Some(substitute(base));
            }
            let by_pattern = entry
                .as_object()?
                .iter()
                .find(|(p, _)| wildcard(&p.to_lowercase(), &code))?
                .1;
            by_type(by_pattern, "base", &code)
                .and_then(Value::as_str)
                .map(&substitute)
        };
        const IGNORED: [&str; 4] = ["specialsecondtexture", "particletop", "snowed", "inside"];
        let first = textures
            .iter()
            .filter(|(k, _)| !IGNORED.contains(&k.to_lowercase().as_str()))
            .find_map(|(_, v)| base_of(v));
        let pick = |keys: &[&str]| -> Option<String> {
            keys.iter()
                .find_map(|k| {
                    textures
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(k))
                })
                .and_then(|(_, v)| base_of(v))
                .or_else(|| first.clone())
        };

        let tint = by_type(json, "climateColorMap", &code)
            .and_then(Value::as_str)
            .map(|map| {
                if map.to_lowercase().contains("water") {
                    WATER_TINT
                } else if code.starts_with("leaves") {
                    FOLIAGE_TINT
                } else {
                    PLANT_TINT
                }
            });
        let face = |base: Option<String>, tint: Option<[u8; 3]>| -> Option<Face> {
            Some(Face {
                texture: self.texture(&base?)?,
                tint,
            })
        };
        let draw = by_type(json, "drawtype", &code)
            .and_then(Value::as_str)
            .unwrap_or("json")
            .to_lowercase();
        let shape = by_type(json, "shape", &code);
        let shape_base = shape
            .and_then(|s| by_type(s, "base", &code))
            .and_then(Value::as_str)
            .map(&substitute)
            .unwrap_or_default()
            .to_lowercase();

        if draw == "liquid" {
            return Some(BlockLook::single(
                Shape::Liquid,
                face(pick(&["all"]), tint)?,
            ));
        }
        let plant_shape = [
            "plant/", "flower", "grass", "fern", "reed", "crop", "sapling", "mushroom",
        ]
        .iter()
        .any(|k| shape_base.contains(k))
            && !shape_base.contains("leaves");
        let shaped = !shape_base.is_empty()
            && !shape_base.starts_with("block/basic/cube")
            && !matches!(draw.as_str(), "cube" | "topsoil" | "cross" | "liquid")
            && !code.contains("slab")
            && !code.starts_with("snowlayer");
        // Texture d'une face de forme : clé du bloc, puis `all` du bloc, puis texture de la forme.
        let shape_json = shaped.then(|| self.shape(&shape_base)).flatten();
        let resolve = |key: &str| -> Option<String> {
            let key = key.trim_start_matches('#');
            textures
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .or_else(|| {
                    textures
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("all"))
                })
                .and_then(|(_, v)| base_of(v))
                .or_else(|| {
                    let shape_textures = get(shape_json.as_ref()?, "textures")?;
                    get(shape_textures, key)
                        .and_then(Value::as_str)
                        .map(&substitute)
                })
        };

        if draw == "cross" || (shaped && plant_shape) {
            let base = pick(&["all"]).or_else(|| {
                let shape = shape_json.as_ref()?;
                first_face_texture(shape).and_then(|k| resolve(&k))
            });
            return Some(BlockLook::single(Shape::Cross, face(base, tint)?));
        }
        if let Some(shape_json) = &shape_json {
            let angle = |key: &str| {
                shape
                    .and_then(|s| by_type(s, key, &code))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as i32
            };
            // VS tourne dans le sens trigonométrique vu de dessus, Minecraft dans l'autre.
            let (rx, ry) = (angle("rotateX"), -angle("rotateY"));
            // Teinte climatique (herbes, fougères, feuillages) sur toutes les faces.
            if let Some(look) = self.shape_look(shape_json, rx, ry, &|key| face(resolve(key), tint))
            {
                return Some(look);
            }
        }

        let mut top = face(pick(&["up", "verticals", "all"]), None)?;
        let mut south = face(pick(&["south", "horizontals", "sides", "all"]), None)?;
        let mut east = face(pick(&["east", "horizontals", "sides", "all"]), None)?;
        // Herbe : la texture de couverture est double, frange des côtés à gauche et dessus à
        // droite ; chaque moitié est superposée à la terre, teintée.
        if let Some(cover) = textures
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("specialSecondTexture"))
            .and_then(|(_, v)| base_of(v))
            && !code.ends_with("-none")
            && let Some((side_cover, top_cover)) = self.cover_halves(&cover)
        {
            let tint = tint.unwrap_or([255; 3]);
            top = Face {
                texture: Arc::new(composite(&top.texture, &top_cover, tint)),
                tint: None,
            };
            let side = Arc::new(composite(&south.texture, &side_cover, tint));
            south = Face {
                texture: side.clone(),
                tint: None,
            };
            east = Face {
                texture: side,
                tint: None,
            };
        } else if tint.is_some() && draw != "json" {
            top.tint = tint;
        }
        if code.starts_with("leaves") {
            top.tint = tint;
            south.tint = tint;
            east.tint = tint;
        }

        let (mut min, mut max) = ([0.0f32; 3], [1.0f32; 3]);
        if let Some(n) = code
            .strip_prefix("snowlayer-")
            .and_then(|n| n.parse::<f32>().ok())
        {
            max[1] = n / 8.0;
        } else if code.contains("slab") {
            let parts: Vec<&str> = code.split('-').collect();
            if parts.contains(&"down") {
                max[1] = 0.5;
            } else if parts.contains(&"up") {
                min[1] = 0.5;
            } else if parts.contains(&"north") || parts.contains(&"n") {
                max[2] = 0.5;
            } else if parts.contains(&"south") || parts.contains(&"s") {
                min[2] = 0.5;
            } else if parts.contains(&"west") || parts.contains(&"w") {
                max[0] = 0.5;
            } else if parts.contains(&"east") || parts.contains(&"e") {
                min[0] = 0.5;
            }
        } else if code.starts_with("carpet") || code.starts_with("smallcarpet") {
            max[1] = 1.0 / 16.0;
        }
        Some(BlockLook::cuboid(min, max, top, south, east))
    }
}

/// Taille (largeur, hauteur) de référence des UV d'une texture de la forme.
fn texture_size(shape: &Value, key: &str) -> (f32, f32) {
    let size = |v: Option<&Value>| v.and_then(Value::as_f64).map(|x| x as f32);
    if let Some(Value::Array(s)) = get(shape, "textureSizes").and_then(|t| get(t, key)) {
        let (w, h) = (size(s.first()), size(s.get(1)));
        if let (Some(w), Some(h)) = (w, h) {
            return (w, h);
        }
    }
    (
        size(get(shape, "textureWidth")).unwrap_or(16.0),
        size(get(shape, "textureHeight")).unwrap_or(16.0),
    )
}

/// Première texture référencée par une face d'élément.
fn first_face_texture(shape: &Value) -> Option<String> {
    let mut stack: Vec<&Value> = get(shape, "elements")?.as_array()?.iter().collect();
    while let Some(element) = stack.pop() {
        if let Some(Value::Object(faces)) = get(element, "faces") {
            for face in faces.values() {
                if face.get("enabled") == Some(&Value::Bool(false)) {
                    continue;
                }
                if let Some(t) = face.get("texture").and_then(Value::as_str)
                    && t != "#null"
                {
                    return Some(t.to_string());
                }
            }
        }
        if let Some(Value::Array(children)) = get(element, "children") {
            stack.extend(children);
        }
    }
    None
}

/// Superpose `cover` (teintée) sur `base`, à la taille de `base`.
fn composite(base: &Texture, cover: &Texture, tint: [u8; 3]) -> Texture {
    let mut out = base.clone();
    let (w, h) = (base.width as usize, base.height as usize);
    for y in 0..h {
        for x in 0..w {
            let c = cover.sample((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let a = c[3] as u32;
            let i = (y * w + x) * 4;
            for k in 0..3 {
                let tinted = c[k] as u32 * tint[k] as u32 / 255;
                out.rgba[i + k] = ((tinted * a + out.rgba[i + k] as u32 * (255 - a)) / 255) as u8;
            }
        }
    }
    out
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_looks_from_the_installed_game() {
        let Some(root) = detect_vs_assets() else {
            eprintln!("Vintage Story absent, test ignoré");
            return;
        };
        let assets = VsAssets::open(&root).unwrap();
        // Même ordre de grandeur que l'inventaire de référence (14 136 codes).
        assert!(assets.known_codes() > 13_000, "{}", assets.known_codes());
        let granite = assets.look("game:rock-granite").unwrap();
        assert!(granite.opaque);
        let grass = assets.look("game:soil-medium-normal").unwrap();
        let dirt = assets.look("game:soil-medium-none").unwrap();
        assert_ne!(
            grass.top.texture, dirt.top.texture,
            "couverture d'herbe superposée"
        );
        assert_eq!(
            assets.look("game:water-still-7").unwrap().shape,
            Shape::Liquid
        );
        let slab = assets.look("game:plankslab-oak-down-free").unwrap();
        assert!(matches!(slab.shape, Shape::Cuboid { max, .. } if max[1] == 0.5));
        assert!(assets.look("game:air").is_none());
    }
}
