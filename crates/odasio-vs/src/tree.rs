//! `TreeAttribute` du jeu (format binaire des block entities) et blob d'une block entity.
//! Format décrit dans `docs/vs-block-entities.md`, section 1.

/// Valeur d'attribut. Les identifiants de type sont ceux de `AttributeIdMapping`.
#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Int(i32),
    Long(i64),
    Double(f64),
    Float(f32),
    Str(String),
    Tree(Tree),
    Bool(bool),
}

impl Attr {
    fn type_id(&self) -> u8 {
        match self {
            Attr::Int(_) => 1,
            Attr::Long(_) => 2,
            Attr::Double(_) => 3,
            Attr::Float(_) => 4,
            Attr::Str(_) => 5,
            Attr::Tree(_) => 6,
            Attr::Bool(_) => 9,
        }
    }
}

/// Arbre ordonné de clés : le jeu écrit dans l'ordre d'insertion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree(pub Vec<(String, Attr)>);

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: &str, value: Attr) -> &mut Self {
        self.0.retain(|(k, _)| k != key);
        self.0.push((key.to_string(), value));
        self
    }

    pub fn get(&self, key: &str) -> Option<&Attr> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn write(&self, out: &mut Vec<u8>) {
        for (key, value) in &self.0 {
            out.push(value.type_id());
            write_string(out, key);
            match value {
                Attr::Int(v) => out.extend_from_slice(&v.to_le_bytes()),
                Attr::Long(v) => out.extend_from_slice(&v.to_le_bytes()),
                Attr::Double(v) => out.extend_from_slice(&v.to_le_bytes()),
                Attr::Float(v) => out.extend_from_slice(&v.to_le_bytes()),
                Attr::Str(v) => write_string(out, v),
                Attr::Tree(v) => v.write(out),
                Attr::Bool(v) => out.push(*v as u8),
            }
        }
        // Un octet nul termine chaque arbre.
        out.push(0);
    }
}

/// Chaîne .NET `BinaryWriter` : longueur en octets UTF-8 en varint 7 bits, puis les octets.
fn write_string(out: &mut Vec<u8>, s: &str) {
    let mut n = s.len();
    while n >= 0x80 {
        out.push((n as u8 & 0x7f) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
    out.extend_from_slice(s.as_bytes());
}

/// Lecteur binaire du format .NET `BinaryReader`.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let out = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(out)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let mut len = 0usize;
        for shift in (0..35).step_by(7) {
            let b = self.u8()?;
            len |= ((b & 0x7f) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
        }
        Some(String::from_utf8_lossy(self.take(len)?).into_owned())
    }

    fn tree(&mut self) -> Option<Tree> {
        let mut tree = Tree::new();
        loop {
            let kind = self.u8()?;
            if kind == 0 {
                return Some(tree);
            }
            let key = self.string()?;
            let value = match kind {
                1 => Attr::Int(self.i32()?),
                2 => Attr::Long(i64::from_le_bytes(self.take(8)?.try_into().ok()?)),
                3 => Attr::Double(f64::from_le_bytes(self.take(8)?.try_into().ok()?)),
                4 => Attr::Float(f32::from_le_bytes(self.take(4)?.try_into().ok()?)),
                5 => Attr::Str(self.string()?),
                6 => Attr::Tree(self.tree()?),
                9 => Attr::Bool(self.u8()? != 0),
                // Types non utilisés par la conversion : sautés sans être conservés.
                7 => {
                    if self.u8()? == 0 {
                        self.take(12)?;
                        self.tree()?;
                    }
                    continue;
                }
                8 => {
                    let len = u16::from_le_bytes(self.take(2)?.try_into().ok()?) as usize;
                    self.take(len)?;
                    continue;
                }
                10 => {
                    for _ in 0..self.i32()? {
                        self.string()?;
                    }
                    continue;
                }
                11 | 12 => {
                    let n = self.i32()? as usize;
                    self.take(n * 4)?;
                    continue;
                }
                13 | 15 => {
                    let n = self.i32()? as usize;
                    self.take(n * 8)?;
                    continue;
                }
                14 => {
                    for _ in 0..self.i32()? {
                        self.tree()?;
                    }
                    continue;
                }
                16 => {
                    let n = self.i32()? as usize;
                    self.take(n)?;
                    continue;
                }
                _ => return None,
            };
            tree.0.push((key, value));
        }
    }
}

/// Block entity à écrire dans le champ 8 du chunk qui contient sa position.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockEntity {
    /// Nom de classe enregistré dans le jeu (`Sign`, `Generic`, `GenericTypedContainer`...).
    pub class: String,
    /// Position absolue du bloc.
    pub pos: [i32; 3],
    /// Code court du bloc, sans le domaine `game:`.
    pub block_code: String,
    /// Attributs propres à la classe, ajoutés après les clés de base.
    pub attrs: Tree,
}

