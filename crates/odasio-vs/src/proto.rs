//! Protobuf minimal : les blobs VS sont des messages protobuf-net, lus avec un décodeur
//! standard côté jeu. L'ordre des champs est libre.

use crate::{Result, VsError};

#[derive(Default)]
pub struct ProtoWriter {
    buf: Vec<u8>,
}

impl ProtoWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.buf.push((v as u8) | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
    }

    fn tag(&mut self, field: u32, wire: u8) {
        self.varint(((field as u64) << 3) | wire as u64);
    }

    pub fn uint(&mut self, field: u32, v: u64) -> &mut Self {
        self.tag(field, 0);
        self.varint(v);
        self
    }

    /// `int32` protobuf : un négatif s'écrit en complément à deux sur 10 octets.
    pub fn int32(&mut self, field: u32, v: i32) -> &mut Self {
        self.uint(field, v as i64 as u64)
    }

    pub fn bool(&mut self, field: u32, v: bool) -> &mut Self {
        self.uint(field, v as u64)
    }

    /// Entiers répétés en encodage compacté : une étiquette et une longueur pour tout le
    /// tableau, au lieu d'une étiquette par valeur.
    pub fn packed_uints(&mut self, field: u32, values: impl IntoIterator<Item = u64>) -> &mut Self {
        let mut inner = ProtoWriter::new();
        for v in values {
            inner.varint(v);
        }
        self.bytes(field, &inner.buf)
    }

    pub fn bytes(&mut self, field: u32, data: &[u8]) -> &mut Self {
        self.tag(field, 2);
        self.varint(data.len() as u64);
        self.buf.extend_from_slice(data);
        self
    }

    pub fn string(&mut self, field: u32, s: &str) -> &mut Self {
        self.bytes(field, s.as_bytes())
    }

    pub fn message(&mut self, field: u32, build: impl FnOnce(&mut ProtoWriter)) -> &mut Self {
        let mut inner = ProtoWriter::new();
        build(&mut inner);
        self.bytes(field, &inner.buf)
    }

    /// Recopie un champ déjà encodé tel quel.
    pub fn raw(&mut self, encoded_field: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(encoded_field);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

impl<'a> Value<'a> {
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            Value::Varint(v) | Value::Fixed64(v) => Some(v),
            Value::Fixed32(v) => Some(v as u64),
            Value::Bytes(_) => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match *self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    pub number: u32,
    pub value: Value<'a>,
    /// Encodage complet du champ (tag compris), pour le recopier à l'identique.
    pub raw: &'a [u8],
}

pub struct ProtoReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn varint(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self
                .data
                .get(self.pos)
                .ok_or_else(|| VsError::Corrupt("varint tronqué".into()))?;
            self.pos += 1;
            v |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(VsError::Corrupt("varint trop long".into()))
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| VsError::Corrupt("champ tronqué".into()))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn next_field(&mut self) -> Result<Field<'a>> {
        let start = self.pos;
        let key = self.varint()?;
        let number = (key >> 3) as u32;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => Value::Fixed64(u64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            2 => {
                let len = self.varint()? as usize;
                Value::Bytes(self.take(len)?)
            }
            5 => Value::Fixed32(u32::from_le_bytes(self.take(4)?.try_into().unwrap())),
            wire => return Err(VsError::Corrupt(format!("type protobuf {wire} non géré"))),
        };
        Ok(Field {
            number,
            value,
            raw: &self.data[start..self.pos],
        })
    }
}

impl<'a> Iterator for ProtoReader<'a> {
    type Item = Result<Field<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let field = self.next_field();
        if field.is_err() {
            self.pos = self.data.len();
        }
        Some(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_fields() {
        let mut w = ProtoWriter::new();
        w.uint(1, 300).int32(2, -1).string(3, "1.22.7");
        w.message(4, |m| {
            m.uint(1, 7);
        });
        let bytes = w.into_bytes();
        let fields: Vec<_> = ProtoReader::new(&bytes).collect::<Result<_>>().unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0].value.as_u64(), Some(300));
        assert_eq!(fields[1].value.as_u64().map(|v| v as i64 as i32), Some(-1));
        assert_eq!(fields[2].value.as_bytes(), Some(&b"1.22.7"[..]));
        let inner: Vec<_> = ProtoReader::new(fields[3].value.as_bytes().unwrap())
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(inner[0].value.as_u64(), Some(7));
    }
}
