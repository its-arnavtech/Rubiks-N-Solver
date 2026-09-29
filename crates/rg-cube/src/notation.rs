//! WCA notation plus a SiGN subset (see `docs/05-cube-model.md` §7).

use std::fmt;

use crate::moves::{Axis, Move, mirror_layers};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Byte offsets of the offending token.
    pub start: usize,
    pub end: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {}..{})", self.message, self.start, self.end)
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    n: u8,
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn err<T>(&self, start: usize, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            message: message.into(),
            start,
            end: self.pos.max(start + 1),
        })
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace() || c == ',') {
            self.bump();
        }
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.bump();
        }
        self.src[start..self.pos].parse().ok()
    }

    /// Optional amount suffix: `2`, `'`, `2'` → multiplier 2, 3 (inverse), 2.
    fn amount(&mut self) -> u8 {
        let mut mult = 1;
        if self.peek() == Some('2') {
            self.bump();
            mult = 2;
        }
        if matches!(self.peek(), Some('\'' | '’')) {
            self.bump();
            mult = if mult == 2 { 2 } else { 3 };
        }
        mult
    }

    fn sequence(&mut self, depth: usize) -> Result<Vec<Move>, ParseError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => break,
                Some(')') if depth > 0 => break,
                Some(_) => out.extend(self.item(depth)?),
            }
        }
        Ok(out)
    }

    fn item(&mut self, depth: usize) -> Result<Vec<Move>, ParseError> {
        let start = self.pos;
        if self.peek() == Some('(') {
            self.bump();
            let inner = self.sequence(depth + 1)?;
            if self.bump() != Some(')') {
                return self.err(start, "unclosed '('");
            }
            let count = self.number().unwrap_or(1) as usize;
            return Ok(inner
                .iter()
                .copied()
                .cycle()
                .take(inner.len() * count)
                .collect());
        }
        let prefix = self.number();
        let Some(c) = self.bump() else {
            return self.err(start, "expected a move after the layer number");
        };
        let n = self.n;
        let full = ((1u16 << n) - 1) as u8;
        let (axis, layers, base) = match c {
            'U' | 'D' | 'R' | 'L' | 'F' | 'B' => {
                let wide = self.peek() == Some('w');
                if wide {
                    self.bump();
                }
                let k = prefix.unwrap_or(if wide { 2 } else { 1 });
                if k == 0 || k > u32::from(n) {
                    return self.err(
                        start,
                        format!("layer count {k} is out of range for a {n}x{n}"),
                    );
                }
                let from_top = if wide {
                    ((1u16 << k) - 1) as u8
                } else {
                    1 << (k - 1)
                };
                face_turn(c, from_top, n)
            }
            'u' | 'd' | 'r' | 'l' | 'f' | 'b' => {
                let k = prefix.unwrap_or(2);
                if k == 0 || k > u32::from(n) {
                    return self.err(
                        start,
                        format!("layer count {k} is out of range for a {n}x{n}"),
                    );
                }
                face_turn(c.to_ascii_uppercase(), ((1u16 << k) - 1) as u8, n)
            }
            'M' | 'E' | 'S' => {
                if prefix.is_some() {
                    return self.err(start, "slice moves take no layer number");
                }
                if n % 2 == 0 {
                    return self.err(
                        start,
                        format!("{c} needs a middle layer; a {n}x{n} has none"),
                    );
                }
                let mid = 1 << ((n - 1) / 2);
                match c {
                    'M' => (Axis::X, mid, 3),
                    'E' => (Axis::Y, mid, 3),
                    _ => (Axis::Z, mid, 1),
                }
            }
            'x' | 'y' | 'z' => {
                if prefix.is_some() {
                    return self.err(start, "rotations take no layer number");
                }
                let axis = match c {
                    'x' => Axis::X,
                    'y' => Axis::Y,
                    _ => Axis::Z,
                };
                (axis, full, 1)
            }
            other => return self.err(start, format!("unexpected '{other}'")),
        };
        let turns = (base * self.amount()) % 4;
        Ok(vec![Move::new(axis, layers, turns)])
    }
}

