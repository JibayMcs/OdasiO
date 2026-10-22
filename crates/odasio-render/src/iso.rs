//! Rendu isométrique logiciel d'une zone de blocs.
//!
//! Projection classique : un cube unité fait `2a` pixels de large, son dessus `a` de haut et
//! ses arêtes verticales `a`. Les blocs sont dessinés d'arrière en avant (x + y + z croissant,
//! la caméra regardant depuis le sud-est, au-dessus) ; seules les faces exposées sont tracées.

use crate::look::{BlockLook, Face, Shape};

/// Zone à dessiner : une grille de blocs, chaque cellule désignant un aspect (0 = air).
#[derive(Debug, Clone)]
pub struct Scene {
    pub size_x: usize,
    pub size_y: usize,
    pub size_z: usize,
    /// Index dans la table des aspects, ordre `(y * size_z + z) * size_x + x`, 0 = air.
    pub cells: Vec<u32>,
}

impl Scene {
    pub fn new(size_x: usize, size_y: usize, size_z: usize) -> Self {
        Self {
            size_x,
            size_y,
            size_z,
            cells: vec![0; size_x * size_y * size_z],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        self.cells[(y * self.size_z + z) * self.size_x + x]
    }

    /// Coordonnées (x, y, z) d'un index de cellule.
    pub fn position(&self, cell: usize) -> (usize, usize, usize) {
        let x = cell % self.size_x;
        let z = (cell / self.size_x) % self.size_z;
        (x, cell / (self.size_x * self.size_z), z)
    }

    /// Position dans la vue tournée de `quarter` quarts de tour d'une colonne (`x`, `z`) de
    /// cette scène (comme [`Scene::rotated`]).
    pub fn rotate_xz(&self, quarter: u8, x: usize, z: usize) -> (usize, usize) {
        match quarter % 4 {
            0 => (x, z),
            1 => (self.size_z - 1 - z, x),
            2 => (self.size_x - 1 - x, self.size_z - 1 - z),
            _ => (z, self.size_x - 1 - x),
        }
    }

    /// Position dans la scène d'origine d'une colonne (`x`, `z`) de la vue tournée de
    /// `quarter` quarts de tour (inverse de [`Scene::rotated`]).
    pub fn unrotate(&self, quarter: u8, x: usize, z: usize) -> (usize, usize) {
        match quarter % 4 {
            0 => (x, z),
            1 => (z, self.size_z - 1 - x),
            2 => (self.size_x - 1 - x, self.size_z - 1 - z),
            _ => (self.size_x - 1 - z, x),
        }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, look: u32) {
        self.cells[(y * self.size_z + z) * self.size_x + x] = look;
    }

    /// Vue tournée de `quarter` quarts de tour : la caméra fait le tour de la zone.
    pub fn rotated(&self, quarter: u8) -> Scene {
        let quarter = quarter % 4;
        if quarter == 0 {
            return self.clone();
        }
        let (nx, nz) = if quarter % 2 == 1 {
            (self.size_z, self.size_x)
        } else {
            (self.size_x, self.size_z)
        };
        let mut out = Scene::new(nx, self.size_y, nz);
        for y in 0..self.size_y {
            for z in 0..self.size_z {
                for x in 0..self.size_x {
                    let (tx, tz) = match quarter {
                        1 => (self.size_z - 1 - z, x),
                        2 => (self.size_x - 1 - x, self.size_z - 1 - z),
                        _ => (z, self.size_x - 1 - x),
                    };
                    out.set(tx, y, tz, self.get(x, y, z));
                }
            }
        }
        out
    }
}

/// Image RGBA produite.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Pour chaque pixel, la cellule de la scène dessinée au premier plan (index + 1, 0 = rien).
    pub pick: Vec<u32>,
}

impl Image {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; width * height * 4],
            pick: vec![0; width * height],
        }
    }

    /// Cellule de la scène visible au pixel (`x`, `y`).
    pub fn picked(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pick[y * self.width + x]
            .checked_sub(1)
            .map(|c| c as usize)
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height || color[3] == 0
        {
            return;
        }
        let i = (y as usize * self.width + x as usize) * 4;
        let a = color[3] as u32;
        for (dst, &src) in self.rgba[i..i + 3].iter_mut().zip(&color[..3]) {
            *dst = ((src as u32 * a + *dst as u32 * (255 - a)) / 255) as u8;
        }
        self.rgba[i + 3] =
            (self.rgba[i + 3] as u32 + a * (255 - self.rgba[i + 3] as u32) / 255) as u8;
    }
}

