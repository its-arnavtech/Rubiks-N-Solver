//! Cube states and the fast move path (ARCHITECTURE §4).
//!
//! A move touches only its layer: four side strips of N stickers, plus a face rotation for
//! layer 0 (and layer N−1). Strip index formulas are derived once per N from the 3D geometry
//! (`geometry.rs`) and are affine in `(layer, k)`, so a move never builds a permutation.

use std::fmt;

use crate::geometry::{self, negative_face, positive_face, sticker_count};
use crate::moves::{Axis, Move};

/// Stickers of one side face in one layer: `index(ℓ, k) = base + ℓ·lstride + k·kstride`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Strip {
    base: i64,
    lstride: i64,
    kstride: i64,
}

impl Strip {
    #[inline(always)]
    fn at(self, layer: i64, k: i64) -> usize {
        (self.base + layer * self.lstride + k * self.kstride) as usize
    }
}

/// Per-N move tables. For each axis, a clockwise quarter turn sends the sticker at
/// `strips[axis][i].at(ℓ, k)` to `strips[axis][(i + 1) % 4].at(ℓ, k)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    n: u32,
    strips: [[Strip; 4]; 3],
}

impl Layout {
    pub fn new(n: u32) -> Self {
        assert!(n >= 2, "cube size must be at least 2, got {n}");
        let strips = Axis::ALL.map(|axis| Self::axis_strips(n, axis));
        Self { n, strips }
    }

    fn axis_strips(n: u32, axis: Axis) -> [Strip; 4] {
        let a = axis as usize;
        let ni = i64::from(n);
        let m = ni - 1;
        // First side face (lowest face number whose normal is not along `axis`).
        let f0 = (0..6)
            .find(|&f| geometry::sticker_position(n, f, 0, 0)[a].abs() != ni)
            .expect("a side face exists");
        let p0 = geometry::sticker_position(n, f0, 0, 0);
        let d_row = geometry::sticker_position(n, f0, 1, 0)[a] - p0[a];
        let fbase = geometry::sticker_index(n, f0, 0, 0) as i64;
        // layer = (m − pos[a]) / 2 and pos[a] = p0[a] + row·d_row + col·d_col, where
        // p0[a] = ±m and exactly one of d_row, d_col is ±2.
        let first = if d_row != 0 {
            // Layer varies with the row, so the strip is a row: k = col.
            if d_row < 0 {
                Strip {
                    base: fbase,
                    lstride: ni,
                    kstride: 1,
                }
            } else {
                Strip {
                    base: fbase + m * ni,
                    lstride: -ni,
                    kstride: 1,
                }
            }
        } else {
            let d_col = geometry::sticker_position(n, f0, 0, 1)[a] - p0[a];
            if d_col < 0 {
                Strip {
                    base: fbase,
                    lstride: 1,
                    kstride: ni,
                }
            } else {
                Strip {
                    base: fbase + m,
                    lstride: -1,
                    kstride: ni,
                }
            }
        };
        // Each next strip is the image of the previous one under a clockwise quarter turn.
        let image = |s: Strip, l: i64, k: i64| {
            let p = geometry::position_of(n, s.at(l, k));
            geometry::sticker_at(n, geometry::rotate_cw(axis, p)).expect("lands on a sticker")
                as i64
        };
        let mut strips = [first; 4];
        for i in 1..4 {
            let prev = strips[i - 1];
            let base = image(prev, 0, 0);
            strips[i] = Strip {
                base,
                lstride: image(prev, 1, 0) - base,
                kstride: image(prev, 0, 1) - base,
            };
        }
        strips
    }

    pub fn n(&self) -> u32 {
        self.n
    }

