//! Maillages pour le rendu 3D : faces visibles des blocs d'un chunk (textures dans un
//! atlas), relief lointain d'une région (colonnes colorées, niveaux de détail).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};

use crate::look::{BlockLook, Face, Shape, Texture};
use crate::topmap::{INCOMPLETE, REGION, RegionMap};
use crate::voxel::ChunkBlocks;

/// Sommet : position monde, coordonnées dans l'atlas (négatives = sans texture, relief
/// lointain), couleur (teinte × ombrage) et opacité.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Debug, Default, Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Quad `a b c d` (dans l'ordre du contour) avec les coordonnées de texture associées.
    fn quad(&mut self, corners: [[f32; 3]; 4], uvs: [[f32; 2]; 4], color: [u8; 4]) {
        let base = self.vertices.len() as u32;
        for (pos, uv) in corners.into_iter().zip(uvs) {
            self.vertices.push(Vertex { pos, uv, color });
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Maillage d'un chunk : blocs opaques ou découpés (test alpha), et liquides translucides.
#[derive(Debug, Default, Clone)]
pub struct ChunkMesh {
    pub opaque: MeshData,
    pub translucent: MeshData,
}

/// Atlas de textures : tuiles carrées de `TILE` pixels, la tuile 0 est blanche.
pub struct Atlas {
    pub size: u32,
    pub rgba: Vec<u8>,
    tiles: HashMap<usize, u32>,
    /// Garde les textures en vie : leur adresse sert de clé.
    keep: Vec<Arc<Texture>>,
    next: u32,
    /// Version, incrémentée à chaque ajout (pour renvoyer l'atlas au GPU).
    pub version: u64,
}

pub const TILE: u32 = 32;

impl Atlas {
    pub fn new(size: u32) -> Self {
        let mut atlas = Self {
            size,
            rgba: vec![0; (size * size * 4) as usize],
            tiles: HashMap::new(),
            keep: Vec::new(),
            next: 1,
            version: 0,
        };
        for y in 0..TILE {
            for x in 0..TILE {
                let i = ((y * size + x) * 4) as usize;
                atlas.rgba[i..i + 4].copy_from_slice(&[255; 4]);
            }
        }
        atlas
    }

    fn per_row(&self) -> u32 {
        self.size / TILE
    }

    /// Tuile de la texture, ajoutée au besoin (la tuile blanche si l'atlas est plein).
    pub fn tile(&mut self, texture: &Arc<Texture>) -> u32 {
        let key = Arc::as_ptr(texture) as usize;
        if let Some(&tile) = self.tiles.get(&key) {
            return tile;
        }
        let capacity = self.per_row() * self.per_row();
        if self.next >= capacity {
            return 0;
        }
        let tile = self.next;
        self.next += 1;
        let (tx, ty) = (
            (tile % self.per_row()) * TILE,
            (tile / self.per_row()) * TILE,
        );
        for y in 0..TILE {
            for x in 0..TILE {
                let texel = texture.sample(
                    (x as f32 + 0.5) / TILE as f32,
                    (y as f32 + 0.5) / TILE as f32,
                );
                let i = (((ty + y) * self.size + tx + x) * 4) as usize;
                self.rgba[i..i + 4].copy_from_slice(&texel);
            }
        }
        self.tiles.insert(key, tile);
        self.keep.push(texture.clone());
        self.version += 1;
        tile
    }

    /// Rectangle (u0, v0, u1, v1) d'une tuile, légèrement rentré pour éviter les débords.
    pub fn rect(&self, tile: u32) -> [f32; 4] {
        let (tx, ty) = (
            (tile % self.per_row()) * TILE,
            (tile / self.per_row()) * TILE,
        );
        let s = self.size as f32;
        [
            (tx as f32 + 0.3) / s,
            (ty as f32 + 0.3) / s,
            (tx + TILE) as f32 / s - 0.3 / s,
            (ty + TILE) as f32 / s - 0.3 / s,
        ]
    }
}

/// Coordonnées d'atlas des textures, avec un cache local pour limiter le verrouillage.
struct Uvs<'a> {
    atlas: &'a Mutex<Atlas>,
    cache: HashMap<usize, [f32; 4]>,
}

impl Uvs<'_> {
    fn rect(&mut self, texture: &Arc<Texture>) -> [f32; 4] {
        let key = Arc::as_ptr(texture) as usize;
        if let Some(r) = self.cache.get(&key) {
            return *r;
        }
        let mut atlas = self.atlas.lock().unwrap();
        let tile = atlas.tile(texture);
        let r = atlas.rect(tile);
        self.cache.insert(key, r);
        r
    }
}

