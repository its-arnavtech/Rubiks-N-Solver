//! Piece identity from colors, insertion, parity and the fixed-center frame
//! (CONVENTIONS §4–5, ARCHITECTURE §5).

use std::fmt;
use std::sync::OnceLock;

use crate::cube::{Cube, LabeledCube};
use crate::geometry::{D, U};
use crate::moves::{Axis, Move};
use crate::orbits::OrbitKind;
use crate::rng::{below, rng};
use crate::slots::{CORNER_SLOTS, MID_EDGE_SLOTS, SlotMap, WING_EDGES};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// The colors in a slot don't form any piece of this orbit kind.
    UnknownPiece {
        kind: OrbitKind,
        slot: usize,
        colors: Vec<u8>,
    },
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPiece { kind, slot, colors } => {
                write!(
                    f,
                    "{kind} slot {slot} has colors {colors:?}, which is no piece"
                )
            }
        }
    }
}

impl std::error::Error for IdentityError {}

/// Wing lookup `(color on f1 side, color on f2 side, slot's s bit) → home slot`
/// (CONVENTIONS §5), built by exhaustive placement on a labeled N=4 cube.
struct WingTable([[[u8; 2]; 6]; 6]);

const NONE: u8 = u8::MAX;

fn wing_table() -> &'static WingTable {
    static TABLE: OnceLock<WingTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let n = 4;
        let map = SlotMap::new(n, OrbitKind::Wing, 1, 0);
        let mut table = [[[NONE; 2]; 6]; 6];
        let mut seen = [[false; 24]; 24];
        let mut remaining = 24 * 24;
        let mut cube = LabeledCube::solved(n);
        let mut r = rng(0);
        let moves = crate::moves::allowed_moves(n);
        for _ in 0..1_000_000 {
            let colors = cube.to_colors();
            let homes = map.extract_labeled(&cube);
            for (slot, &home) in homes.iter().enumerate() {
                let st = map.slot(slot);
                let x = colors.facelets()[st[0] as usize] as usize;
                let y = colors.facelets()[st[1] as usize] as usize;
                let cell = &mut table[x][y][slot % 2];
                assert!(
                    *cell == NONE || *cell == home,
                    "wing colors do not determine identity"
                );
                *cell = home;
                if !seen[usize::from(home)][slot] {
                    seen[usize::from(home)][slot] = true;
                    remaining -= 1;
                }
            }
            if remaining == 0 {
                return WingTable(table);
            }
            cube.apply(moves[below(&mut r, moves.len() as u64) as usize]);
        }
        panic!("wing placement did not reach every (piece, slot) pair");
    })
}

/// Home slot of the wing showing colors `(x, y)` (f1 side, f2 side) in a slot with bit `s`.
pub fn wing_home(x: u8, y: u8, s: usize) -> Option<u8> {
    let v = *wing_table()
        .0
        .get(usize::from(x))?
        .get(usize::from(y))?
        .get(s)?;
    (v != NONE).then_some(v)
}

/// Colors `(f1 side, f2 side)` wing `home` shows in `slot`.
pub fn wing_colors(home: u8, slot: usize) -> (u8, u8) {
    let (f1, f2) = WING_EDGES[usize::from(home) / 2];
    let (f1, f2) = (f1 as u8, f2 as u8);
    if wing_home(f1, f2, slot % 2) == Some(home) {
        (f1, f2)
    } else {
        debug_assert_eq!(wing_home(f2, f1, slot % 2), Some(home));
        (f2, f1)
    }
}

