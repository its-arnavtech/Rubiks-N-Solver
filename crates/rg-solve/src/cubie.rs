//! Cubie-level model of the 3x3 (and the corners of the 2x2), derived from facelets.
//!
//! Slot numbering follows Kociemba: corners URF UFL ULB UBR DFR DLF DBL DRB, edges
//! UR UF UL UB DR DF DL DB FR FL BL BR. Which facelets form each slot, and in what order,
//! is computed from the geometry, and move cubies are obtained by reading facelets after
//! each move is applied, so no piece table is typed in by hand (docs/05 §5–6).

use std::fmt;

use rg_cube::{Geometry, P3};
use rg_graph::combinatorics::{comb_rank, comb_unrank, is_odd, perm_rank, perm_unrank};

const U: P3 = [0, 1, 0];
const D: P3 = [0, -1, 0];
const R: P3 = [1, 0, 0];
const L: P3 = [-1, 0, 0];
const F: P3 = [0, 0, 1];
const B: P3 = [0, 0, -1];

/// Corner slots by the sign of (x, y, z): URF UFL ULB UBR DFR DLF DBL DRB.
const CORNER_SIGNS: [P3; 8] = [
    [1, 1, 1],
    [-1, 1, 1],
    [-1, 1, -1],
    [1, 1, -1],
    [1, -1, 1],
    [-1, -1, 1],
    [-1, -1, -1],
    [1, -1, -1],
];

/// Edge slots as (reference face, other face): UR UF UL UB DR DF DL DB FR FL BL BR.
const EDGE_FACES: [(P3, P3); 12] = [
    (U, R),
    (U, F),
    (U, L),
    (U, B),
    (D, R),
    (D, F),
    (D, L),
    (D, B),
    (F, R),
    (F, L),
    (B, L),
    (B, R),
];

pub const DBL: usize = 6;

fn triple(a: P3, b: P3, c: P3) -> i32 {
    let cross = [
        b[1] * c[2] - b[2] * c[1],
        b[2] * c[0] - b[0] * c[2],
        b[0] * c[1] - b[1] * c[0],
    ];
    a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]
}

fn sticker(geom: &Geometry, face: P3, others: &[P3]) -> usize {
    let n = i32::from(geom.n());
    let mut p = face.map(|v| v * n);
    for o in others {
        for k in 0..3 {
            p[k] += o[k] * (n - 1);
        }
    }
    geom.index_of(p).expect("slot sticker exists")
}

/// Facelet indices of each corner slot: the U/D sticker first, then clockwise.
pub fn corner_facelets(geom: &Geometry) -> [[usize; 3]; 8] {
    CORNER_SIGNS.map(|[sx, sy, sz]| {
        let ud = [0, sy, 0];
        let (rl, fb) = ([sx, 0, 0], [0, 0, sz]);
        // Clockwise seen from outside means a negative triple product (U, R, F at URF).
        let (b, c) = if triple(ud, rl, fb) == -1 {
            (rl, fb)
        } else {
            (fb, rl)
        };
        [
            sticker(geom, ud, &[b, c]),
            sticker(geom, b, &[ud, c]),
            sticker(geom, c, &[ud, b]),
        ]
    })
}

/// Facelet indices of each edge slot of a 3x3: reference sticker (U/D, else F/B) first.
pub fn edge_facelets(geom: &Geometry) -> [[usize; 2]; 12] {
    EDGE_FACES.map(|(a, b)| [sticker(geom, a, &[b]), sticker(geom, b, &[a])])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidCube {
    WrongLength,
    ColourCount,
    CentresMisplaced,
    NoReferenceCorner,
    UnknownCorner,
    UnknownEdge,
    DuplicatePiece,
    CornerTwist,
    EdgeFlip,
    Parity,
}

impl fmt::Display for InvalidCube {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WrongLength => "wrong number of facelets",
            Self::ColourCount => "every colour must appear on exactly one face's worth of stickers",
            Self::CentresMisplaced => "the centre colours are not arranged like a real cube",
            Self::NoReferenceCorner => "no corner carries the down-back-left colours",
            Self::UnknownCorner => "a corner shows an impossible colour combination",
            Self::UnknownEdge => "an edge shows an impossible colour combination",
            Self::DuplicatePiece => "the same piece appears twice",
            Self::CornerTwist => "a corner is twisted in place (twist sum is not 0 mod 3)",
            Self::EdgeFlip => "an edge is flipped in place (flip sum is odd)",
            Self::Parity => "two pieces are swapped (corner and edge parities differ)",
        })
    }
}

impl std::error::Error for InvalidCube {}

/// Solved colour of every facelet is its face index.
fn solved_colour(geom: &Geometry, facelet: usize) -> u8 {
    let per_face = usize::from(geom.n()).pow(2);
    (facelet / per_face) as u8
}

