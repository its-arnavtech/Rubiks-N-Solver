use std::collections::HashMap;
use std::fmt;

use crate::geometry::Geometry;
use crate::moves::Move;
use crate::notation::{ParseError, parse_alg};
use crate::{MAX_N, MIN_N};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CubeError {
    UnsupportedSize(u8),
    WrongFaceletCount { expected: usize, got: usize },
    BadColour { index: usize, value: u8 },
    Parse(ParseError),
}

impl fmt::Display for CubeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSize(n) => write!(
                f,
                "unsupported cube size {n} (supported: {MIN_N}..={MAX_N})"
            ),
            Self::WrongFaceletCount { expected, got } => {
                write!(f, "expected {expected} facelets, got {got}")
            }
            Self::BadColour { index, value } => {
                write!(f, "facelet {index} has invalid colour {value}")
            }
            Self::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CubeError {}

impl From<ParseError> for CubeError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

/// Facelet-level cube: 6·N² colours in U R F D L B order. The ground truth that
/// every solver's output is verified against.
#[derive(Clone, Debug)]
pub struct Cube {
    geom: Geometry,
    facelets: Vec<u8>,
    perms: HashMap<Move, Vec<u16>>,
}

impl Cube {
    pub fn new(n: u8) -> Result<Self, CubeError> {
        if !(MIN_N..=MAX_N).contains(&n) {
            return Err(CubeError::UnsupportedSize(n));
        }
        let geom = Geometry::new(n);
        let facelets = solved_facelets(n);
        Ok(Self {
            geom,
            facelets,
            perms: HashMap::new(),
        })
    }

    pub fn n(&self) -> u8 {
        self.geom.n()
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geom
    }

    pub fn facelets(&self) -> &[u8] {
        &self.facelets
    }

    /// Replace the colours. Checks shape only; full solvability validation is separate.
    pub fn set_facelets(&mut self, facelets: &[u8]) -> Result<(), CubeError> {
        if facelets.len() != self.facelets.len() {
            return Err(CubeError::WrongFaceletCount {
                expected: self.facelets.len(),
                got: facelets.len(),
            });
        }
        if let Some((index, &value)) = facelets.iter().enumerate().find(|&(_, &c)| c > 5) {
            return Err(CubeError::BadColour { index, value });
        }
        self.facelets.copy_from_slice(facelets);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.facelets = solved_facelets(self.n());
    }

    pub fn apply_move(&mut self, mv: Move) {
        let geom = &self.geom;
        let src = self
            .perms
            .entry(mv)
            .or_insert_with(|| geom.move_permutation(mv));
        let old = self.facelets.clone();
        for (dst, &s) in self.facelets.iter_mut().zip(src.iter()) {
            *dst = old[usize::from(s)];
        }
    }

    pub fn apply_moves(&mut self, moves: &[Move]) {
        for &m in moves {
            self.apply_move(m);
        }
    }

    /// Parse and apply an algorithm; returns the parsed moves.
    pub fn apply_alg(&mut self, alg: &str) -> Result<Vec<Move>, CubeError> {
        let moves = parse_alg(self.n(), alg)?;
        self.apply_moves(&moves);
        Ok(moves)
    }

    /// Every face shows a single colour (in any orientation of the whole cube).
    pub fn is_solved(&self) -> bool {
        let per_face = usize::from(self.n()).pow(2);
        self.facelets
            .chunks(per_face)
            .all(|face| face.iter().all(|&c| c == face[0]))
    }
}