fn color(face: &Face, shade: f32, alpha: u8) -> [u8; 4] {
    let tint = face.tint.unwrap_or([255; 3]);
    [
        (tint[0] as f32 * shade) as u8,
        (tint[1] as f32 * shade) as u8,
        (tint[2] as f32 * shade) as u8,
        alpha,
    ]
}

/// Sous-rectangle (fu0, fv0, fu1, fv1 dans 0..1) d'une tuile.
fn sub(r: [f32; 4], f: [f32; 4]) -> [[f32; 2]; 4] {
    let u = |t: f32| r[0] + (r[2] - r[0]) * t;
    let v = |t: f32| r[1] + (r[3] - r[1]) * t;
    [
        [u(f[0]), v(f[1])],
        [u(f[2]), v(f[1])],
        [u(f[2]), v(f[3])],
        [u(f[0]), v(f[3])],
    ]
}

/// Directions des voisins : dessus, dessous, sud, nord, est, ouest.
const DIRS: [[i32; 3]; 6] = [
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
    [1, 0, 0],
    [-1, 0, 0],
];
const SHADES: [f32; 6] = [1.0, 0.55, 0.8, 0.8, 0.66, 0.66];

/// Blocs enfouis conservés sous la surface la plus basse du chunk (falaises, berges).
const BURIED: i32 = 6;