    /// Apply `mv` to a facelet array in place. Works for any layer `0..N`, including the
    /// never-allowed layer N−1 (used only by tests against the oracle).
    pub fn apply<T: Copy>(&self, facelets: &mut [T], mv: Move) {
        let n = self.n;
        assert!(mv.layer < n, "layer {} out of range for N={n}", mv.layer);
        debug_assert_eq!(facelets.len(), sticker_count(n));
        let t = usize::from(mv.turns % 4);
        if t == 0 {
            return;
        }
        let s = self.strips[mv.axis as usize];
        let l = i64::from(mv.layer);
        for k in 0..i64::from(n) {
            let idx = [s[0].at(l, k), s[1].at(l, k), s[2].at(l, k), s[3].at(l, k)];
            let v = idx.map(|i| facelets[i]);
            for i in 0..4 {
                facelets[idx[(i + t) % 4]] = v[i];
            }
        }
        if mv.layer == 0 {
            rotate_face_cw(facelets, n, positive_face(mv.axis), t);
        }
        if mv.layer == n - 1 {
            // Clockwise from the positive side is counter-clockwise seen from outside this face.
            rotate_face_cw(facelets, n, negative_face(mv.axis), 4 - t);
        }
    }
}

/// Rotate one face `t` clockwise quarter turns (seen from outside): the sticker at `(r, c)`
/// moves to `(c, N−1−r)`, per CONVENTIONS §4.
fn rotate_face_cw<T: Copy>(facelets: &mut [T], n: u32, face: usize, t: usize) {
    let nn = n as usize;
    let base = face * nn * nn;
    let m = nn - 1;
    let at = |r: usize, c: usize| base + r * nn + c;
    for r in 0..nn / 2 {
        for c in 0..nn.div_ceil(2) {
            let idx = [at(r, c), at(c, m - r), at(m - r, m - c), at(m - c, r)];
            let v = idx.map(|i| facelets[i]);
            for i in 0..4 {
                facelets[idx[(i + t) % 4]] = v[i];
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimError {
    UnsupportedSize(u32),
    WrongFaceletCount { expected: usize, got: usize },
    BadColor { index: usize, value: u8 },
    MoveNotAllowed { n: u32, mv: Move },
    BadMoveWord(u32),
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSize(n) => write!(f, "unsupported cube size {n} (need N >= 2)"),
            Self::WrongFaceletCount { expected, got } => {
                write!(f, "expected {expected} facelets, got {got}")
            }
            Self::BadColor { index, value } => {
                write!(f, "facelet {index} has invalid color {value}")
            }
            Self::MoveNotAllowed { n, mv } => write!(f, "move {mv} is not allowed on N={n}"),
            Self::BadMoveWord(w) => write!(f, "invalid move word {w:#x}"),
        }
    }
}

impl std::error::Error for SimError {}

/// Facelet state of an NxN cube. `T = u8` holds colors ([`Cube`]); `T = u32` holds each
/// sticker's home index ([`LabeledCube`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CubeState<T> {
    layout: Layout,
    facelets: Vec<T>,
}

/// Colors mode: what the solver may see.
pub type Cube = CubeState<u8>;
/// Labeled mode: home sticker index per position. Verification and tests only (ADR-012).
pub type LabeledCube = CubeState<u32>;

impl<T: Copy> CubeState<T> {
    pub fn n(&self) -> u32 {
        self.layout.n
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn facelets(&self) -> &[T] {
        &self.facelets
    }

    pub fn into_facelets(self) -> Vec<T> {
        self.facelets
    }

    /// Apply one move. Panics if the layer is `>= N`; allowed-move policy is the caller's.
    pub fn apply(&mut self, mv: Move) {
        self.layout.apply(&mut self.facelets, mv);
    }

    pub fn apply_all(&mut self, moves: &[Move]) {
        for &m in moves {
            self.apply(m);
        }
    }
}

fn check_size(n: u32) -> Result<(), SimError> {
    if n < 2 {
        return Err(SimError::UnsupportedSize(n));
    }
    Ok(())
}

impl Cube {
    pub fn solved(n: u32) -> Self {
        let per_face = (n as usize) * (n as usize);
        let facelets = (0..6u8)
            .flat_map(|f| std::iter::repeat_n(f, per_face))
            .collect();
        Self {
            layout: Layout::new(n),
            facelets,
        }
    }

    /// Wrap a color array, checking only its shape (length and colors in 0..6).
    pub fn from_facelets(n: u32, facelets: Vec<u8>) -> Result<Self, SimError> {
        check_size(n)?;
        let expected = sticker_count(n);
        if facelets.len() != expected {
            return Err(SimError::WrongFaceletCount {
                expected,
                got: facelets.len(),
            });
        }
        if let Some((index, &value)) = facelets.iter().enumerate().find(|&(_, &c)| c > 5) {
            return Err(SimError::BadColor { index, value });
        }
        Ok(Self {
            layout: Layout::new(n),
            facelets,
        })
    }

    /// Every sticker shows its own face's color.
    pub fn is_solved(&self) -> bool {
        let per_face = (self.n() as usize).pow(2);
        self.facelets
            .chunks_exact(per_face)
            .zip(0u8..)
            .all(|(face, f)| face.iter().all(|&c| c == f))
    }
}

impl LabeledCube {
    pub fn solved(n: u32) -> Self {
        let facelets = (0..sticker_count(n) as u32).collect();
        Self {
            layout: Layout::new(n),
            facelets,
        }
    }

    pub fn is_identity(&self) -> bool {
        self.facelets
            .iter()
            .enumerate()
            .all(|(i, &h)| h as usize == i)
    }

    /// The colors this labeling shows.
    pub fn to_colors(&self) -> Cube {
        let per_face = (self.n() as usize).pow(2);
        let facelets = self
            .facelets
            .iter()
            .map(|&h| (h as usize / per_face) as u8)
            .collect();
        Cube {
            layout: self.layout,
            facelets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::move_permutation;

    #[test]
    fn strips_close_after_four_turns() {
        for n in 2..=9 {
            let layout = Layout::new(n);
            for s in layout.strips {
                assert_eq!(
                    s.iter()
                        .map(|s| s.base)
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                    4
                );
            }
        }
    }

    #[test]
    fn fast_path_matches_geometry_for_every_layer() {
        for n in 2..=12 {
            for axis in Axis::ALL {
                for layer in 0..n {
                    for turns in 1..=3 {
                        let mv = Move::new(axis, layer, turns);
                        let mut c = LabeledCube::solved(n);
                        c.apply(mv);
                        assert_eq!(c.facelets(), move_permutation(n, mv), "n={n} {mv:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn hand_checked_sticker_movements() {
        let face = |c: &Cube, f: usize| c.facelets()[f * 9..(f + 1) * 9].to_vec();
        // U: R's top row moves to F's top row.
        let mut c = Cube::solved(3);
        c.apply("U".parse().unwrap());
        assert_eq!(&face(&c, 2)[0..3], &[1, 1, 1]);
        // R: F's right column moves up to U's right column.
        let mut c = Cube::solved(3);
        c.apply("R".parse().unwrap());
        assert_eq!([face(&c, 0)[2], face(&c, 0)[5], face(&c, 0)[8]], [2, 2, 2]);
        // F: U's bottom row moves to R's left column.
        let mut c = Cube::solved(3);
        c.apply("F".parse().unwrap());
        assert_eq!([face(&c, 1)[0], face(&c, 1)[3], face(&c, 1)[6]], [0, 0, 0]);
    }

    #[test]
    fn from_facelets_checks_shape() {
        assert!(Cube::from_facelets(3, vec![0; 54]).is_ok());
        assert_eq!(
            Cube::from_facelets(3, vec![0; 53]),
            Err(SimError::WrongFaceletCount {
                expected: 54,
                got: 53
            })
        );
        let mut bad = vec![0; 54];
        bad[7] = 6;
        assert_eq!(
            Cube::from_facelets(3, bad),
            Err(SimError::BadColor { index: 7, value: 6 })
        );
        assert_eq!(
            Cube::from_facelets(1, vec![0; 6]),
            Err(SimError::UnsupportedSize(1))
        );
    }

    #[test]
    fn labeled_colors_match_color_cube() {
        let moves = crate::parse_moves("R 2U' F2 3R U").unwrap();
        let mut a = Cube::solved(5);
        let mut b = LabeledCube::solved(5);
        a.apply_all(&moves);
        b.apply_all(&moves);
        assert_eq!(b.to_colors(), a);
        assert!(!a.is_solved());
        a.apply_all(&crate::invert(&moves));
        assert!(a.is_solved());
    }
}
