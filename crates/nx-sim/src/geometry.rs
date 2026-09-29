//! Sticker geometry (CONVENTIONS §1–2).
//!
//! `sticker_position` is a direct port of `rg-cube/src/geometry.rs::sticker_position`, the
//! reference layout for this project. Everything else here (inverse lookup, reference move
//! permutations) is derived from it and used to build and test the fast index arithmetic in
//! `cube.rs`.

use crate::moves::{Axis, Move};

/// A point on the doubled integer lattice: sticker centres have one coordinate `±N` and the
/// others in `-(N−1)..=N−1` with the parity of `N−1`.
pub type P3 = [i64; 3];

/// Face numbers (CONVENTIONS §1).
pub const U: usize = 0;
pub const R: usize = 1;
pub const F: usize = 2;
pub const D: usize = 3;
pub const L: usize = 4;
pub const B: usize = 5;

/// Face letters in face-number order.
pub const FACE_NAMES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

/// Number of stickers on an NxN cube.
pub fn sticker_count(n: u32) -> usize {
    6 * (n as usize) * (n as usize)
}

/// Sticker index of face-local `(row, col)` (CONVENTIONS §1).
pub fn sticker_index(n: u32, face: usize, r: u32, c: u32) -> usize {
    let n = n as usize;
    face * n * n + r as usize * n + c as usize
}

/// `(face, row, col)` of a sticker index.
pub fn sticker_coords(n: u32, index: usize) -> (usize, u32, u32) {
    let nn = n as usize;
    let face = index / (nn * nn);
    let rem = index % (nn * nn);
    (face, (rem / nn) as u32, (rem % nn) as u32)
}

/// Sticker centre for face-local `(r, c)`, with the face viewed from outside in the standard
/// net orientation. Ported from `rg-cube/src/geometry.rs::sticker_position`.
pub fn sticker_position(n: u32, face: usize, r: u32, c: u32) -> P3 {
    let (n, r, c) = (i64::from(n), i64::from(r), i64::from(c));
    let m = n - 1;
    match face {
        U => [2 * c - m, n, 2 * r - m],
        R => [n, m - 2 * r, m - 2 * c],
        F => [2 * c - m, m - 2 * r, n],
        D => [2 * c - m, -n, m - 2 * r],
        L => [-n, m - 2 * r, 2 * c - m],
        B => [m - 2 * c, m - 2 * r, -n],
        _ => unreachable!("face index out of range"),
    }
}

/// Position of a sticker index.
pub fn position_of(n: u32, index: usize) -> P3 {
    let (face, r, c) = sticker_coords(n, index);
    sticker_position(n, face, r, c)
}

/// Inverse of [`sticker_position`]: the sticker index at `p`, if `p` is a sticker centre.
pub fn sticker_at(n: u32, p: P3) -> Option<usize> {
    let ni = i64::from(n);
    let m = ni - 1;
    let [x, y, z] = p;
    // (face, 2·row, 2·col), solved from the formulas in `sticker_position`.
    let (face, r2, c2) = if y == ni {
        (U, z + m, x + m)
    } else if x == ni {
        (R, m - y, m - z)
    } else if z == ni {
        (F, m - y, x + m)
    } else if y == -ni {
        (D, m - z, x + m)
    } else if x == -ni {
        (L, m - y, z + m)
    } else if z == -ni {
        (B, m - y, m - x)
    } else {
        return None;
    };
    let ok = |v: i64| v >= 0 && v <= 2 * m && v % 2 == 0;
    if !ok(r2) || !ok(c2) {
        return None;
    }
    Some(sticker_index(n, face, (r2 / 2) as u32, (c2 / 2) as u32))
}

/// Clockwise quarter turn about `axis`, as seen from the axis' positive face.
pub fn rotate_cw(axis: Axis, [x, y, z]: P3) -> P3 {
    match axis {
        Axis::X => [x, z, -y],
        Axis::Y => [-z, y, x],
        Axis::Z => [y, -x, z],
    }
}

/// Cubie centre of a sticker: the `±N` component becomes `±(N−1)`.
pub fn cubie_of(n: u32, p: P3) -> P3 {
    let ni = i64::from(n);
    p.map(|v| {
        if v.abs() == ni {
            v.signum() * (ni - 1)
        } else {
            v
        }
    })
}

/// Layer (from the positive face, CONVENTIONS §2) of a sticker's cubie along `axis`.
pub fn layer_of(n: u32, index: usize, axis: Axis) -> u32 {
    let m = i64::from(n) - 1;
    let a = cubie_of(n, position_of(n, index))[axis as usize];
    ((m - a) / 2) as u32
}

/// The face turned by layer 0 of `axis` (R, U, F).
pub const fn positive_face(axis: Axis) -> usize {
    match axis {
        Axis::X => R,
        Axis::Y => U,
        Axis::Z => F,
    }
}

/// The face turned by layer `N−1` of `axis` (L, D, B).
pub const fn negative_face(axis: Axis) -> usize {
    match axis {
        Axis::X => L,
        Axis::Y => D,
        Axis::Z => B,
    }
}

/// Reference gather permutation of a move computed from 3D geometry: after the move,
/// `new[j] = old[src[j]]`. O(stickers); used to derive and test the fast path.
pub fn move_permutation(n: u32, mv: Move) -> Vec<u32> {
    let len = sticker_count(n);
    let mut src: Vec<u32> = (0..len as u32).collect();
    for i in 0..len {
        if layer_of(n, i, mv.axis) != mv.layer {
            continue;
        }
        let mut p = position_of(n, i);
        for _ in 0..mv.turns % 4 {
            p = rotate_cw(mv.axis, p);
        }
        let j = sticker_at(n, p).expect("rotated sticker lands on a sticker");
        src[j] = i as u32;
    }
    src
}

/// The three stickers of the D-L-B corner, which never moves under allowed moves (ADR-002).
pub fn dlb_corner(n: u32) -> [usize; 3] {
    let ni = i64::from(n);
    let m = ni - 1;
    [[-m, -ni, -m], [-ni, -m, -m], [-m, -m, -ni]]
        .map(|p| sticker_at(n, p).expect("DLB sticker exists"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_at_inverts_sticker_position() {
        for n in 2..=12 {
            for i in 0..sticker_count(n) {
                assert_eq!(sticker_at(n, position_of(n, i)), Some(i), "n={n} i={i}");
            }
            assert_eq!(sticker_at(n, [0, 0, 0]), None);
        }
    }

    #[test]
    fn rotations_have_order_four() {
        let p = [1, 3, 5];
        for axis in Axis::ALL {
            let q = (0..4).fold(p, |acc, _| rotate_cw(axis, acc));
            assert_eq!(p, q);
        }
    }

    #[test]
    fn dlb_corner_stickers_are_on_d_l_b() {
        for n in 2..=9 {
            let faces = dlb_corner(n).map(|i| sticker_coords(n, i).0);
            assert_eq!(faces, [D, L, B]);
        }
    }
}
