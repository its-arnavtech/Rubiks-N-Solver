//! Code phases (ARCHITECTURE §7): phase 0 wing parity, phase 1a fixed-center frame, phase 1b
//! corner parity.

use std::collections::{HashMap, VecDeque};

use nx_sim::identity::piece_parity;
use nx_sim::{Axis, Cube, Move, Orbit, OrbitKind, SlotMap};

/// Phase 0: one quarter turn of slice layer `p` for each wing orbit with odd parity. That
/// 4-cycles four wings of orbit `p` and touches no other wing orbit, corner or middle edge.
pub fn wing_parity(cube: &Cube, orbits: &[Orbit]) -> Vec<(u32, Move)> {
    let n = cube.n();
    orbits
        .iter()
        .filter(|o| o.kind == OrbitKind::Wing)
        .filter_map(|o| {
            let content = SlotMap::for_orbit(n, o).extract(cube).ok()?;
            piece_parity(OrbitKind::Wing, &content).then_some((o.id, Move::new(Axis::X, o.a, 1)))
        })
        .collect()
}

/// Phase 1a (odd N): shortest sequence of MID turns bringing the six face centers home,
/// by breadth-first search over the 24 frame states. The frame is N-independent, so the
/// search runs on a 3×3 and binds MID afterwards.
pub fn core_frame(cube: &Cube, frame: &Orbit) -> Option<Vec<Move>> {
    let n = cube.n();
    let centers = SlotMap::for_orbit(n, frame).extract(cube).ok()?;
    let small_map = SlotMap::new(3, OrbitKind::FixedCenter, 0, 0);
    let mut start = Cube::solved(3);
    small_map.insert(&mut start, &centers);
    let key = |c: &Cube| small_map.extract(c).expect("colors");
    let moves: Vec<Move> = Axis::ALL
        .iter()
        .flat_map(|&a| (1..=3).map(move |t| Move::new(a, 1, t)))
        .collect();
    let mut prev: HashMap<Vec<u8>, Option<(Vec<u8>, Move)>> = HashMap::new();
    let mut queue = VecDeque::from([start.clone()]);
    prev.insert(key(&start), None);
    let home: Vec<u8> = (0..6).collect();
    while let Some(c) = queue.pop_front() {
        let k = key(&c);
        if k == home {
            let mut path = Vec::new();
            let mut cur = k;
            while let Some(Some((p, m))) = prev.get(&cur) {
                path.push(Move::new(m.axis, (n - 1) / 2, m.turns));
                cur = p.clone();
            }
            path.reverse();
            return Some(path);
        }
        for &m in &moves {
            let mut next = c.clone();
            next.apply(m);
            let nk = key(&next);
            if let std::collections::hash_map::Entry::Vacant(e) = prev.entry(nk) {
                e.insert(Some((k.clone(), m)));
                queue.push_back(next);
            }
        }
    }
    None
}

/// Phase 1b: an outer quarter turn if the corner permutation is odd (it is even on wings, and
/// on odd N it also flips middle-edge parity, keeping the 3×3 law).
pub fn corner_parity(cube: &Cube, corner: &Orbit) -> Option<Move> {
    let content = SlotMap::for_orbit(cube.n(), corner).extract(cube).ok()?;
    piece_parity(OrbitKind::Corner, &content).then_some(Move::new(Axis::Y, 0, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nx_sim::identity::{frame_index, frame_rotations};
    use nx_sim::{orbits, random_state};

    #[test]
    fn frame_bfs_reaches_home_from_every_rotation() {
        for n in [3, 5, 9] {
            let os = orbits(n);
            let frame = os
                .iter()
                .find(|o| o.kind == OrbitKind::FixedCenter)
                .unwrap();
            let map = SlotMap::for_orbit(n, frame);
            for rot in frame_rotations() {
                let mut c = Cube::solved(n);
                map.insert(&mut c, rot);
                let path = core_frame(&c, frame).unwrap();
                assert!(path.len() <= 3);
                c.apply_all(&path);
                assert_eq!(frame_index(&map.extract(&c).unwrap()), Some(0));
            }
        }
    }

    #[test]
    fn parity_phases_make_parities_even() {
        for seed in 0..10 {
            for n in [6, 7] {
                let os = orbits(n);
                let mut c = random_state(n, seed);
                for (_, m) in wing_parity(&c, &os) {
                    c.apply(m);
                }
                assert!(wing_parity(&c, &os).is_empty());
                let corner = os.iter().find(|o| o.kind == OrbitKind::Corner).unwrap();
                if let Some(m) = corner_parity(&c, corner) {
                    c.apply(m);
                }
                assert!(corner_parity(&c, corner).is_none());
                assert!(
                    wing_parity(&c, &os).is_empty(),
                    "outer turn kept wing parity"
                );
            }
        }
    }
}
