//! M1.3: `rg-cube` oracle and property tests for the move engine.

use nx_sim::geometry::dlb_corner;
use nx_sim::rng::{below, between, rng};
use nx_sim::{Axis, Cube, LabeledCube, Move, allowed_moves};

fn rg_move(mv: Move) -> rg_cube::Move {
    let axis = rg_cube::Axis::from_index(mv.axis as u8).unwrap();
    rg_cube::Move::new(axis, 1 << mv.layer, mv.turns)
}

fn random_move(r: &mut nx_sim::rng::ChaCha8Rng, n: u32) -> Move {
    let axis = Axis::ALL[below(r, 3) as usize];
    let layer = below(r, u64::from(n - 1)) as u32;
    Move::new(axis, layer, between(r, 1, 3) as u8)
}

fn random_labeled(n: u32, len: usize, seed: u64) -> LabeledCube {
    let mut r = rng(seed);
    let mut c = LabeledCube::solved(n);
    for _ in 0..len {
        c.apply(random_move(&mut r, n));
    }
    c
}

#[test]
fn every_move_matches_rg_cube() {
    for n in 2..=7u32 {
        let geom = rg_cube::Geometry::new(n as u8);
        // Every allowed move, plus the never-allowed layer N−1, from the solved labeling.
        for axis in Axis::ALL {
            for layer in 0..n {
                for turns in 1..=3 {
                    let mv = Move::new(axis, layer, turns);
                    let mut c = LabeledCube::solved(n);
                    c.apply(mv);
                    let want: Vec<u32> = geom
                        .move_permutation(rg_move(mv))
                        .into_iter()
                        .map(u32::from)
                        .collect();
                    assert_eq!(c.facelets(), want, "n={n} {mv}");
                }
            }
        }
    }
}

#[test]
fn random_sequences_match_rg_cube_colors() {
    for n in 2..=7u32 {
        let mut r = rng(u64::from(n));
        let moves: Vec<Move> = (0..200).map(|_| random_move(&mut r, n)).collect();
        let mut ours = Cube::solved(n);
        ours.apply_all(&moves);
        let mut oracle = rg_cube::Cube::new(n as u8).unwrap();
        for &m in &moves {
            oracle.apply_move(rg_move(m));
        }
        assert_eq!(ours.facelets(), oracle.facelets(), "n={n}");
    }
}

#[test]
fn property_inverse_order_commute_and_fixed_corner() {
    let mut r = rng(2026);
    for case in 0..60 {
        let n = between(&mut r, 2, 64) as u32;
        let start = random_labeled(n, 40, case);
        for _ in 0..8 {
            let m = random_move(&mut r, n);
            // move ∘ inverse = id
            let mut c = start.clone();
            c.apply_all(&[m, m.inverse()]);
            assert_eq!(c, start, "n={n} {m} inverse");
            // four quarter turns = id
            let q = Move::new(m.axis, m.layer, 1);
            let mut c = start.clone();
            c.apply_all(&[q; 4]);
            assert_eq!(c, start, "n={n} {q} order 4");
            // same-axis moves commute
            let other = Move::new(m.axis, below(&mut r, u64::from(n - 1)) as u32, 1);
            let (mut a, mut b) = (start.clone(), start.clone());
            a.apply_all(&[m, other]);
            b.apply_all(&[other, m]);
            assert_eq!(a, b, "n={n} {m} {other} commute");
        }
        // Allowed moves never move the D-L-B corner.
        let dlb = dlb_corner(n);
        let mut c = LabeledCube::solved(n);
        for m in allowed_moves(n) {
            c.apply(m);
        }
        c.apply_all(&random_labeled_moves(&mut r, n, 100));
        assert!(
            dlb.iter().all(|&i| c.facelets()[i] as usize == i),
            "n={n} DLB moved"
        );
    }
}

fn random_labeled_moves(r: &mut nx_sim::rng::ChaCha8Rng, n: u32, len: usize) -> Vec<Move> {
    (0..len).map(|_| random_move(r, n)).collect()
}

#[test]
fn large_cube_scramble_and_inverse_round_trip() {
    for n in [100u32, 257, 400] {
        let mut r = rng(u64::from(n));
        let moves = random_labeled_moves(&mut r, n, 300);
        let mut c = Cube::solved(n);
        c.apply_all(&moves);
        assert!(!c.is_solved());
        c.apply_all(&nx_sim::invert(&moves));
        assert!(c.is_solved(), "n={n}");
    }
}
