//! Whole-cube rotations and conjugation (docs/02 §9, docs/05 §7).
//!
//! Solvers work in a normalized frame (fixed centres on odd cubes, a fixed DBL corner on
//! the 2x2). A solution found there is mapped back to the user's frame by conjugating each
//! move with the normalizing rotation, so no whole-cube rotations appear in the output.

use std::collections::HashSet;

use crate::geometry::Geometry;
use crate::moves::{Axis, Move, invert};

/// Gather permutation of a move sequence applied left to right: `new[j] = old[p[j]]`.
pub fn sequence_permutation(geom: &Geometry, moves: &[Move]) -> Vec<u16> {
    let mut acc: Vec<u16> = (0..geom.sticker_count() as u16).collect();
    for &m in moves {
        let p = geom.move_permutation(m);
        acc = p.iter().map(|&j| acc[usize::from(j)]).collect();
    }
    acc
}

/// The 24 whole-cube rotations as move sequences, identity first (BFS over x and y).
pub fn whole_cube_rotations(n: u8) -> Vec<Vec<Move>> {
    let geom = Geometry::new(n);
    let full = ((1u16 << n) - 1) as u8;
    let gens = [Move::new(Axis::X, full, 1), Move::new(Axis::Y, full, 1)];
    let mut seen = HashSet::from([sequence_permutation(&geom, &[])]);
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for seq in &frontier {
            for g in gens {
                let mut s: Vec<Move> = seq.clone();
                s.push(g);
                if seen.insert(sequence_permutation(&geom, &s)) {
                    out.push(s.clone());
                    next.push(s);
                }
            }
        }
        frontier = next;
    }
    debug_assert_eq!(out.len(), 24);
    out
}

/// The single layer turn equal to `rot · m · rot⁻¹` (apply `rot`, then `m`, then undo `rot`).
pub fn conjugate(geom: &Geometry, rot: &[Move], m: Move) -> Move {
    let seq: Vec<Move> = rot.iter().copied().chain([m]).chain(invert(rot)).collect();
    let target = sequence_permutation(geom, &seq);
    let n = geom.n();
    let width = m.layers.count_ones();
    for axis in Axis::ALL {
        for layers in 1..(1u16 << n) {
            let layers = layers as u8;
            if layers.count_ones() != width {
                continue;
            }
            for turns in 1..=3 {
                let c = Move::new(axis, layers, turns);
                if geom.move_permutation(c) == target {
                    return c;
                }
            }
        }
    }
    unreachable!("a rotation conjugates a layer turn to a layer turn")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cube, parse_alg};

    #[test]
    fn there_are_24_rotations() {
        for n in 2..=5 {
            assert_eq!(whole_cube_rotations(n).len(), 24);
        }
    }

    #[test]
    fn conjugation_matches_the_sequence() {
        for n in 2..=5u8 {
            let geom = Geometry::new(n);
            let alg = if n % 2 == 1 {
                "R U' Fw2 M D' Lw"
            } else {
                "R U' Fw2 D' Lw 2B"
            };
            let moves = parse_alg(n, alg).unwrap();
            for rot in whole_cube_rotations(n) {
                for &m in &moves {
                    let c = conjugate(&geom, &rot, m);
                    let mut a = Cube::new(n).unwrap();
                    let mut b = Cube::new(n).unwrap();
                    a.apply_alg("F R2 U'").unwrap();
                    b.apply_alg("F R2 U'").unwrap();
                    a.apply_move(c);
                    b.apply_moves(&rot);
                    b.apply_move(m);
                    b.apply_moves(&invert(&rot));
                    assert_eq!(a.facelets(), b.facelets(), "n={n} rot={rot:?} m={m:?}");
                }
            }
        }
    }
}
