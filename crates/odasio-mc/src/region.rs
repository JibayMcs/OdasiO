//! Fichiers région Anvil (`r.<x>.<z>.mca`) : 32x32 chunks, secteurs de 4 Kio.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use flate2::read::{GzDecoder, ZlibDecoder};

use crate::{McError, Result};

const SECTOR: u64 = 4096;

/// Position d'un chunk dans la grille absolue (en chunks de 16x16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkLocation {
    pub x: i32,
    pub z: i32,
}

pub struct Region {
    path: PathBuf,
    file: File,
    pub x: i32,
    pub z: i32,
    /// Table des emplacements : (offset en secteurs, nombre de secteurs) par chunk local.
    offsets: [(u32, u8); 1024],
}

impl Region {
    /// Coordonnées de région déduites du nom `r.<x>.<z>.mca`.
    pub fn coords_from_name(path: &Path) -> Option<(i32, i32)> {
        let name = path.file_name()?.to_str()?;
        let mut parts = name.strip_prefix("r.")?.strip_suffix(".mca")?.split('.');
        let x = parts.next()?.parse().ok()?;
        let z = parts.next()?.parse().ok()?;
        parts.next().is_none().then_some((x, z))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let (x, z) = Self::coords_from_name(path).ok_or_else(|| {
            McError::Corrupt(format!("nom de région invalide : {}", path.display()))
        })?;
        let mut file = File::open(path)?;
        let mut offsets = [(0u32, 0u8); 1024];
        if file.metadata()?.len() >= SECTOR {
            let mut header = [0u8; SECTOR as usize];
            file.read_exact(&mut header)?;
            for (i, entry) in header.chunks_exact(4).enumerate() {
                let offset = u32::from_be_bytes([0, entry[0], entry[1], entry[2]]);
                offsets[i] = (offset, entry[3]);
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            file,
            x,
            z,
            offsets,
        })
    }

    /// Chunks présents dans la région, en coordonnées absolues.
    pub fn chunks(&self) -> impl Iterator<Item = ChunkLocation> + '_ {
        self.offsets
            .iter()
            .enumerate()
            .filter(|(_, (off, count))| *off >= 2 && *count > 0)
            .map(|(i, _)| ChunkLocation {
                x: self.x * 32 + (i % 32) as i32,
                z: self.z * 32 + (i / 32) as i32,
            })
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks().count()
    }

    /// NBT décompressé du chunk, ou `None` s'il n'a jamais été généré.
    pub fn read_chunk_nbt(&mut self, loc: ChunkLocation) -> Result<Option<Vec<u8>>> {
        let index = (loc.x.rem_euclid(32) + loc.z.rem_euclid(32) * 32) as usize;
        let (offset, count) = self.offsets[index];
        if offset < 2 || count == 0 {
            return Ok(None);
        }
        self.file.seek(SeekFrom::Start(offset as u64 * SECTOR))?;
        let mut head = [0u8; 5];
        self.file.read_exact(&mut head)?;
        let length = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let compression = head[4];
        if length == 0 {
            return Ok(None);
        }

        // Bit 128 : chunk trop gros, stocké à part dans `c.<x>.<z>.mcc`.
        let payload = if compression & 0x80 != 0 {
            let external = self
                .path
                .with_file_name(format!("c.{}.{}.mcc", loc.x, loc.z));
            std::fs::read(external)?
        } else {
            let mut buf = vec![0u8; length - 1];
            self.file.read_exact(&mut buf)?;
            buf
        };
        decompress(compression & 0x7f, &payload).map(Some)
    }
}

fn decompress(compression: u8, data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match compression {
        1 => {
            GzDecoder::new(data).read_to_end(&mut out)?;
        }
        2 => {
            ZlibDecoder::new(data).read_to_end(&mut out)?;
        }
        3 => out.extend_from_slice(data),
        4 => out = decompress_lz4_java(data)?,
        other => return Err(McError::UnsupportedCompression(other)),
    }
    Ok(out)
}

/// Format `LZ4BlockOutputStream` de lz4-java, utilisé par Minecraft 1.20.5+ (compression 4).
fn decompress_lz4_java(mut data: &[u8]) -> Result<Vec<u8>> {
    const MAGIC: &[u8] = b"LZ4Block";
    let corrupt = || McError::Corrupt("bloc LZ4 invalide".into());
    let mut out = Vec::new();
    while !data.is_empty() {
        if data.len() < 21 || &data[..8] != MAGIC {
            return Err(corrupt());
        }
        let method = data[8] & 0xf0;
        let read_i32 = |at: usize| i32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        let compressed_len = read_i32(9) as usize;
        let decompressed_len = read_i32(13) as usize;
        let body = data.get(21..21 + compressed_len).ok_or_else(corrupt)?;
        if decompressed_len == 0 {
            break;
        }
        match method {
            0x10 => out.extend_from_slice(body),
            0x20 => out.extend_from_slice(
                &lz4_flex::block::decompress(body, decompressed_len).map_err(|_| corrupt())?,
            ),
            _ => return Err(corrupt()),
        }
        data = &data[21 + compressed_len..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_region_names() {
        assert_eq!(
            Region::coords_from_name(Path::new("r.0.-2.mca")),
            Some((0, -2))
        );
        assert_eq!(
            Region::coords_from_name(Path::new("/a/b/r.-10.13.mca")),
            Some((-10, 13))
        );
        assert_eq!(Region::coords_from_name(Path::new("r.0.mca")), None);
        assert_eq!(Region::coords_from_name(Path::new("level.dat")), None);
    }
}