/// `(axis, layers, base turns)` for a face letter and a mask counted from that face.
fn face_turn(face: char, from_face: u8, n: u8) -> (Axis, u8, u8) {
    let (axis, positive) = match face {
        'U' => (Axis::Y, true),
        'D' => (Axis::Y, false),
        'R' => (Axis::X, true),
        'L' => (Axis::X, false),
        'F' => (Axis::Z, true),
        _ => (Axis::Z, false),
    };
    if positive {
        (axis, from_face, 1)
    } else {
        (axis, mirror_layers(from_face, n), 3)
    }
}

/// Parse an algorithm for an NxN cube.
pub fn parse_alg(n: u8, src: &str) -> Result<Vec<Move>, ParseError> {
    let mut p = Parser { n, src, pos: 0 };
    let moves = p.sequence(0)?;
    if p.pos < src.len() {
        return p.err(p.pos, "unmatched ')'");
    }
    Ok(moves)
}

fn suffix(turns: u8) -> &'static str {
    match turns % 4 {
        2 => "2",
        3 => "'",
        _ => "",
    }
}

fn is_prefix_mask(layers: u8) -> bool {
    layers != 0 && layers & layers.wrapping_add(1) == 0
}

/// WCA-style name of a single move.
pub fn format_move(n: u8, m: Move) -> String {
    let (pos, neg, rot, slice) = match m.axis {
        Axis::X => ('R', 'L', 'x', 'M'),
        Axis::Y => ('U', 'D', 'y', 'E'),
        Axis::Z => ('F', 'B', 'z', 'S'),
    };
    let full = ((1u16 << n) - 1) as u8;
    let width = |mask: u8| match mask.count_ones() {
        1 => String::new(),
        2 => "w".into(),
        k => format!("{k}"),
    };
    let named = |face: char, mask: u8, turns: u8| {
        let w = width(mask);
        match w.as_str() {
            "" => format!("{face}{}", suffix(turns)),
            "w" => format!("{face}w{}", suffix(turns)),
            k => format!("{k}{face}w{}", suffix(turns)),
        }
    };
    let mirrored = mirror_layers(m.layers, n);
    if m.layers == full {
        format!("{rot}{}", suffix(m.turns))
    } else if is_prefix_mask(m.layers) {
        named(pos, m.layers, m.turns)
    } else if is_prefix_mask(mirrored) {
        named(neg, mirrored, 4 - m.turns)
    } else if m.layers.count_ones() == 1 {
        let l = m.layers.trailing_zeros() as u8;
        if n % 2 == 1 && l == (n - 1) / 2 {
            // M and E follow L and D; S follows F.
            let t = if m.axis == Axis::Z {
                m.turns
            } else {
                4 - m.turns
            };
            format!("{slice}{}", suffix(t))
        } else if l < n - 1 - l {
            format!("{}{pos}{}", l + 1, suffix(m.turns))
        } else {
            format!("{}{neg}{}", n - l, suffix(4 - m.turns))
        }
    } else {
        format!(
            "[{rot}:{:0width$b}{}]",
            m.layers,
            suffix(m.turns),
            width = usize::from(n)
        )
    }
}

/// Space-separated WCA-style algorithm.
pub fn format_alg(n: u8, moves: &[Move]) -> String {
    moves
        .iter()
        .map(|&m| format_move(n, m))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for (n, alg) in [
            (3, "R U R' U' F2 D' L B2 M E' S2 x y' z2"),
            (4, "Rw U2 Fw' 3Rw 2R 2L' D Lw2"),
            (5, "Uw Dw' Lw2 Bw 2U 2D' M S'"),
            (2, "U R2 F'"),
        ] {
            let moves = parse_alg(n, alg).unwrap();
            assert_eq!(format_alg(n, &moves), alg, "n={n}");
        }
    }

    #[test]
    fn sign_lowercase_is_wide_and_groups_repeat() {
        assert_eq!(parse_alg(3, "r").unwrap(), parse_alg(3, "Rw").unwrap());
        assert_eq!(parse_alg(3, "(R U)3").unwrap().len(), 6);
        assert_eq!(parse_alg(3, "R2'").unwrap(), parse_alg(3, "R2").unwrap());
    }

    #[test]
    fn errors_point_at_the_token() {
        let e = parse_alg(3, "R U Q").unwrap_err();
        assert_eq!((e.start, e.end), (4, 5));
        assert!(parse_alg(4, "M").is_err());
        assert!(parse_alg(3, "4Rw").is_err());
        assert!(parse_alg(3, "(R U").is_err());
        assert!(parse_alg(3, "R)").is_err());
    }
}