struct Projection {
    a: f32,
    origin_x: f32,
    origin_y: f32,
}

impl Projection {
    #[inline]
    fn point(&self, x: f32, y: f32, z: f32) -> [f32; 2] {
        [
            self.origin_x + (x - z) * self.a,
            self.origin_y + (x + z) * self.a / 2.0 - y * self.a,
        ]
    }
}

/// Habillage d'un quad : face, zone de texture (u0, v0, u1, v1), ombrage et opacité.
struct Skin<'a> {
    face: &'a Face,
    uv: [f32; 4],
    shade: f32,
    alpha: f32,
    /// Cellule de la scène (index + 1) inscrite dans le tampon de sélection.
    cell: u32,
}

/// Dessine un parallélogramme texturé : `o` + u·`du` + v·`dv`, avec (u, v) dans [0, 1]²
/// associés à la zone de texture de `skin`.
fn draw_quad(image: &mut Image, o: [f32; 2], du: [f32; 2], dv: [f32; 2], skin: &Skin) {
    let Skin {
        face,
        uv,
        shade,
        alpha,
        cell,
    } = *skin;
    let det = du[0] * dv[1] - du[1] * dv[0];
    if det.abs() < 1e-6 {
        return;
    }
    let xs = [o[0], o[0] + du[0], o[0] + dv[0], o[0] + du[0] + dv[0]];
    let ys = [o[1], o[1] + du[1], o[1] + dv[1], o[1] + du[1] + dv[1]];
    let x0 = xs.iter().cloned().fold(f32::MAX, f32::min).floor() as i32;
    let x1 = xs.iter().cloned().fold(f32::MIN, f32::max).ceil() as i32;
    let y0 = ys.iter().cloned().fold(f32::MAX, f32::min).floor() as i32;
    let y1 = ys.iter().cloned().fold(f32::MIN, f32::max).ceil() as i32;
    let tint = face.tint.unwrap_or([255, 255, 255]);
    for py in y0.max(0)..y1.min(image.height as i32) {
        for px in x0.max(0)..x1.min(image.width as i32) {
            let (rx, ry) = (px as f32 + 0.5 - o[0], py as f32 + 0.5 - o[1]);
            let u = (rx * dv[1] - ry * dv[0]) / det;
            let v = (du[0] * ry - du[1] * rx) / det;
            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                continue;
            }
            let texel = face
                .texture
                .sample(uv[0] + u * (uv[2] - uv[0]), uv[1] + v * (uv[3] - uv[1]));
            if texel[3] < 16 {
                continue;
            }
            let color = [
                (texel[0] as f32 * tint[0] as f32 / 255.0 * shade) as u8,
                (texel[1] as f32 * tint[1] as f32 / 255.0 * shade) as u8,
                (texel[2] as f32 * tint[2] as f32 / 255.0 * shade) as u8,
                (texel[3] as f32 * alpha) as u8,
            ];
            image.blend(px, py, color);
            if texel[3] >= 128 {
                image.pick[py as usize * image.width + px as usize] = cell;
            }
        }
    }
}

/// Ombrage des faces : dessus pleine lumière, faces latérales assombries.
const SHADE_TOP: f32 = 1.0;
const SHADE_SOUTH: f32 = 0.8;
const SHADE_EAST: f32 = 0.64;

/// Position dans l'image rendue d'un point (coordonnées continues) de la scène.
pub fn project(scene: &Scene, a: f32, x: f32, y: f32, z: f32) -> [f32; 2] {
    Projection {
        a,
        origin_x: scene.size_z as f32 * a + 1.0,
        origin_y: scene.size_y as f32 * a + 1.0,
    }
    .point(x, y, z)
}

