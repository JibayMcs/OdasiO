//! Aspect d'un bloc pour le rendu : forme simplifiée et textures des faces visibles.

use std::sync::Arc;

/// Texture RGBA.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Texture {
    /// Image entière.
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        Self::crop(&image, 0, 0, width, height)
    }

    /// Première image d'une texture animée de Minecraft (bande verticale d'images carrées).
    pub fn from_png_first_frame(bytes: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        let height = if height > width && height % width == 0 {
            width
        } else {
            height
        };
        Self::crop(&image, 0, 0, width, height)
    }

    /// Carré de `size` pixels pris en (`x`, `y`) dans une image (texture double, etc.).
    pub fn from_png_crop(bytes: &[u8], x: u32, y: u32, size: u32) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        Self::crop(&image, x, y, size, size)
    }

    fn crop(image: &image::RgbaImage, x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || x + width > image.width() || y + height > image.height() {
            return None;
        }
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for py in y..y + height {
            for px in x..x + width {
                rgba.extend_from_slice(&image.get_pixel(px, py).0);
            }
        }
        Some(Self {
            width,
            height,
            rgba,
        })
    }

    pub fn solid(color: [u8; 4]) -> Self {
        Self {
            width: 1,
            height: 1,
            rgba: color.to_vec(),
        }
    }

    /// Pixel aux coordonnées normalisées (0..1).
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        let x = (u * self.width as f32).clamp(0.0, self.width as f32 - 0.001) as usize;
        let y = (v * self.height as f32).clamp(0.0, self.height as f32 - 0.001) as usize;
        let i = (y * self.width as usize + x) * 4;
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

/// Face d'un élément : texture, zone de texture (u0, v0, u1, v1 dans 0..1), teinte, et si
/// elle peut être cachée par un voisin opaque (face posée sur le bord du bloc).
#[derive(Debug, Clone, PartialEq)]
pub struct ElementFace {
    pub texture: Arc<Texture>,
    pub uv: [f32; 4],
    pub tint: Option<[u8; 3]>,
    pub cull: bool,
}

/// Boîte d'un modèle détaillé (bornes en blocs, 0..1 pour un bloc), faces dans l'ordre
/// dessus, dessous, sud, nord, est, ouest.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: [Option<ElementFace>; 6],
}

/// Normales des faces d'un élément, dans l'ordre de `Element::faces`.
pub const ELEMENT_NORMALS: [[i32; 3]; 6] = [
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
    [1, 0, 0],
    [-1, 0, 0],
];

impl Element {
    /// Tourne l'élément autour du centre du bloc, par pas de 90 degrés : `rx` autour de x
    /// (le haut bascule vers le nord), puis `ry` autour de y (nord vers est, vu de dessus).
    pub fn rotated(&self, rx: i32, ry: i32) -> Element {
        let turn = |v: [f32; 3]| -> [f32; 3] {
            let mut v = v;
            for _ in 0..(rx / 90).rem_euclid(4) {
                v = [v[0], v[2], -v[1]];
            }
            for _ in 0..(ry / 90).rem_euclid(4) {
                v = [-v[2], v[1], v[0]];
            }
            v
        };
        let corners = [self.from, self.to].map(|c| turn([c[0] - 0.5, c[1] - 0.5, c[2] - 0.5]));
        let from = [0, 1, 2].map(|i| corners[0][i].min(corners[1][i]) + 0.5);
        let to = [0, 1, 2].map(|i| corners[0][i].max(corners[1][i]) + 0.5);
        let mut faces: [Option<ElementFace>; 6] = Default::default();
        for (d, face) in self.faces.iter().enumerate() {
            let n = ELEMENT_NORMALS[d].map(|x| x as f32);
            let r = turn(n).map(|x| x.round() as i32);
            if let Some(target) = ELEMENT_NORMALS.iter().position(|m| *m == r) {
                faces[target] = face.clone();
            }
        }
        Element { from, to, faces }
    }

    /// Faces sur le bord du bloc : elles seules peuvent être cachées par un voisin.
    pub fn mark_cull(&mut self) {
        let edge = |d: usize| match d {
            0 => self.to[1] >= 0.999,
            1 => self.from[1] <= 0.001,
            2 => self.to[2] >= 0.999,
            3 => self.from[2] <= 0.001,
            4 => self.to[0] >= 0.999,
            _ => self.from[0] <= 0.001,
        };
        let edges: [bool; 6] = std::array::from_fn(edge);
        for (face, on_edge) in self.faces.iter_mut().zip(edges) {
            if let Some(face) = face {
                face.cull = on_edge;
            }
        }
    }
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
    /// Modèle détaillé (vide : la forme simplifiée suffit).
    pub elements: Vec<Element>,
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
            elements: Vec::new(),
        }
    }

    /// Ajoute le modèle détaillé ; seul un cube plein aux faces opaques cache ses voisins.
    pub fn with_elements(mut self, elements: Vec<Element>) -> Self {
        let single_cube = elements.len() == 1
            && elements[0].from.iter().all(|&v| v <= 0.001)
            && elements[0].to.iter().all(|&v| v >= 0.999)
            && elements[0]
                .faces
                .iter()
                .all(|f| f.as_ref().is_some_and(|f| f.texture.is_opaque()));
        self.opaque = single_cube;
        self.elements = elements;
        self
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
            elements: Vec::new(),
        }
    }
}
