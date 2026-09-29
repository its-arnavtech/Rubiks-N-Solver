/// Rotation axis: x points to R, y to U, z to F.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn from_index(i: u8) -> Option<Axis> {
        Self::ALL.get(usize::from(i)).copied()
    }
}

/// Any turn on any N: face, wide, slice or whole-cube rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Move {
    pub axis: Axis,
    /// Bit ℓ set = layer ℓ turns; layer 0 is the U/R/F face.
    pub layers: u8,
    /// Quarter turns, clockwise as seen from the positive face: 1, 2 or 3.
    pub turns: u8,
}

impl Move {
    pub const fn new(axis: Axis, layers: u8, turns: u8) -> Self {
        Self {
            axis,
            layers,
            turns,
        }
    }

    pub fn inverse(self) -> Self {
        Self {
            turns: (4 - self.turns % 4) % 4,
            ..self
        }
    }

    /// Compact wire encoding used by the wasm API: `[axis, layers, turns]`.
    pub fn encode(self) -> [u8; 3] {
        [self.axis as u8, self.layers, self.turns]
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        match *bytes {
            [a, layers, turns] if layers != 0 && (1..=3).contains(&turns) => {
                Some(Self::new(Axis::from_index(a)?, layers, turns))
            }
            _ => None,
        }
    }
}

/// Mirror a layer mask: layer ℓ becomes layer N−1−ℓ.
pub(crate) fn mirror_layers(layers: u8, n: u8) -> u8 {
    (0..n)
        .filter(|&l| layers & (1 << l) != 0)
        .fold(0, |acc, l| acc | 1 << (n - 1 - l))
}

/// Base generators (quarter turns) of each puzzle's move set, per `docs/05-cube-model.md` §4.
pub fn generator_moves(n: u8) -> Vec<Move> {
    let face = |axis, positive: bool, width: u8| {
        let mask = (1u8 << width) - 1;
        if positive {
            Move::new(axis, mask, 1)
        } else {
            Move::new(axis, mirror_layers(mask, n), 3)
        }
    };
    let mut gens = Vec::new();
    match n {
        // 2x2: DBL corner stays fixed, so only U, R, F turn.
        2 => gens.extend(Axis::ALL.map(|a| face(a, true, 1))),
        _ => {
            for axis in Axis::ALL {
                gens.push(face(axis, true, 1));
                gens.push(face(axis, false, 1));
            }
            // 4x4: Uw, Rw, Fw. 5x5 and up: two-layer (and wider) turns on all six faces.
            for width in 2..=n / 2 {
                for axis in Axis::ALL {
                    gens.push(face(axis, true, width));
                    if n != 4 {
                        gens.push(face(axis, false, width));
                    }
                }
            }
        }
    }
    gens
}

/// Inverse of a move sequence.
pub fn invert(moves: &[Move]) -> Vec<Move> {
    moves.iter().rev().map(|m| m.inverse()).collect()
}

/// Merge adjacent turns of the same layers and drop the ones that cancel.
pub fn simplify(moves: &[Move]) -> Vec<Move> {
    let mut out: Vec<Move> = Vec::with_capacity(moves.len());
    for &m in moves {
        match out.last_mut() {
            Some(last) if last.axis == m.axis && last.layers == m.layers => {
                let turns = (last.turns + m.turns) % 4;
                if turns == 0 {
                    out.pop();
                } else {
                    last.turns = turns;
                }
            }
            _ => out.push(m),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_counts_match_docs() {
        // Quarter-turn generators; each is used with turns 1, 2, 3.
        assert_eq!(generator_moves(2).len() * 3, 9);
        assert_eq!(generator_moves(3).len() * 3, 18);
        assert_eq!(generator_moves(4).len() * 3, 27);
        assert_eq!(generator_moves(5).len() * 3, 36);
    }

    #[test]
    fn simplify_merges_and_cancels() {
        let r = Move::new(Axis::X, 1, 1);
        let u = Move::new(Axis::Y, 1, 1);
        assert_eq!(simplify(&[r, r]), vec![Move::new(Axis::X, 1, 2)]);
        assert_eq!(simplify(&[r, u, u.inverse(), r.inverse()]), vec![]);
    }
}
