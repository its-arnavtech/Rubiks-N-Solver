//! Axes, single-layer moves, wire encoding and display notation (CONVENTIONS §2).

use std::fmt;
use std::str::FromStr;

/// Rotation axis: x points to R, y to U, z to F.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Axis {
    X = 0,
    Y = 1,
    Z = 2,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn from_index(i: u32) -> Option<Axis> {
        Self::ALL.get(i as usize).copied()
    }

    /// Face letter of layer 0 on this axis.
    pub const fn letter(self) -> char {
        match self {
            Axis::X => 'R',
            Axis::Y => 'U',
            Axis::Z => 'F',
        }
    }

    /// Axis name used in symbolic moves: `x`, `y`, `z`.
    pub const fn name(self) -> char {
        match self {
            Axis::X => 'x',
            Axis::Y => 'y',
            Axis::Z => 'z',
        }
    }
}

/// One layer turned on one axis. Layer 0 is the R/U/F face; allowed moves use layers
/// `0..=N−2` (the fixed-corner frame, ADR-002).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Move {
    pub axis: Axis,
    pub layer: u32,
    /// Clockwise quarter turns as seen from the positive face: 1, 2 or 3.
    pub turns: u8,
}

/// Largest layer the wire encoding can carry.
pub const MAX_LAYER: u32 = (1 << 28) - 1;

impl Move {
    pub const fn new(axis: Axis, layer: u32, turns: u8) -> Self {
        Self { axis, layer, turns }
    }

    pub const fn inverse(self) -> Self {
        Self {
            turns: (4 - self.turns % 4) % 4,
            ..self
        }
    }

    /// Allowed on an NxN cube: layer in `0..=N−2`, turns in `1..=3`.
    pub fn is_allowed(self, n: u32) -> bool {
        self.layer + 2 <= n && (1..=3).contains(&self.turns)
    }

    /// Wire encoding: `(layer << 4) | (axis << 2) | turns`.
    pub fn encode(self) -> u32 {
        debug_assert!(self.layer <= MAX_LAYER);
        (self.layer << 4) | ((self.axis as u32) << 2) | u32::from(self.turns)
    }

    pub fn decode(w: u32) -> Option<Self> {
        let turns = (w & 3) as u8;
        if turns == 0 {
            return None;
        }
        Some(Self::new(Axis::from_index((w >> 2) & 3)?, w >> 4, turns))
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.layer > 0 {
            write!(f, "{}", self.layer + 1)?;
        }
        let suffix = match self.turns {
            1 => "",
            2 => "2",
            3 => "'",
            _ => "?",
        };
        write!(f, "{}{suffix}", self.axis.letter())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseMoveError(pub String);

impl fmt::Display for ParseMoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid move {:?}", self.0)
    }
}

impl std::error::Error for ParseMoveError {}

impl FromStr for Move {
    type Err = ParseMoveError;

    /// Parses display notation: `R`, `U2`, `F'`, `2R`, `13U'`. The prefix is `layer + 1` and
    /// must be at least 2 when present.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseMoveError(s.to_string());
        let digits = s.chars().take_while(char::is_ascii_digit).count();
        let rest = &s[digits..];
        let layer = if digits == 0 {
            0
        } else {
            let depth: u32 = s[..digits].parse().map_err(|_| err())?;
            if depth < 2 || depth - 1 > MAX_LAYER {
                return Err(err());
            }
            depth - 1
        };
        let mut chars = rest.chars();
        let axis = match chars.next() {
            Some('R') => Axis::X,
            Some('U') => Axis::Y,
            Some('F') => Axis::Z,
            _ => return Err(err()),
        };
        let turns = match chars.as_str() {
            "" => 1,
            "2" => 2,
            "'" => 3,
            _ => return Err(err()),
        };
        Ok(Self::new(axis, layer, turns))
    }
}

/// Every allowed move on an NxN cube, ordered by axis, layer, turns.
pub fn allowed_moves(n: u32) -> Vec<Move> {
    let mut out = Vec::with_capacity(9 * n.saturating_sub(1) as usize);
    for axis in Axis::ALL {
        for layer in 0..n.saturating_sub(1) {
            for turns in 1..=3 {
                out.push(Move::new(axis, layer, turns));
            }
        }
    }
    out
}

/// Inverse of a move sequence.
pub fn invert(moves: &[Move]) -> Vec<Move> {
    moves.iter().rev().map(|m| m.inverse()).collect()
}

/// Space-separated display notation.
pub fn format_moves(moves: &[Move]) -> String {
    moves
        .iter()
        .map(Move::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse whitespace-separated display notation.
pub fn parse_moves(s: &str) -> Result<Vec<Move>, ParseMoveError> {
    s.split_whitespace().map(str::parse).collect()
}

/// Little-endian `u32` wire bytes of a move list (base64 is added at the API boundary).
pub fn encode_moves(moves: &[Move]) -> Vec<u8> {
    moves
        .iter()
        .flat_map(|m| m.encode().to_le_bytes())
        .collect()
}

/// Inverse of [`encode_moves`]. `None` if the length is not a multiple of 4 or a word is invalid.
pub fn decode_moves(bytes: &[u8]) -> Option<Vec<Move>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    bytes
        .chunks_exact(4)
        .map(|b| Move::decode(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_encoding_round_trips() {
        for n in [2, 3, 7, 100, 1025] {
            for m in allowed_moves(n) {
                assert_eq!(Move::decode(m.encode()), Some(m));
            }
        }
        assert_eq!(Move::new(Axis::Z, 3, 2).encode(), (3 << 4) | (2 << 2) | 2);
        assert_eq!(Move::decode(0), None);
        assert_eq!(Move::decode((3 << 2) | 1), None);
        let ms = allowed_moves(5);
        assert_eq!(decode_moves(&encode_moves(&ms)), Some(ms));
        assert_eq!(decode_moves(&[1, 0, 0]), None);
    }

    #[test]
    fn notation_round_trips() {
        let cases = [
            (Move::new(Axis::X, 0, 1), "R"),
            (Move::new(Axis::Y, 0, 2), "U2"),
            (Move::new(Axis::Z, 0, 3), "F'"),
            (Move::new(Axis::X, 1, 1), "2R"),
            (Move::new(Axis::Y, 12, 3), "13U'"),
        ];
        for (m, s) in cases {
            assert_eq!(m.to_string(), s);
            assert_eq!(s.parse::<Move>(), Ok(m));
        }
        for bad in ["", "L", "1R", "0R", "R3", "R2'", "2", "r"] {
            assert!(bad.parse::<Move>().is_err(), "{bad}");
        }
        let alg = "R 2U' F2 13R";
        assert_eq!(format_moves(&parse_moves(alg).unwrap()), alg);
    }

    #[test]
    fn allowed_moves_skip_the_last_layer() {
        assert_eq!(allowed_moves(2).len(), 9);
        assert_eq!(allowed_moves(5).len(), 36);
        assert!(
            allowed_moves(6)
                .iter()
                .all(|m| m.is_allowed(6) && m.layer <= 4)
        );
        assert!(!Move::new(Axis::X, 5, 1).is_allowed(6));
    }

    #[test]
    fn inverse_and_invert() {
        let m = Move::new(Axis::Y, 2, 1);
        assert_eq!(m.inverse().turns, 3);
        assert_eq!(m.inverse().inverse(), m);
        assert_eq!(Move::new(Axis::Y, 2, 2).inverse().turns, 2);
        let seq = parse_moves("R U2 3F'").unwrap();
        assert_eq!(format_moves(&invert(&seq)), "3F U2 R'");
    }
}