impl SlotMap {
    /// Slot contents from colors (CONVENTIONS §4–5). Same values as `extract_labeled` for any
    /// reachable state.
    pub fn extract(&self, cube: &Cube) -> Result<Vec<u8>, IdentityError> {
        let f = cube.facelets();
        let colors =
            |i: usize| -> Vec<u8> { self.slot(i).iter().map(|&s| f[s as usize]).collect() };
        let unknown = |slot: usize| IdentityError::UnknownPiece {
            kind: self.kind,
            slot,
            colors: colors(slot),
        };
        (0..self.slot_count())
            .map(|i| {
                let c = colors(i);
                match self.kind {
                    OrbitKind::Corner => {
                        let ori = c
                            .iter()
                            .position(|&x| usize::from(x) == U || usize::from(x) == D);
                        let ori = ori.ok_or_else(|| unknown(i))?;
                        let rotated = [0, 1, 2].map(|k| usize::from(c[(k + ori) % 3]));
                        let piece = CORNER_SLOTS.iter().position(|p| *p == rotated);
                        Ok((piece.ok_or_else(|| unknown(i))? * 3 + ori) as u8)
                    }
                    OrbitKind::MidEdge => {
                        let (a, b) = (usize::from(c[0]), usize::from(c[1]));
                        if let Some(p) = MID_EDGE_SLOTS.iter().position(|p| *p == [a, b]) {
                            Ok((2 * p) as u8)
                        } else if let Some(p) = MID_EDGE_SLOTS.iter().position(|p| *p == [b, a]) {
                            Ok((2 * p + 1) as u8)
                        } else {
                            Err(unknown(i))
                        }
                    }
                    OrbitKind::Wing => wing_home(c[0], c[1], i % 2).ok_or_else(|| unknown(i)),
                    _ => Ok(c[0]),
                }
            })
            .collect()
    }

    /// Write slot contents into a color cube (inverse of [`SlotMap::extract`]).
    pub fn insert(&self, cube: &mut Cube, content: &[u8]) {
        assert_eq!(content.len(), self.slot_count(), "content length");
        let mut writes: Vec<(u32, u8)> = Vec::with_capacity(self.stickers.len());
        for (i, &x) in content.iter().enumerate() {
            let slot = self.slot(i);
            match self.kind {
                OrbitKind::Corner => {
                    let (piece, ori) = (usize::from(x / 3), usize::from(x % 3));
                    for k in 0..3 {
                        writes.push((slot[(k + ori) % 3], CORNER_SLOTS[piece][k] as u8));
                    }
                }
                OrbitKind::MidEdge => {
                    let (piece, ori) = (usize::from(x / 2), usize::from(x % 2));
                    for k in 0..2 {
                        writes.push((slot[(k + ori) % 2], MID_EDGE_SLOTS[piece][k] as u8));
                    }
                }
                OrbitKind::Wing => {
                    let (a, b) = wing_colors(x, i);
                    writes.extend([(slot[0], a), (slot[1], b)]);
                }
                _ => writes.push((slot[0], x)),
            }
        }
        cube.set_stickers(&writes);
    }
}

/// Permutation parity (true = odd) of a permutation of `0..len`.
pub fn permutation_parity(perm: &[u8]) -> bool {
    let mut seen = vec![false; perm.len()];
    let mut odd = false;
    for start in 0..perm.len() {
        if seen[start] {
            continue;
        }
        let mut len = 0;
        let mut i = start;
        while !seen[i] {
            seen[i] = true;
            i = usize::from(perm[i]);
            len += 1;
        }
        odd ^= len % 2 == 0;
    }
    odd
}

/// Parity of the piece permutation in extracted contents (corner, middle edge, wing).
pub fn piece_parity(kind: OrbitKind, content: &[u8]) -> bool {
    let w = match kind {
        OrbitKind::Corner => 3,
        OrbitKind::MidEdge => 2,
        OrbitKind::Wing => 1,
        _ => panic!("{kind} has no piece parity"),
    };
    let perm: Vec<u8> = content.iter().map(|&x| x / w).collect();
    permutation_parity(&perm)
}

