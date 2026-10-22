use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

/// État de bloc Minecraft : identifiant namespacé + propriétés (`axis`, `facing`...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
pub struct BlockState {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Properties", default)]
    pub properties: BTreeMap<String, String>,
}

impl BlockState {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            properties: BTreeMap::new(),
        }
    }

    /// Lit la notation `minecraft:oak_log[axis=y]`.
    pub fn parse(text: &str) -> Self {
        let (name, props) = match text.split_once('[') {
            Some((name, rest)) => (name, rest.trim_end_matches(']')),
            None => (text, ""),
        };
        let properties = props
            .split(',')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Self {
            name: name.to_string(),
            properties,
        }
    }

    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    pub fn is_air(&self) -> bool {
        matches!(
            self.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }

    /// Espace de noms (`minecraft`, ou l'identifiant d'un mod).
    pub fn namespace(&self) -> &str {
        self.name.split_once(':').map_or("minecraft", |(ns, _)| ns)
    }
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.properties.is_empty() {
            f.write_str("[")?;
            for (i, (k, v)) in self.properties.iter().enumerate() {
                if i > 0 {
                    f.write_str(",")?;
                }
                write!(f, "{k}={v}")?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrips_with_display() {
        for text in [
            "minecraft:stone",
            "minecraft:oak_log[axis=y]",
            "minecraft:snow[layers=7]",
        ] {
            assert_eq!(BlockState::parse(text).to_string(), text);
        }
    }
}