/// Corner permutation and orientation read from facelets (Kociemba's convention:
/// `cp[i]` is the corner in slot i, `co[i]` the position of its U/D colour).
pub fn corners_from_facelets(geom: &Geometry, f: &[u8]) -> Result<([u8; 8], [u8; 8]), InvalidCube> {
    let slots = corner_facelets(geom);
    let colours = slots.map(|s| s.map(|i| solved_colour(geom, i)));
    let (mut cp, mut co) = ([0u8; 8], [0u8; 8]);
    for (i, slot) in slots.iter().enumerate() {
        let c = slot.map(|k| f[k]);
        let ori = c
            .iter()
            .position(|&x| x == 0 || x == 3)
            .ok_or(InvalidCube::UnknownCorner)?;
        let (c1, c2) = (c[(ori + 1) % 3], c[(ori + 2) % 3]);
        let j = colours
            .iter()
            .position(|col| col[1] == c1 && col[2] == c2)
            .ok_or(InvalidCube::UnknownCorner)?;
        cp[i] = j as u8;
        co[i] = ori as u8;
    }
    if !is_permutation(&cp) {
        return Err(InvalidCube::DuplicatePiece);
    }
    if co.iter().map(|&o| u32::from(o)).sum::<u32>() % 3 != 0 {
        return Err(InvalidCube::CornerTwist);
    }
    Ok((cp, co))
}