fn solved_facelets(n: u8) -> Vec<u8> {
    let per_face = usize::from(n).pow(2);
    (0..6u8)
        .flat_map(|f| std::iter::repeat_n(f, per_face))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moves::{Axis, generator_moves, invert};

    fn order(n: u8, alg: &str) -> usize {
        let moves = parse_alg(n, alg).unwrap();
        let mut c = Cube::new(n).unwrap();
        let solved = c.facelets().to_vec();
        for k in 1.. {
            c.apply_moves(&moves);
            if c.facelets() == solved {
                return k;
            }
        }
        unreachable!()
    }

    fn face(c: &Cube, f: usize) -> &[u8] {
        let per = usize::from(c.n()).pow(2);
        &c.facelets()[f * per..(f + 1) * per]
    }

    #[test]
    fn hand_checked_sticker_movements() {
        // U: R's top row moves to F's top row.
        let mut c = Cube::new(3).unwrap();
        c.apply_alg("U").unwrap();
        assert_eq!(&face(&c, 2)[0..3], &[1, 1, 1]);
        // R: F's right column moves up to U's right column.
        let mut c = Cube::new(3).unwrap();
        c.apply_alg("R").unwrap();
        assert_eq!([face(&c, 0)[2], face(&c, 0)[5], face(&c, 0)[8]], [2, 2, 2]);
        // F: U's bottom row moves to R's left column.
        let mut c = Cube::new(3).unwrap();
        c.apply_alg("F").unwrap();
        assert_eq!([face(&c, 1)[0], face(&c, 1)[3], face(&c, 1)[6]], [0, 0, 0]);
    }

    #[test]
    fn known_orders() {
        assert_eq!(order(3, "R U"), 105);
        assert_eq!(order(3, "R U R' U'"), 6);
        assert_eq!(order(3, "R"), 4);
        assert_eq!(order(3, "R2 U2"), 6);
    }

    #[test]
    fn every_turn_has_order_four_and_inverts() {
        for n in 2..=5u8 {
            for axis in Axis::ALL {
                for layer in 0..n {
                    let m = Move::new(axis, 1 << layer, 1);
                    let mut c = Cube::new(n).unwrap();
                    c.apply_alg("R U F' D2 L B'").unwrap_or_default();
                    let start = c.facelets().to_vec();
                    c.apply_moves(&[m, m.inverse()]);
                    assert_eq!(c.facelets(), start, "n={n} {m:?} inverse");
                    c.apply_moves(&[m; 4]);
                    assert_eq!(c.facelets(), start, "n={n} {m:?} order");
                }
            }
        }
    }

    #[test]
    fn rotation_equals_its_layers() {
        // x = R M' L' on 3x3; Uw = U 2U on 4x4.
        let mut a = Cube::new(3).unwrap();
        let mut b = Cube::new(3).unwrap();
        a.apply_alg("F x").unwrap();
        b.apply_alg("F R M' L'").unwrap();
        assert_eq!(a.facelets(), b.facelets());

        let mut a = Cube::new(4).unwrap();
        let mut b = Cube::new(4).unwrap();
        a.apply_alg("F Uw").unwrap();
        b.apply_alg("F U 2U").unwrap();
        assert_eq!(a.facelets(), b.facelets());
    }

    #[test]
    fn scramble_then_inverse_solves_and_generators_never_move_fixed_frames() {
        for n in 2..=5u8 {
            let mut c = Cube::new(n).unwrap();
            let s = crate::random_move_scramble(n, 30, 7);
            c.apply_moves(&s);
            assert!(!c.is_solved());
            c.apply_moves(&invert(&s));
            assert!(c.is_solved());
        }
        // 2x2 generators never touch the DBL corner; 5x5 generators never move centres.
        let mut c = Cube::new(2).unwrap();
        let dbl = [(3, 2), (4, 2), (5, 3)].map(|(f, i)| f * 4 + i);
        for g in generator_moves(2) {
            c.apply_move(g);
        }
        assert!(dbl.iter().all(|&i| usize::from(c.facelets()[i]) == i / 4));
        let mut c = Cube::new(5).unwrap();
        c.apply_moves(&crate::random_move_scramble(5, 60, 3));
        assert!((0..6).all(|f| c.facelets()[f * 25 + 12] == f as u8));
    }
}
