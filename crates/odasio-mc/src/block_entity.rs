//! Block entities Minecraft utiles à la conversion (texte des panneaux).

use fastnbt::Value;

/// Texte d'un panneau, aplati en texte brut.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignText {
    /// Lignes de la face avant (jusqu'à 4).
    pub lines: Vec<String>,
    /// Couleur de teinture Minecraft (`black`, `red`...), `None` = noir.
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McBlockEntity {
    /// Identifiant namespacé (`minecraft:sign`).
    pub id: String,
    /// Position absolue.
    pub pos: [i32; 3],
    pub sign: Option<SignText>,
}

fn get<'a>(compound: &'a Value, key: &str) -> Option<&'a Value> {
    match compound {
        Value::Compound(map) => map.get(key),
        _ => None,
    }
}

fn as_i32(value: Option<&Value>) -> Option<i32> {
    match value? {
        Value::Int(v) => Some(*v),
        Value::Short(v) => Some(*v as i32),
        Value::Byte(v) => Some(*v as i32),
        Value::Long(v) => Some(*v as i32),
        _ => None,
    }
}

fn as_str(value: Option<&Value>) -> Option<&str> {
    match value? {
        Value::String(s) => Some(s),
        _ => None,
    }
}

/// Aplatit un composant de texte JSON (`"Bonjour"`, `{"text":..,"extra":[..]}`, `[..]`).
fn flatten_json(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Array(parts) => parts.iter().for_each(|p| flatten_json(p, out)),
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(text)) = map.get("text") {
                out.push_str(text);
            } else if let Some(serde_json::Value::String(key)) = map.get("translate") {
                out.push_str(key);
            }
            if let Some(extra) = map.get("extra") {
                flatten_json(extra, out);
            }
        }
        _ => {}
    }
}

/// Aplatit un composant de texte stocké en NBT : chaîne JSON (1.13 à 1.21.4), chaîne brute
/// ou composé (1.21.5+).
fn flatten_component(value: &Value, out: &mut String) {
    match value {
        Value::String(s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(json) => flatten_json(&json, out),
            Err(_) => out.push_str(s),
        },
        Value::List(parts) => parts.iter().for_each(|p| flatten_component(p, out)),
        Value::Compound(_) => {
            if let Some(text) = as_str(get(value, "text")) {
                out.push_str(text);
            } else if let Some(key) = as_str(get(value, "translate")) {
                out.push_str(key);
            }
            if let Some(extra) = get(value, "extra") {
                flatten_component(extra, out);
            }
        }
        _ => {}
    }
}

fn component_text(value: &Value) -> String {
    let mut out = String::new();
    flatten_component(value, &mut out);
    out
}

fn sign_text(be: &Value) -> Option<SignText> {
    // 1.20+ : face avant dans `front_text`.
    if let Some(front) = get(be, "front_text") {
        let lines = match get(front, "messages") {
            Some(Value::List(messages)) => messages.iter().map(component_text).collect(),
            _ => Vec::new(),
        };
        return Some(SignText {
            lines,
            color: as_str(get(front, "color")).map(str::to_string),
        });
    }
    // Avant 1.20 : `Text1` à `Text4`, couleur à la racine (1.14+).
    let lines: Vec<String> = (1..=4)
        .filter_map(|i| get(be, &format!("Text{i}")).map(component_text))
        .collect();
    (!lines.is_empty()).then(|| SignText {
        lines,
        color: as_str(get(be, "Color")).map(str::to_string),
    })
}

/// Identifiants d'avant 1.11 (`Sign`, `Chest`) ramenés au format namespacé.
fn normalize_id(id: &str) -> String {
    if id.contains(':') {
        return id.to_string();
    }
    let mut snake = String::new();
    for (i, c) in id.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            snake.push('_');
        }
        snake.extend(c.to_lowercase());
    }
    format!("minecraft:{snake}")
}

pub fn parse(value: &Value) -> Option<McBlockEntity> {
    let id = normalize_id(as_str(get(value, "id"))?);
    let pos = [
        as_i32(get(value, "x"))?,
        as_i32(get(value, "y"))?,
        as_i32(get(value, "z"))?,
    ];
    let sign = id.ends_with("sign").then(|| sign_text(value)).flatten();
    Some(McBlockEntity { id, pos, sign })
}

impl SignText {
    /// Texte prêt pour un panneau VS : lignes jointes par `\n`, lignes vides finales retirées.
    pub fn joined(&self) -> String {
        let mut lines: Vec<&str> = self.lines.iter().map(|l| l.trim_end()).collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn compound(entries: Vec<(&str, Value)>) -> Value {
        Value::Compound(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<HashMap<_, _>>(),
        )
    }

    #[test]
    fn reads_modern_sign_with_json_messages() {
        let be = compound(vec![
            ("id", Value::String("minecraft:oak_sign".into())),
            ("x", Value::Int(10)),
            ("y", Value::Int(64)),
            ("z", Value::Int(-3)),
            (
                "front_text",
                compound(vec![
                    (
                        "messages",
                        Value::List(vec![
                            Value::String(r#"{"text":"Bien","extra":[{"text":"venue"}]}"#.into()),
                            Value::String(r#""à Riveciel""#.into()),
                            Value::String(r#""""#.into()),
                            Value::String(r#""""#.into()),
                        ]),
                    ),
                    ("color", Value::String("red".into())),
                ]),
            ),
        ]);
        let parsed = parse(&be).unwrap();
        assert_eq!(parsed.pos, [10, 64, -3]);
        let sign = parsed.sign.unwrap();
        assert_eq!(sign.joined(), "Bienvenue\nà Riveciel");
        assert_eq!(sign.color.as_deref(), Some("red"));
    }

    #[test]
    fn reads_nbt_components_and_legacy_signs() {
        let modern = compound(vec![
            ("id", Value::String("minecraft:oak_wall_sign".into())),
            ("x", Value::Int(0)),
            ("y", Value::Int(0)),
            ("z", Value::Int(0)),
            (
                "front_text",
                compound(vec![(
                    "messages",
                    Value::List(vec![
                        compound(vec![("text", Value::String("Taverne".into()))]),
                        Value::String("ouverte".into()),
                    ]),
                )]),
            ),
        ]);
        assert_eq!(
            parse(&modern).unwrap().sign.unwrap().joined(),
            "Taverne\nouverte"
        );

        let legacy = compound(vec![
            ("id", Value::String("Sign".into())),
            ("x", Value::Int(1)),
            ("y", Value::Int(2)),
            ("z", Value::Int(3)),
            ("Text1", Value::String("Mine".into())),
            ("Text2", Value::String(r#"{"text":"au nord"}"#.into())),
        ]);
        let parsed = parse(&legacy).unwrap();
        assert_eq!(parsed.id, "minecraft:sign");
        assert_eq!(parsed.sign.unwrap().joined(), "Mine\nau nord");
    }
}
