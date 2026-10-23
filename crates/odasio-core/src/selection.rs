//! Zone à convertir : un ensemble de chunks Minecraft (16 × 16 blocs). Sans sélection,
//! tout le monde est converti.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub chunks: BTreeSet<(i32, i32)>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn contains(&self, cx: i32, cz: i32) -> bool {
        self.chunks.contains(&(cx, cz))
    }

    /// Chunks d'un rectangle (bornes incluses, en chunks), dans les deux sens.
    fn rect(a: (i32, i32), b: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
        (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| (x, z)))
    }

    pub fn add_rect(&mut self, a: (i32, i32), b: (i32, i32)) {
        self.chunks.extend(Self::rect(a, b));
    }

    pub fn remove_rect(&mut self, a: (i32, i32), b: (i32, i32)) {
        for c in Self::rect(a, b) {
            self.chunks.remove(&c);
        }
    }

    pub fn toggle(&mut self, c: (i32, i32)) {
        if !self.chunks.remove(&c) {
            self.chunks.insert(c);
        }
    }

    /// Zone en blocs à partir de rectangles `x1,z1,x2,z2` (coordonnées de blocs incluses).
    pub fn from_block_rects(rects: &[[i32; 4]]) -> Self {
        let mut selection = Self::default();
        for r in rects {
            selection.add_rect(
                (r[0].div_euclid(16), r[1].div_euclid(16)),
                (r[2].div_euclid(16), r[3].div_euclid(16)),
            );
        }
        selection
    }

    /// Bornes en chunks (min x, min z, max x, max z), bornes incluses.
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut it = self.chunks.iter();
        let first = *it.next()?;
        Some(it.fold((first.0, first.1, first.0, first.1), |b, &(x, z)| {
            (b.0.min(x), b.1.min(z), b.2.max(x), b.3.max(z))
        }))
    }

    /// Centre de la zone, en blocs.
    pub fn center(&self) -> Option<[i64; 2]> {
        let (x0, z0, x1, z1) = self.bounds()?;
        Some([
            (x0 as i64 + x1 as i64 + 1) * 8,
            (z0 as i64 + z1 as i64 + 1) * 8,
        ])
    }

    /// Vrai si la région (`rx`, `rz`) contient au moins un chunk sélectionné.
    pub fn touches_region(&self, rx: i32, rz: i32) -> bool {
        self.chunks
            .range((rx * 32, i32::MIN)..)
            .take_while(|c| c.0 < rx * 32 + 32)
            .any(|c| c.1.div_euclid(32) == rz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_add_remove_and_bound() {
        let mut s = Selection::default();
        s.add_rect((2, 3), (0, 1));
        assert_eq!(s.len(), 9);
        s.remove_rect((1, 2), (1, 2));
        assert_eq!(s.len(), 8);
        assert!(!s.contains(1, 2));
        assert_eq!(s.bounds(), Some((0, 1, 2, 3)));
        assert_eq!(s.center(), Some([24, 40]));
        s.toggle((1, 2));
        assert!(s.contains(1, 2));
    }

    #[test]
    fn block_rects_and_regions() {
        let s = Selection::from_block_rects(&[[-1, 0, 40, 15]]);
        assert_eq!(s.bounds(), Some((-1, 0, 2, 0)));
        assert!(s.touches_region(-1, 0));
        assert!(s.touches_region(0, 0));
        assert!(!s.touches_region(0, 1));
        assert!(!s.touches_region(1, 0));
    }
}
