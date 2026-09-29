use std::collections::HashMap;

use crate::moves::{Axis, Move};

/// A point on the doubled integer lattice (see `docs/05-cube-model.md` §2).
pub type P3 = [i32; 3];

/// Sticker centre for face-local (row, col), with the face viewed from outside in
/// the standard net orientation.
fn sticker_position(n: i32, face: usize, r: i32, c: i32) -> P3 {
    let m = n - 1;
    match face {
        0 => [2 * c - m, n, 2 * r - m],  // U
        1 => [n, m - 2 * r, m - 2 * c],  // R
        2 => [2 * c - m, m - 2 * r, n],  // F
        3 => [2 * c - m, -n, m - 2 * r], // D
        4 => [-n, m - 2 * r, 2 * c - m], // L
        5 => [m - 2 * c, m - 2 * r, -n], // B
        _ => unreachable!("face index out of range"),
    }
}

/// Clockwise quarter turn about `axis`, as seen from the axis' positive face.
pub fn rotate_cw(axis: Axis, [x, y, z]: P3) -> P3 {
    match axis {
        Axis::X => [x, z, -y],
        Axis::Y => [-z, y, x],
        Axis::Z => [y, -x, z],
    }
}

/// Number of visible cubies on an NxN cube.
pub fn cubie_count(n: u8) -> usize {
    let n = n as usize;
    n * n * n - n.saturating_sub(2).pow(3)
}

/// Sticker positions, their cubie centres, and the reverse lookup for one cube size.
#[derive(Clone, Debug)]
pub struct Geometry {
    n: u8,
    positions: Vec<P3>,
    cubies: Vec<P3>,
    index: HashMap<P3, u16>,
}

impl Geometry {
    pub fn new(n: u8) -> Self {
        let ni = i32::from(n);
        let mut positions = Vec::with_capacity(6 * usize::from(n) * usize::from(n));
        for face in 0..6 {
            for r in 0..ni {
                for c in 0..ni {
                    positions.push(sticker_position(ni, face, r, c));
                }
            }
        }
        // The cubie centre replaces the ±N component with ±(N−1).
        let cubies = positions
            .iter()
            .map(|p| {
                p.map(|v| {
                    if v.abs() == ni {
                        v.signum() * (ni - 1)
                    } else {
                        v
                    }
                })
            })
            .collect();
        let index = positions
            .iter()
            .enumerate()
            .map(|(i, &p)| (p, u16::try_from(i).expect("sticker index fits in u16")))
            .collect();
        Self {
            n,
            positions,
            cubies,
            index,
        }
    }

    pub fn n(&self) -> u8 {
        self.n
    }

    pub fn sticker_count(&self) -> usize {
        self.positions.len()
    }

    /// Doubled-coordinate sticker centres, in facelet order.
    pub fn positions(&self) -> &[P3] {
        &self.positions
    }

    /// Doubled-coordinate cubie centre of each sticker, in facelet order.
    pub fn cubies(&self) -> &[P3] {
        &self.cubies
    }

    pub fn index_of(&self, p: P3) -> Option<usize> {
        self.index.get(&p).map(|&i| usize::from(i))
    }

    /// Layer of a sticker's cubie along `axis`, counted from the positive face (0).
    pub fn layer_of(&self, sticker: usize, axis: Axis) -> u8 {
        let a = self.cubies[sticker][axis as usize];
        u8::try_from((i32::from(self.n) - 1 - a) / 2).expect("layer in range")
    }

    /// Gather permutation of `mv`: after the move, `new[j] = old[src[j]]`.
    pub fn move_permutation(&self, mv: Move) -> Vec<u16> {
        let len = self.positions.len();
        let mut src: Vec<u16> = (0..len).map(|i| i as u16).collect();
        for i in 0..len {
            if mv.layers & (1 << self.layer_of(i, mv.axis)) == 0 {
                continue;
            }
            let mut p = self.positions[i];
            for _ in 0..mv.turns % 4 {
                p = rotate_cw(mv.axis, p);
            }
            let j = self
                .index_of(p)
                .expect("rotated sticker lands on a sticker");
            src[j] = i as u16;
        }
        src
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn positions_are_distinct_and_cubie_counts_match() {
        for n in 2..=7 {
            let g = Geometry::new(n);
            assert_eq!(
                g.index.len(),
                g.sticker_count(),
                "n={n}: duplicate sticker position"
            );
            let distinct: HashSet<P3> = g.cubies().iter().copied().collect();
            assert_eq!(distinct.len(), cubie_count(n), "n={n}");
        }
    }

    #[test]
    fn rotations_have_order_four() {
        let p = [1, 3, 5];
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let q = (0..4).fold(p, |acc, _| rotate_cw(axis, acc));
            assert_eq!(p, q);
        }
    }
}