impl BlockEntity {
    pub fn new(class: &str, pos: [i32; 3], block_code: &str) -> Self {
        Self {
            class: class.to_string(),
            pos,
            block_code: block_code
                .strip_prefix("game:")
                .unwrap_or(block_code)
                .to_string(),
            attrs: Tree::new(),
        }
    }

    /// Lit un élément du champ 8 d'un chunk : nom de classe puis arbre d'attributs.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let mut reader = Reader { data, pos: 0 };
        let class = reader.string()?;
        let mut attrs = reader.tree()?;
        let int = |t: &Tree, k: &str| match t.get(k) {
            Some(Attr::Int(v)) => Some(*v),
            _ => None,
        };
        let pos = [
            int(&attrs, "posx")?,
            int(&attrs, "posy")?,
            int(&attrs, "posz")?,
        ];
        let block_code = match attrs.get("blockCode") {
            Some(Attr::Str(code)) => code.clone(),
            _ => String::new(),
        };
        attrs
            .0
            .retain(|(k, _)| !matches!(k.as_str(), "posx" | "posy" | "posz" | "blockCode"));
        Some(Self {
            class,
            pos,
            block_code,
            attrs,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut tree = Tree::new();
        tree.set("posx", Attr::Int(self.pos[0]))
            .set("posy", Attr::Int(self.pos[1]))
            .set("posz", Attr::Int(self.pos[2]))
            .set("blockCode", Attr::Str(self.block_code.clone()));
        for (key, value) in &self.attrs.0 {
            tree.set(key, value.clone());
        }
        let mut out = Vec::new();
        write_string(&mut out, &self.class);
        tree.write(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_documented_sign_blob() {
        // Exemple de docs/vs-block-entities.md, section 1.4.
        let mut sign = BlockEntity::new("Sign", [512002, 4, 512004], "game:sign-ground-north");
        sign.attrs
            .set("text", Attr::Str("Hi".into()))
            .set("meshAngle", Attr::Float(-std::f32::consts::FRAC_PI_2));
        let expected: Vec<u8> = [
            "04 53 69 67 6e",
            "01 04 70 6f 73 78 02 d0 07 00",
            "01 04 70 6f 73 79 04 00 00 00",
            "01 04 70 6f 73 7a 04 d0 07 00",
            "05 09 62 6c 6f 63 6b 43 6f 64 65",
            "11 73 69 67 6e 2d 67 72 6f 75 6e 64 2d 6e 6f 72 74 68",
            "05 04 74 65 78 74 02 48 69",
            "04 09 6d 65 73 68 41 6e 67 6c 65 db 0f c9 bf",
            "00",
        ]
        .join(" ")
        .split(' ')
        .map(|h| u8::from_str_radix(h, 16).unwrap())
        .collect();
        assert_eq!(sign.to_bytes(), expected);
    }

    #[test]
    fn block_entities_roundtrip() {
        let mut door = BlockEntity::new("Generic", [512000, 140, -3], "game:door-solid-oak");
        door.attrs
            .set("rotateYRad", Attr::Float(std::f32::consts::PI))
            .set("opened", Attr::Bool(true))
            .set("inventory", Attr::Tree(Tree::new()));
        let read = BlockEntity::from_bytes(&door.to_bytes()).unwrap();
        assert_eq!(read, door);
    }

    #[test]
    fn long_strings_use_multibyte_length() {
        let mut out = Vec::new();
        write_string(&mut out, &"a".repeat(200));
        assert_eq!(&out[..2], &[0xc8, 0x01]);
    }
}