/// Contour à l'écran (hexagone, en pixels de l'image rendue) du bloc (`x`, `y`, `z`) de la
/// scène, pour surligner une sélection.
pub fn block_outline(scene: &Scene, a: f32, x: usize, y: usize, z: usize) -> [[f32; 2]; 6] {
    let projection = Projection {
        a,
        origin_x: scene.size_z as f32 * a + 1.0,
        origin_y: scene.size_y as f32 * a + 1.0,
    };
    let (x0, y0, z0) = (x as f32, y as f32, z as f32);
    let (x1, y1, z1) = (x0 + 1.0, y0 + 1.0, z0 + 1.0);
    [
        projection.point(x0, y1, z0),
        projection.point(x1, y1, z0),
        projection.point(x1, y0, z0),
        projection.point(x1, y0, z1),
        projection.point(x0, y0, z1),
        projection.point(x0, y1, z1),
    ]
}

/// Rend la scène. `looks[i]` est l'aspect de l'index `i` (l'index 0, l'air, est ignoré).
/// `a` : demi-largeur d'un bloc à l'écran, en pixels.
pub fn render(scene: &Scene, looks: &[Option<BlockLook>], a: f32) -> Image {
    let (sx, sy, sz) = (
        scene.size_x as f32,
        scene.size_y as f32,
        scene.size_z as f32,
    );
    let width = ((sx + sz) * a).ceil() as usize + 2;
    let height = ((sx + sz) * a / 2.0 + sy * a).ceil() as usize + 2;
    let projection = Projection {
        a,
        origin_x: sz * a + 1.0,
        origin_y: sy * a + 1.0,
    };
    let mut image = Image::new(width, height);

    let look =
        |index: u32| -> Option<&BlockLook> { looks.get(index as usize).and_then(Option::as_ref) };
    let hides = |x: usize, y: usize, z: usize, own: u32| -> bool {
        let other = scene.get(x, y, z);
        match look(other) {
            Some(l) if l.opaque => true,
            // Les surfaces internes d'un même liquide ne sont pas dessinées.
            Some(l) if matches!(l.shape, Shape::Liquid) => other == own,
            _ => false,
        }
    };

    // Parcours par diagonales x + y + z croissantes : de l'arrière vers l'avant.
    let max_d = scene.size_x + scene.size_y + scene.size_z;
    for d in 0..max_d {
        for y in 0..scene.size_y.min(d + 1) {
            for x in 0..scene.size_x.min(d - y + 1) {
                let z = d - y - x;
                if z >= scene.size_z {
                    continue;
                }
                let index = scene.get(x, y, z);
                let Some(block) = look(index) else {
                    continue;
                };
                let top_open = y + 1 >= scene.size_y || !hides(x, y + 1, z, index);
                let south_open = z + 1 >= scene.size_z || !hides(x, y, z + 1, index);
                let east_open = x + 1 >= scene.size_x || !hides(x + 1, y, z, index);
                if !(top_open || south_open || east_open) {
                    continue;
                }
                let (fx, fy, fz) = (x as f32, y as f32, z as f32);
                let cell = ((y * scene.size_z + z) * scene.size_x + x) as u32 + 1;
                match &block.shape {
                    Shape::Cuboid { min, max } => {
                        let (x0, x1) = (fx + min[0], fx + max[0]);
                        let (y0, y1) = (fy + min[1], fy + max[1]);
                        let (z0, z1) = (fz + min[2], fz + max[2]);
                        let p = |x, y, z| projection.point(x, y, z);
                        let sub = |a: [f32; 2], b: [f32; 2]| [b[0] - a[0], b[1] - a[1]];
                        // Les faces d'un bloc partiel restent visibles même contre un voisin.
                        let partial = *min != [0.0; 3] || *max != [1.0; 3];
                        if south_open || partial {
                            let o = p(x0, y1, z1);
                            draw_quad(
                                &mut image,
                                o,
                                sub(o, p(x1, y1, z1)),
                                sub(o, p(x0, y0, z1)),
                                &Skin {
                                    face: &block.south,
                                    uv: [min[0], 1.0 - max[1], max[0], 1.0 - min[1]],
                                    shade: SHADE_SOUTH,
                                    alpha: 1.0,
                                    cell,
                                },
                            );
                        }
                        if east_open || partial {
                            let o = p(x1, y1, z1);
                            draw_quad(
                                &mut image,
                                o,
                                sub(o, p(x1, y1, z0)),
                                sub(o, p(x1, y0, z1)),
                                &Skin {
                                    face: &block.east,
                                    uv: [1.0 - max[2], 1.0 - max[1], 1.0 - min[2], 1.0 - min[1]],
                                    shade: SHADE_EAST,
                                    alpha: 1.0,
                                    cell,
                                },
                            );
                        }
                        if top_open || partial {
                            let o = p(x0, y1, z0);
                            draw_quad(
                                &mut image,
                                o,
                                sub(o, p(x1, y1, z0)),
                                sub(o, p(x0, y1, z1)),
                                &Skin {
                                    face: &block.top,
                                    uv: [min[0], min[2], max[0], max[2]],
                                    shade: SHADE_TOP,
                                    alpha: 1.0,
                                    cell,
                                },
                            );
                        }
                    }
                    Shape::Cross => {
                        // Sprite face à la caméra, centré dans le bloc.
                        let o = projection.point(fx + 0.15, fy + 1.0, fz + 0.85);
                        let right = projection.point(fx + 0.85, fy + 1.0, fz + 0.15);
                        draw_quad(
                            &mut image,
                            o,
                            [right[0] - o[0], right[1] - o[1]],
                            [0.0, a],
                            &Skin {
                                face: &block.top,
                                uv: [0.0, 0.0, 1.0, 1.0],
                                shade: SHADE_TOP,
                                alpha: 1.0,
                                cell,
                            },
                        );
                    }
                    Shape::Liquid => {
                        if top_open {
                            let level = fy + 0.875;
                            let o = projection.point(fx, level, fz);
                            let ox = projection.point(fx + 1.0, level, fz);
                            let oz = projection.point(fx, level, fz + 1.0);
                            draw_quad(
                                &mut image,
                                o,
                                [ox[0] - o[0], ox[1] - o[1]],
                                [oz[0] - o[0], oz[1] - o[1]],
                                &Skin {
                                    face: &block.top,
                                    uv: [0.0, 0.0, 1.0, 1.0],
                                    shade: SHADE_TOP,
                                    alpha: 0.8,
                                    cell,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::Texture;
    use std::sync::Arc;

    fn face(color: [u8; 4]) -> Face {
        Face {
            texture: Arc::new(Texture::solid(color)),
            tint: None,
        }
    }

    #[test]
    fn a_single_cube_shows_three_shaded_faces() {
        let looks = vec![
            None,
            Some(BlockLook::cuboid(
                [0.0; 3],
                [1.0; 3],
                face([200, 200, 200, 255]),
                face([200, 200, 200, 255]),
                face([200, 200, 200, 255]),
            )),
        ];
        let mut scene = Scene::new(1, 1, 1);
        scene.set(0, 0, 0, 1);
        let image = render(&scene, &looks, 16.0);
        let pixel = |x: usize, y: usize| {
            let i = (y * image.width + x) * 4;
            image.rgba[i..i + 4].to_vec()
        };
        // Centre du dessus, de la face sud (gauche) et de la face est (droite).
        assert_eq!(pixel(17, 9), vec![200, 200, 200, 255]);
        assert_eq!(pixel(9, 25), vec![160, 160, 160, 255]);
        assert_eq!(pixel(25, 25), vec![128, 128, 128, 255]);
    }

    #[test]
    fn rotation_keeps_every_block() {
        let mut scene = Scene::new(3, 2, 5);
        scene.set(0, 1, 4, 7);
        for quarter in 0..4 {
            let rotated = scene.rotated(quarter);
            let cell = rotated.cells.iter().position(|&c| c == 7).unwrap();
            let (x, y, z) = rotated.position(cell);
            assert_eq!(y, 1);
            assert_eq!(scene.unrotate(quarter, x, z), (0, 4), "quart {quarter}");
            assert_eq!(scene.rotate_xz(quarter, 0, 4), (x, z), "quart {quarter}");
        }
        assert_eq!(scene.rotated(1).size_x, 5);
    }
}
