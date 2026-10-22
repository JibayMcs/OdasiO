//! Aspect d'un bloc pour le rendu : forme simplifiée et textures des faces visibles.

use std::sync::Arc;

/// Texture RGBA carrée (la première image des textures animées).
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub size: u32,
    pub rgba: Vec<u8>,
}

impl Texture {
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let size = image.width().min(image.height());
        if size == 0 {
            return None;
        }
        // Texture animée : bande verticale d'images carrées, on garde la première.
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                rgba.extend_from_slice(&image.get_pixel(x, y).0);
            }
        }
        Some(Self { size, rgba })
    }

    pub fn solid(color: [u8; 4]) -> Self {
        Self {
            size: 1,
            rgba: color.to_vec(),
        }
    }

    /// Pixel aux coordonnées normalisées (0..1).
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        let max = self.size as f32 - 0.001;
        let x = (u * self.size as f32).clamp(0.0, max) as usize;
        let y = (v * self.size as f32).clamp(0.0, max) as usize;
        let i = (y * self.size as usize + x) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Vrai si aucun pixel n'est transparent.
    pub fn is_opaque(&self) -> bool {
        self.rgba.chunks_exact(4).all(|p| p[3] == 255)
    }
}

/// Face d'un bloc, avec une teinte multiplicative éventuelle (herbe, feuillage, eau).
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub texture: Arc<Texture>,
    pub tint: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Boîte alignée sur les axes, bornes dans 0..1.
    Cuboid { min: [f32; 3], max: [f32; 3] },
    /// Plante ou objet fin : sprite face à la caméra.
    Cross,
    /// Liquide : surface translucide, faces cachées entre liquides identiques.
    Liquid,
}

/// Aspect d'un bloc vu de dessus, du sud et de l'est.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockLook {
    pub shape: Shape,
    pub top: Face,
    pub south: Face,
    pub east: Face,
    /// Cube plein et opaque : cache les faces voisines.
    pub opaque: bool,
}

impl BlockLook {
    pub fn cuboid(min: [f32; 3], max: [f32; 3], top: Face, south: Face, east: Face) -> Self {
        let full = min == [0.0; 3] && max == [1.0; 3];
        let opaque = full
            && top.texture.is_opaque()
            && south.texture.is_opaque()
            && east.texture.is_opaque();
        Self {
            shape: Shape::Cuboid { min, max },
            top,
            south,
            east,
            opaque,
        }
    }

    /// Couleur moyenne vue du dessus (texture du dessus teintée), pour les cartes.
    pub fn map_color(&self) -> [u8; 3] {
        let t = &self.top.texture;
        let (mut sum, mut weight) = ([0u64; 3], 0u64);
        for p in t.rgba.chunks_exact(4) {
            let a = p[3] as u64;
            for k in 0..3 {
                sum[k] += p[k] as u64 * a;
            }
            weight += a;
        }
        let tint = self.top.tint.unwrap_or([255; 3]);
        let weight = weight.max(1);
        [0, 1, 2].map(|k| (sum[k] / weight * tint[k] as u64 / 255) as u8)
    }

    pub fn single(shape: Shape, face: Face) -> Self {
        Self {
            shape,
            top: face.clone(),
            south: face.clone(),
            east: face,
            opaque: false,
        }
    }
}
