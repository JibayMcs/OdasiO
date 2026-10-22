//! Couches de chunk à palette (format "combined" de la version 2 des chunkdata) :
//! en-tête int32, palette brute ou zstd, puis une trame zstd de plans de bits.

use std::collections::HashMap;

use crate::{CHUNK_CELLS, Result, VsError};

/// Niveau utilisé par le jeu.
const ZSTD_LEVEL: i32 = -3;
/// Au-delà, le jeu compresse aussi la palette.
const RAW_PALETTE_MAX: usize = 18;
const PLANE_WORDS: usize = CHUNK_CELLS / 32;

fn zstd_compress(data: &[u8]) -> Result<Vec<u8>> {
    // `bulk` inscrit la taille du contenu dans l'en-tête de trame, ce que le lecteur exige.
    Ok(zstd::bulk::compress(data, ZSTD_LEVEL)?)
}

fn zstd_decompress(data: &[u8]) -> Result<Vec<u8>> {
    let size = zstd::zstd_safe::get_frame_content_size(data)
        .ok()
        .flatten()
        .ok_or_else(|| VsError::Corrupt("trame zstd sans taille".into()))?;
    Ok(zstd::bulk::decompress(data, size as usize)?)
}

/// Nombre de plans : `ceil(log2(n))`, au moins 1.
fn bits_for(palette_len: usize) -> usize {
    ((usize::BITS - (palette_len.max(2) - 1).leading_zeros()) as usize).max(1)
}

fn le_bytes(values: impl IntoIterator<Item = u32>) -> Vec<u8> {
    values.into_iter().flat_map(u32::to_le_bytes).collect()
}

/// Encode une couche de blocs ou de fluides (32 768 cellules, 0 = vide).
pub fn encode(cells: &[u32]) -> Result<Vec<u8>> {
    debug_assert_eq!(cells.len(), CHUNK_CELLS);
    if cells.iter().all(|&v| v == 0) {
        return Ok(vec![0; 4]);
    }

    // palette[0] doit valoir 0 : le jeu s'appuie sur cette convention.
    let mut palette = vec![0u32];
    let mut lookup: HashMap<u32, u32> = HashMap::from([(0, 0)]);
    let mut indices = vec![0u32; CHUNK_CELLS];
    let mut last = (0u32, 0u32);
    for (slot, &value) in indices.iter_mut().zip(cells) {
        if value != last.0 {
            let index = *lookup.entry(value).or_insert_with(|| {
                palette.push(value);
                (palette.len() - 1) as u32
            });
            last = (value, index);
        }
        *slot = last.1;
    }

    let nbits = bits_for(palette.len());
    let mut planes = vec![0u32; nbits * PLANE_WORDS];
    for (i, &p) in indices.iter().enumerate() {
        if p == 0 {
            continue;
        }
        let (word, bit) = (i >> 5, i & 31);
        for k in 0..nbits {
            if (p >> k) & 1 == 1 {
                planes[k * PLANE_WORDS + word] |= 1 << bit;
            }
        }
    }

    let palette_bytes = le_bytes(palette.iter().copied());
    let mut out = Vec::with_capacity(64 + palette_bytes.len());
    if palette.len() > RAW_PALETTE_MAX {
        let compressed = zstd_compress(&palette_bytes)?;
        out.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        out.extend_from_slice(&compressed);
    } else {
        out.extend_from_slice(&(-(palette_bytes.len() as i32)).to_le_bytes());
        out.extend_from_slice(&palette_bytes);
    }
    out.extend_from_slice(&zstd_compress(&le_bytes(planes))?);
    Ok(out)
}

/// Décode une couche encodée par [`encode`] ou par le jeu.
pub fn decode(blob: &[u8]) -> Result<Vec<u32>> {
    let corrupt = |m: &str| VsError::Corrupt(format!("couche : {m}"));
    let header = i32::from_le_bytes(
        blob.get(..4)
            .ok_or_else(|| corrupt("en-tête absent"))?
            .try_into()
            .unwrap(),
    );
    if header == 0 {
        return Ok(vec![0; CHUNK_CELLS]);
    }
    let len = header.unsigned_abs() as usize;
    let raw = blob
        .get(4..4 + len)
        .ok_or_else(|| corrupt("palette tronquée"))?;
    let palette_bytes = if header < 0 {
        raw.to_vec()
    } else {
        zstd_decompress(raw)?
    };
    let palette: Vec<u32> = palette_bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if palette.len() <= 1 {
        return Ok(vec![0; CHUNK_CELLS]);
    }
    let nbits = bits_for(palette.len().next_power_of_two());
    let planes = zstd_decompress(&blob[4 + len..])?;
    if planes.len() < nbits * PLANE_WORDS * 4 {
        return Err(corrupt("plans de bits tronqués"));
    }
    let word = |k: usize, w: usize| {
        let at = (k * PLANE_WORDS + w) * 4;
        u32::from_le_bytes(planes[at..at + 4].try_into().unwrap())
    };
    let mut out = vec![0u32; CHUNK_CELLS];
    for (i, cell) in out.iter_mut().enumerate() {
        let mut p = 0usize;
        for k in 0..nbits {
            p |= (((word(k, i >> 5) >> (i & 31)) & 1) as usize) << k;
        }
        *cell = *palette
            .get(p)
            .ok_or_else(|| corrupt("index hors palette"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_layer_is_four_zero_bytes() {
        assert_eq!(encode(&vec![0; CHUNK_CELLS]).unwrap(), vec![0; 4]);
    }

    #[test]
    fn roundtrips_small_palette() {
        let cells: Vec<u32> = (0..CHUNK_CELLS as u32)
            .map(|i| [0, 4605, 342, 7416][(i % 4) as usize])
            .collect();
        let blob = encode(&cells).unwrap();
        assert!(i32::from_le_bytes(blob[..4].try_into().unwrap()) < 0);
        assert_eq!(decode(&blob).unwrap(), cells);
    }

    #[test]
    fn roundtrips_large_palette_with_compressed_header() {
        let cells: Vec<u32> = (0..CHUNK_CELLS as u32)
            .map(|i| 4599 + (i * 7 % 40))
            .collect();
        let blob = encode(&cells).unwrap();
        assert!(i32::from_le_bytes(blob[..4].try_into().unwrap()) > 0);
        assert_eq!(decode(&blob).unwrap(), cells);
    }

    #[test]
    fn bit_count_matches_game_rule() {
        assert_eq!(bits_for(2), 1);
        assert_eq!(bits_for(3), 2);
        assert_eq!(bits_for(4), 2);
        assert_eq!(bits_for(5), 3);
        assert_eq!(bits_for(19), 5);
    }
}