/// Maillage d'un chunk. `looks[id]` : aspect de l'état `id` du registre. `neighbors` :
/// chunks sud, nord, est et ouest s'ils sont chargés (sinon leurs faces de contact sont
/// cachées). Le sous-sol profond, invisible depuis le ciel, n'est pas maillé.
pub fn mesh_chunk(
    chunk: &ChunkBlocks,
    neighbors: [Option<&ChunkBlocks>; 4],
    looks: &[Option<Arc<BlockLook>>],
    atlas: &Mutex<Atlas>,
) -> ChunkMesh {
    let mut uvs = Uvs {
        atlas,
        cache: HashMap::new(),
    };
    let mut mesh = ChunkMesh::default();
    let look = |id: u16| looks.get(id as usize).and_then(Option::as_deref);
    // Surface la plus basse du chunk (fond sous l'eau compris) : en dessous (moins une marge), rien n'est visible.
    let mut lowest = chunk.height as i32;
    for z in 0..16 {
        for x in 0..16 {
            let top = (0..chunk.height as i32)
                .rev()
                .find(|&y| look(chunk.get(x, y, z)).is_some_and(|l| l.shape != Shape::Liquid))
                .unwrap_or(0);
            lowest = lowest.min(top);
        }
    }
    let cut = (lowest - BURIED).max(0);
    // Bloc voisin, éventuellement dans un chunk adjacent (`None` : inconnu, face cachée).
    let at = |x: i32, y: i32, z: i32| -> Option<Option<&BlockLook>> {
        if y < cut {
            return None;
        }
        let (other, lx, lz) = match (x, z) {
            (0..=15, 16) => (neighbors[0], x, 0),
            (0..=15, -1) => (neighbors[1], x, 15),
            (16, 0..=15) => (neighbors[2], 0, z),
            (-1, 0..=15) => (neighbors[3], 15, z),
            _ => return Some(look(chunk.get(x, y, z))),
        };
        let other = other?;
        Some(look(other.get(lx, chunk.min_y + y - other.min_y, lz)))
    };
    let (ox, oz) = (chunk.cx as f32 * 16.0, chunk.cz as f32 * 16.0);
    for y in cut..chunk.height as i32 {
        for z in 0..16 {
            for x in 0..16 {
                let id = chunk.get(x, y, z);
                let Some(block) = look(id) else {
                    continue;
                };
                let hidden = |d: usize| {
                    at(x + DIRS[d][0], y + DIRS[d][1], z + DIRS[d][2])
                        .is_none_or(|n| n.is_some_and(|n| n.opaque))
                };
                let neighbor =
                    |d: usize| at(x + DIRS[d][0], y + DIRS[d][1], z + DIRS[d][2]).flatten();
                let (fx, fy, fz) = (ox + x as f32, (chunk.min_y + y) as f32, oz + z as f32);
                if !block.elements.is_empty() && !matches!(block.shape, Shape::Liquid) {
                    for element in &block.elements {
                        let (min, max) = (element.from, element.to);
                        for (d, face) in element.faces.iter().enumerate() {
                            let Some(face) = face else {
                                continue;
                            };
                            if face.cull && hidden(d) {
                                continue;
                            }
                            let r = uvs.rect(&face.texture);
                            let (x0, y0, z0) = (fx + min[0], fy + min[1], fz + min[2]);
                            let (x1, y1, z1) = (fx + max[0], fy + max[1], fz + max[2]);
                            let corners = match d {
                                0 => [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
                                1 => [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]],
                                2 => [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1], [x0, y0, z1]],
                                3 => [[x1, y1, z0], [x0, y1, z0], [x0, y0, z0], [x1, y0, z0]],
                                4 => [[x1, y1, z1], [x1, y1, z0], [x1, y0, z0], [x1, y0, z1]],
                                _ => [[x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [x0, y0, z0]],
                            };
                            let tint = face.tint.unwrap_or([255; 3]);
                            let shade = SHADES[d];
                            let color = [
                                (tint[0] as f32 * shade) as u8,
                                (tint[1] as f32 * shade) as u8,
                                (tint[2] as f32 * shade) as u8,
                                255,
                            ];
                            let uv = face.uv.map(|v| v.clamp(0.0, 1.0));
                            mesh.opaque.quad(corners, sub(r, uv), color);
                        }
                    }
                    continue;
                }
                match &block.shape {
                    Shape::Cuboid { min, max } => {
                        let full = *min == [0.0; 3] && *max == [1.0; 3];
                        for (d, &shade) in SHADES.iter().enumerate() {
                            if full && hidden(d) {
                                continue;
                            }
                            let face = match d {
                                0 | 1 => &block.top,
                                2 | 3 => &block.south,
                                _ => &block.east,
                            };
                            let r = uvs.rect(&face.texture);
                            let (x0, y0, z0) = (fx + min[0], fy + min[1], fz + min[2]);
                            let (x1, y1, z1) = (fx + max[0], fy + max[1], fz + max[2]);
                            let (corners, f) = match d {
                                0 => (
                                    [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
                                    [min[0], min[2], max[0], max[2]],
                                ),
                                1 => (
                                    [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]],
                                    [min[0], 1.0 - max[2], max[0], 1.0 - min[2]],
                                ),
                                2 => (
                                    [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1], [x0, y0, z1]],
                                    [min[0], 1.0 - max[1], max[0], 1.0 - min[1]],
                                ),
                                3 => (
                                    [[x1, y1, z0], [x0, y1, z0], [x0, y0, z0], [x1, y0, z0]],
                                    [1.0 - max[0], 1.0 - max[1], 1.0 - min[0], 1.0 - min[1]],
                                ),
                                4 => (
                                    [[x1, y1, z1], [x1, y1, z0], [x1, y0, z0], [x1, y0, z1]],
                                    [1.0 - max[2], 1.0 - max[1], 1.0 - min[2], 1.0 - min[1]],
                                ),
                                _ => (
                                    [[x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [x0, y0, z0]],
                                    [min[2], 1.0 - max[1], max[2], 1.0 - min[1]],
                                ),
                            };
                            mesh.opaque
                                .quad(corners, sub(r, f), color(face, shade, 255));
                        }
                    }
                    Shape::Cross => {
                        let r = uvs.rect(&block.top.texture);
                        let c = color(&block.top, 0.9, 255);
                        let (a, b) = (0.15, 0.85);
                        for (p, q) in [
                            ([fx + a, fz + a], [fx + b, fz + b]),
                            ([fx + a, fz + b], [fx + b, fz + a]),
                        ] {
                            mesh.opaque.quad(
                                [
                                    [p[0], fy + 1.0, p[1]],
                                    [q[0], fy + 1.0, q[1]],
                                    [q[0], fy, q[1]],
                                    [p[0], fy, p[1]],
                                ],
                                sub(r, [0.0, 0.0, 1.0, 1.0]),
                                c,
                            );
                        }
                    }
                    Shape::Liquid => {
                        let same = |n: Option<&BlockLook>| {
                            n.is_some_and(|n| {
                                matches!(n.shape, Shape::Liquid)
                                    && Arc::ptr_eq(&n.top.texture, &block.top.texture)
                            })
                        };
                        let r = uvs.rect(&block.top.texture);
                        let top_open = !same(neighbor(0));
                        let h = if top_open { 0.875 } else { 1.0 };
                        let (x0, x1, z0, z1, y0, y1) = (fx, fx + 1.0, fz, fz + 1.0, fy, fy + h);
                        let full = sub(r, [0.0, 0.0, 1.0, 1.0]);
                        if top_open && !hidden(0) {
                            mesh.translucent.quad(
                                [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
                                full,
                                color(&block.top, 1.0, 200),
                            );
                        }
                        for (d, &shade) in SHADES.iter().enumerate().skip(2) {
                            if same(neighbor(d)) || hidden(d) {
                                continue;
                            }
                            let corners = match d {
                                2 => [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1], [x0, y0, z1]],
                                3 => [[x1, y1, z0], [x0, y1, z0], [x0, y0, z0], [x1, y0, z0]],
                                4 => [[x1, y1, z1], [x1, y1, z0], [x1, y0, z0], [x1, y0, z1]],
                                _ => [[x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [x0, y0, z0]],
                            };
                            mesh.translucent
                                .quad(corners, full, color(&block.top, shade, 200));
                        }
                    }
                }
            }
        }
    }
    mesh
}

/// Couleur de carte d'une colonne et ombrage d'une face.
fn shaded(c: [u8; 3], shade: f32) -> [u8; 4] {
    [
        (c[0] as f32 * shade) as u8,
        (c[1] as f32 * shade) as u8,
        (c[2] as f32 * shade) as u8,
        255,
    ]
}

/// Relief lointain d'une région : une colonne colorée par cellule de `step` × `step`
/// blocs, avec des parois entre cellules de hauteurs différentes. `colors[id]` : couleur de
/// carte de l'état. `skip_incomplete` : colonnes des chunks incomplets ignorées.
pub fn mesh_far(
    region: &RegionMap,
    rx: i32,
    rz: i32,
    step: usize,
    colors: &[[u8; 3]],
    skip_incomplete: bool,
) -> MeshData {
    let mut mesh = MeshData::default();
    let cells = REGION / step;
    let sample = |cx: usize, cz: usize| -> Option<(f32, [u8; 3])> {
        if cx >= cells || cz >= cells {
            return None;
        }
        let i = (cz * step + step / 2) * REGION + cx * step + step / 2;
        let top = region.top[i];
        let id = top & !INCOMPLETE;
        if id == 0 || (skip_incomplete && top & INCOMPLETE != 0) {
            return None;
        }
        let c = colors.get(id as usize).copied().unwrap_or([255, 0, 255]);
        Some((region.height[i] as f32 + 1.0, c))
    };
    let none = [-1.0, -1.0];
    let (ox, oz) = (rx as f32 * REGION as f32, rz as f32 * REGION as f32);
    let s = step as f32;
    for cz in 0..cells {
        for cx in 0..cells {
            let Some((h, c)) = sample(cx, cz) else {
                continue;
            };
            let (x0, z0) = (ox + cx as f32 * s, oz + cz as f32 * s);
            let (x1, z1) = (x0 + s, z0 + s);
            mesh.quad(
                [[x0, h, z0], [x1, h, z0], [x1, h, z1], [x0, h, z1]],
                [none; 4],
                shaded(c, 1.0),
            );
            // Parois vers les voisins plus bas (ou absents : jupe de 16 blocs).
            for (d, (nx, nz)) in [(0, (cx + 1, cz)), (1, (cx, cz + 1))] {
                let lower = match sample(nx, nz) {
                    Some((nh, _)) if nh < h => nh,
                    Some(_) => continue,
                    None => h - 16.0,
                };
                let corners = if d == 0 {
                    [[x1, h, z0], [x1, h, z1], [x1, lower, z1], [x1, lower, z0]]
                } else {
                    [[x0, h, z1], [x1, h, z1], [x1, lower, z1], [x0, lower, z1]]
                };
                mesh.quad(
                    corners,
                    [none; 4],
                    shaded(c, if d == 0 { 0.66 } else { 0.8 }),
                );
            }
            // Côtés ouest et nord : seulement face à un voisin plus bas ou absent.
            for (d, n) in [
                (0, cx.checked_sub(1).map(|x| (x, cz))),
                (1, cz.checked_sub(1).map(|z| (cx, z))),
            ] {
                let lower = match n.and_then(|(nx, nz)| sample(nx, nz)) {
                    Some((nh, _)) if nh < h => nh,
                    Some(_) => continue,
                    None => h - 16.0,
                };
                let corners = if d == 0 {
                    [[x0, h, z1], [x0, h, z0], [x0, lower, z0], [x0, lower, z1]]
                } else {
                    [[x1, h, z0], [x0, h, z0], [x0, lower, z0], [x1, lower, z0]]
                };
                mesh.quad(
                    corners,
                    [none; 4],
                    shaded(c, if d == 0 { 0.66 } else { 0.8 }),
                );
            }
        }
    }
    mesh
}

/// Boîte translucide (surlignage d'un bloc).
pub fn mesh_box(min: [f32; 3], max: [f32; 3], color: [u8; 4]) -> MeshData {
    let mut mesh = MeshData::default();
    let ([x0, y0, z0], [x1, y1, z1]) = (min, max);
    let none = [[-1.0, -1.0]; 4];
    for corners in [
        [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
        [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]],
        [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1], [x0, y0, z1]],
        [[x1, y1, z0], [x0, y1, z0], [x0, y0, z0], [x1, y0, z0]],
        [[x1, y1, z1], [x1, y1, z0], [x1, y0, z0], [x1, y0, z1]],
        [[x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [x0, y0, z0]],
    ] {
        mesh.quad(corners, none, color);
    }
    mesh
}
