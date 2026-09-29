//! Random move scrambles (M1.4). These are weak for big N; evaluation uses `random_state`
//! (CONVENTIONS §6) instead.

use crate::cube::Cube;
use crate::moves::{Axis, Move};
use crate::rng::{below, rng};

/// `len` moves drawn uniformly from the allowed moves, re-drawing any move on the same
/// `(axis, layer)` as the previous one (it would merge with it). Seeded with `ChaCha8Rng`.
pub fn scramble_moves(n: u32, len: usize, seed: u64) -> Vec<Move> {
    assert!(n >= 2, "cube size must be at least 2, got {n}");
    let layers = u64::from(n - 1);
    let mut r = rng(seed);
    let mut out: Vec<Move> = Vec::with_capacity(len);
    while out.len() < len {
        let v = below(&mut r, 9 * layers);
        let axis = Axis::ALL[(v / (3 * layers)) as usize];
        let layer = ((v / 3) % layers) as u32;
        let mv = Move::new(axis, layer, (v % 3) as u8 + 1);
        if out
            .last()
            .is_some_and(|p| p.axis == axis && p.layer == layer)
        {
            continue;
        }
        out.push(mv);
    }
    out
}

impl Cube {
    /// Solved cube with [`scramble_moves`] applied; returns the cube and the moves.
    pub fn scramble(n: u32, len: usize, seed: u64) -> (Cube, Vec<Move>) {
        let moves = scramble_moves(n, len, seed);
        let mut cube = Cube::solved(n);
        cube.apply_all(&moves);
        (cube, moves)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moves::format_moves;

    #[test]
    fn deterministic_allowed_and_never_repeats_a_layer() {
        for n in [2, 3, 4, 10, 101] {
            let a = scramble_moves(n, 500, 11);
            assert_eq!(a, scramble_moves(n, 500, 11));
            assert_ne!(a, scramble_moves(n, 500, 12));
            assert!(a.iter().all(|m| m.is_allowed(n)));
            assert!(
                a.windows(2)
                    .all(|w| (w[0].axis, w[0].layer) != (w[1].axis, w[1].layer))
            );
        }
    }

    #[test]
    fn covers_every_allowed_move() {
        let n = 4;
        let ms = scramble_moves(n, 2000, 0);
        for m in crate::moves::allowed_moves(n) {
            assert!(ms.contains(&m), "{m} never drawn");
        }
    }

    #[test]
    fn scramble_and_inverse_solve() {
        let (mut c, moves) = Cube::scramble(7, 100, 5);
        assert!(!c.is_solved());
        c.apply_all(&crate::moves::invert(&moves));
        assert!(c.is_solved());
    }

    /// Pins the stream: a change here means seeds no longer reproduce old scrambles.
    #[test]
    fn stream_is_pinned() {
        assert_eq!(format_moves(&scramble_moves(5, 8, 0)), PINNED_5_8_0);
    }

    const PINNED_5_8_0: &str = "F 2R 4F2 U 2F' 4R 3R' R'";
}