/// The 24 fixed-center frame states (odd N) as the colors of the six face centers, in
/// breadth-first order from the solved frame over MID quarter turns (index 0 = solved).
pub fn frame_rotations() -> &'static [[u8; 6]; 24] {
    static ROTATIONS: OnceLock<[[u8; 6]; 24]> = OnceLock::new();
    ROTATIONS.get_or_init(|| {
        let map = SlotMap::new(3, OrbitKind::FixedCenter, 0, 0);
        let read = |c: &Cube| -> [u8; 6] {
            let v = map.extract(c).expect("centers are colors");
            v.try_into().expect("six centers")
        };
        let mut states = vec![Cube::solved(3)];
        let mut out = vec![read(&states[0])];
        let mut i = 0;
        while i < states.len() {
            for axis in Axis::ALL {
                let mut c = states[i].clone();
                c.apply(Move::new(axis, 1, 1));
                let key = read(&c);
                if !out.contains(&key) {
                    out.push(key);
                    states.push(c);
                }
            }
            i += 1;
        }
        out.try_into().expect("24 frame rotations")
    })
}

/// Index of a frame state (six center colors), if it is a rotation.
pub fn frame_index(centers: &[u8]) -> Option<usize> {
    frame_rotations().iter().position(|r| r[..] == *centers)
}

/// Permutation parity of a frame state.
pub fn frame_parity(index: usize) -> bool {
    permutation_parity(&frame_rotations()[index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbits::orbits;
    use crate::scramble_moves;

    #[test]
    fn color_identity_equals_labeled_identity() {
        for n in 2..=20 {
            let maps: Vec<SlotMap> = orbits(n).iter().map(|o| SlotMap::for_orbit(n, o)).collect();
            for seed in 0..3 {
                let mut labeled = LabeledCube::solved(n);
                labeled.apply_all(&scramble_moves(n, 150, seed));
                let colors = labeled.to_colors();
                for map in &maps {
                    assert_eq!(
                        map.extract(&colors).unwrap(),
                        map.extract_labeled(&labeled),
                        "n={n} {}",
                        map.kind
                    );
                }
            }
        }
    }

    #[test]
    fn insert_inverts_extract() {
        for n in [2, 3, 4, 5, 8, 11] {
            let (scrambled, _) = Cube::scramble(n, 200, 4);
            let mut rebuilt = Cube::solved(n);
            for o in orbits(n) {
                let map = SlotMap::for_orbit(n, &o);
                map.insert(&mut rebuilt, &map.extract(&scrambled).unwrap());
            }
            assert_eq!(rebuilt, scrambled, "n={n}");
        }
    }

    #[test]
    fn unknown_pieces_are_reported() {
        let mut c = Cube::solved(3);
        let map = SlotMap::new(3, OrbitKind::Corner, 0, 0);
        c.set_stickers(&[(map.slot(0)[1], 0)]); // URF shows U twice
        assert!(matches!(
            map.extract(&c),
            Err(IdentityError::UnknownPiece { slot: 0, .. })
        ));
    }

    #[test]
    fn frames_and_parity_law() {
        let rots = frame_rotations();
        assert_eq!(rots[0], [0, 1, 2, 3, 4, 5]);
        assert_eq!(frame_index(&[0, 1, 2, 3, 4, 5]), Some(0));
        assert_eq!(frame_index(&[1, 0, 2, 3, 4, 5]), None);
        for n in [3, 5, 7, 9] {
            let os = orbits(n);
            let map = |k| SlotMap::for_orbit(n, os.iter().find(|o| o.kind == k).unwrap());
            let (corner, edge, frame) = (
                map(OrbitKind::Corner),
                map(OrbitKind::MidEdge),
                map(OrbitKind::FixedCenter),
            );
            for seed in 0..20 {
                let (c, _) = Cube::scramble(n, 1 + seed as usize, seed);
                let fi = frame_index(&frame.extract(&c).unwrap()).expect("frame is a rotation");
                let law = piece_parity(OrbitKind::Corner, &corner.extract(&c).unwrap())
                    ^ piece_parity(OrbitKind::MidEdge, &edge.extract(&c).unwrap())
                    ^ frame_parity(fi);
                assert!(!law, "n={n} seed={seed}");
            }
        }
    }

    #[test]
    fn parity_of_small_permutations() {
        assert!(!permutation_parity(&[0, 1, 2]));
        assert!(permutation_parity(&[1, 0, 2]));
        assert!(!permutation_parity(&[1, 2, 0]));
        assert!(permutation_parity(&[1, 2, 3, 0]));
    }
}