fn is_permutation(p: &[u8]) -> bool {
    let mut seen = 0u32;
    p.iter().all(|&x| {
        let bit = 1 << x;
        let fresh = usize::from(x) < p.len() && seen & bit == 0;
        seen |= bit;
        fresh
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CubieCube {
    pub cp: [u8; 8],
    pub co: [u8; 8],
    pub ep: [u8; 12],
    pub eo: [u8; 12],
}

impl CubieCube {
    pub const SOLVED: Self = Self {
        cp: [0, 1, 2, 3, 4, 5, 6, 7],
        co: [0; 8],
        ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        eo: [0; 12],
    };

    /// Read a 3x3 whose centres are in standard position. Checks every solvability law.
    pub fn from_facelets(f: &[u8]) -> Result<Self, InvalidCube> {
        let geom = Geometry::new(3);
        if f.len() != 54 {
            return Err(InvalidCube::WrongLength);
        }
        if (0..6).any(|c| f.iter().filter(|&&x| x == c).count() != 9) {
            return Err(InvalidCube::ColourCount);
        }
        if (0..6).any(|face| f[face * 9 + 4] != face as u8) {
            return Err(InvalidCube::CentresMisplaced);
        }
        let (cp, co) = corners_from_facelets(&geom, f)?;
        let slots = edge_facelets(&geom);
        let colours = slots.map(|s| s.map(|i| solved_colour(&geom, i)));
        let (mut ep, mut eo) = ([0u8; 12], [0u8; 12]);
        for (i, slot) in slots.iter().enumerate() {
            let (a, b) = (f[slot[0]], f[slot[1]]);
            let (j, o) = colours
                .iter()
                .enumerate()
                .find_map(
                    |(j, c)| match (c[0] == a && c[1] == b, c[0] == b && c[1] == a) {
                        (true, _) => Some((j, 0)),
                        (_, true) => Some((j, 1)),
                        _ => None,
                    },
                )
                .ok_or(InvalidCube::UnknownEdge)?;
            ep[i] = j as u8;
            eo[i] = o;
        }
        if !is_permutation(&ep) {
            return Err(InvalidCube::DuplicatePiece);
        }
        if eo.iter().map(|&o| u32::from(o)).sum::<u32>() % 2 != 0 {
            return Err(InvalidCube::EdgeFlip);
        }
        if is_odd(&cp) != is_odd(&ep) {
            return Err(InvalidCube::Parity);
        }
        Ok(Self { cp, co, ep, eo })
    }

    /// `self` followed by `b` (Kociemba's corner/edge multiplication).
    pub fn mul(&self, b: &Self) -> Self {
        let mut r = *self;
        for i in 0..8 {
            let j = usize::from(b.cp[i]);
            r.cp[i] = self.cp[j];
            r.co[i] = (self.co[j] + b.co[i]) % 3;
        }
        for i in 0..12 {
            let j = usize::from(b.ep[i]);
            r.ep[i] = self.ep[j];
            r.eo[i] = (self.eo[j] + b.eo[i]) % 2;
        }
        r
    }

    // ---- Kociemba coordinates (docs/07 §4.1) ----

    /// Corner orientation, 0..2187.
    pub fn twist(&self) -> u16 {
        self.co[..7].iter().fold(0, |a, &o| a * 3 + u16::from(o))
    }

    pub fn set_twist(&mut self, mut t: u16) {
        let mut sum = 0;
        for i in (0..7).rev() {
            self.co[i] = (t % 3) as u8;
            sum += self.co[i];
            t /= 3;
        }
        self.co[7] = (3 - sum % 3) % 3;
    }

    /// Edge orientation, 0..2048.
    pub fn flip(&self) -> u16 {
        self.eo[..11].iter().fold(0, |a, &o| a * 2 + u16::from(o))
    }

    pub fn set_flip(&mut self, mut f: u16) {
        let mut sum = 0;
        for i in (0..11).rev() {
            self.eo[i] = (f % 2) as u8;
            sum += self.eo[i];
            f /= 2;
        }
        self.eo[11] = sum % 2;
    }

    /// Positions of the four E-slice edges (FR FL BL BR), 0..495.
    pub fn slice(&self) -> u16 {
        let pos: Vec<u8> = (0..12u8)
            .filter(|&i| self.ep[usize::from(i)] >= 8)
            .collect();
        comb_rank(&pos) as u16
    }

    pub fn set_slice(&mut self, r: u16) {
        let pos = comb_unrank(u32::from(r), 12, 4);
        let (mut slice, mut other) = (8, 0);
        for i in 0..12u8 {
            if pos.contains(&i) {
                self.ep[usize::from(i)] = slice;
                slice += 1;
            } else {
                self.ep[usize::from(i)] = other;
                other += 1;
            }
        }
    }

    /// Corner permutation, 0..40320.
    pub fn corner_perm(&self) -> u16 {
        perm_rank(&self.cp) as u16
    }

    pub fn set_corner_perm(&mut self, r: u16) {
        self.cp.copy_from_slice(&perm_unrank(u32::from(r), 8));
    }

    /// Permutation of the eight U/D-layer edges (valid inside G1), 0..40320.
    pub fn ud_edges(&self) -> u16 {
        perm_rank(&self.ep[..8]) as u16
    }

    pub fn set_ud_edges(&mut self, r: u16) {
        self.ep[..8].copy_from_slice(&perm_unrank(u32::from(r), 8));
        self.ep[8..].copy_from_slice(&[8, 9, 10, 11]);
    }

    /// Permutation of the E-slice edges within the slice (valid inside G1), 0..24.
    pub fn slice_perm(&self) -> u8 {
        let p: Vec<u8> = self.ep[8..].iter().map(|&e| e.saturating_sub(8)).collect();
        perm_rank(&p) as u8
    }

    pub fn set_slice_perm(&mut self, r: u8) {
        self.ep = Self::SOLVED.ep;
        for (k, v) in perm_unrank(u32::from(r), 4).into_iter().enumerate() {
            self.ep[8 + k] = 8 + v;
        }
    }
}

/// Cubie effect of each move, read back from the facelet model.
pub fn move_cubies(moves: &[rg_cube::Move]) -> Vec<CubieCube> {
    moves
        .iter()
        .map(|&m| {
            let mut c = rg_cube::Cube::new(3).expect("3x3");
            c.apply_move(m);
            CubieCube::from_facelets(c.facelets()).expect("a face turn yields a valid cube")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_facelets_match_kociemba_layout() {
        let g = Geometry::new(3);
        let c = corner_facelets(&g);
        assert_eq!(c[0], [8, 9, 20]); // URF: U9 R1 F3
        assert_eq!(c[6], [33, 53, 42]); // DBL: D7 B9 L7
        let e = edge_facelets(&g);
        assert_eq!(e[0], [5, 10]); // UR: U6 R2
        assert_eq!(e[11], [48, 14]); // BR: B4 R6
    }

    #[test]
    fn cubie_multiplication_agrees_with_facelets() {
        let moves =
            rg_cube::parse_alg(3, "U U2 U' R R2 R' F F2 F' D D2 D' L L2 L' B B2 B'").unwrap();
        let cubies = move_cubies(&moves);
        for seed in 0..40 {
            let seq = rg_cube::random_move_scramble(3, 30, seed);
            let mut facelets = rg_cube::Cube::new(3).unwrap();
            facelets.apply_moves(&seq);
            let product = seq.iter().fold(CubieCube::SOLVED, |c, m| {
                let i = moves.iter().position(|x| x == m).unwrap();
                c.mul(&cubies[i])
            });
            assert_eq!(
                CubieCube::from_facelets(facelets.facelets()).unwrap(),
                product
            );
        }
    }

    #[test]
    fn coordinates_round_trip() {
        let mut c = CubieCube::SOLVED;
        for t in 0..2187 {
            c.set_twist(t);
            assert_eq!(c.twist(), t);
        }
        for f in 0..2048 {
            c.set_flip(f);
            assert_eq!(c.flip(), f);
        }
        for s in 0..495 {
            c.set_slice(s);
            assert_eq!(c.slice(), s);
        }
        for p in (0..40320).step_by(7) {
            c.set_corner_perm(p);
            assert_eq!(c.corner_perm(), p);
            c.set_ud_edges(p);
            assert_eq!(c.ud_edges(), p);
        }
        for p in 0..24 {
            c.set_slice_perm(p);
            assert_eq!(c.slice_perm(), p);
        }
    }

    #[test]
    fn detects_illegal_cubes() {
        let solved: Vec<u8> = (0..54).map(|i| (i / 9) as u8).collect();
        let mut twisted = solved.clone();
        twisted.swap(8, 9); // rotate URF: U9 -> R1 -> F3 -> U9
        twisted.swap(8, 20);
        assert_eq!(
            CubieCube::from_facelets(&twisted),
            Err(InvalidCube::CornerTwist)
        );
        let mut flipped = solved.clone();
        flipped.swap(5, 10); // UR
        assert_eq!(
            CubieCube::from_facelets(&flipped),
            Err(InvalidCube::EdgeFlip)
        );
    }
}
