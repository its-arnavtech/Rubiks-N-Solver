//! Piece slots of an NxN cube and how each move permutes them, all derived from geometry
//! (docs/05 §5). The big-cube phases in `docs/08` are coordinates over these slots.
//!
//! A *centre slot* is one single-sticker position; a *wing slot* is a two-sticker position
//! off the middle of an edge; a *midge slot* is the middle edge position of an odd cube.

use std::collections::HashMap;

use rg_cube::{Geometry, Move};

/// Centre orbits, distinguished by the magnitudes of the in-face coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CentreKind {
    /// The single fixed centre of an odd face (never moves; excluded from the orbits).
    Fixed,
    /// One in-face coordinate is 0: the "+" centres of a 5x5.
    T,
    /// Both in-face coordinates equal and non-zero: all 4x4 centres, the 5x5 diagonals.
    X,
    /// Neither equal nor zero (6x6 and up).
    Oblique,
}

/// One orbit of single-sticker slots.
#[derive(Clone, Debug)]
pub struct CentreOrbit {
    pub kind: CentreKind,
    /// Facelet index of each slot.
    pub facelets: Vec<usize>,
    /// Per move: `new[i] = old[perm[i]]`.
    pub perms: Vec<Vec<u8>>,
}

/// One orbit of two-sticker slots (wings, or the midges of an odd cube).
#[derive(Clone, Debug)]
pub struct EdgeOrbit {
    /// The two facelets of each slot, in chirality order (see `order_edge_stickers`).
    pub facelets: Vec<[usize; 2]>,
    pub perms: Vec<Vec<u8>>,
    /// Ordered colour pair of each slot in the solved cube, for identifying pieces.
    solved_colours: Vec<[u8; 2]>,
    piece_by_colours: HashMap<[u8; 2], u8>,
}

#[derive(Clone, Debug)]
pub struct Slots {
    pub n: u8,
    pub moves: Vec<Move>,
    pub centres: Vec<CentreOrbit>,
    /// Wing orbits (one for 4x4 and 5x5), innermost first.
    pub wings: Vec<EdgeOrbit>,
    /// Midges of an odd cube, if any.
    pub midges: Option<EdgeOrbit>,
}

fn sticker_count(geom: &Geometry, sticker: usize) -> usize {
    let n = i32::from(geom.n()) - 1;
    geom.cubies()[sticker]
        .iter()
        .filter(|c| c.abs() == n)
        .count()
}

/// Magnitudes of the two in-face coordinates of a centre sticker.
fn in_face(geom: &Geometry, sticker: usize) -> [i32; 2] {
    let n = i32::from(geom.n());
    let p = geom.positions()[sticker];
    let mut v: Vec<i32> = p.iter().filter(|c| c.abs() != n).map(|c| c.abs()).collect();
    v.sort_unstable();
    [v[0], v[1]]
}

/// Index of the axis along an edge cubie (the one that is neither a face normal nor the
/// edge's other side), with the sticker's signed coordinate on it.
fn edge_axis(geom: &Geometry, sticker: usize) -> (usize, i32) {
    let n = i32::from(geom.n());
    let p = geom.positions()[sticker];
    let c = geom.cubies()[sticker];
    (0..3)
        .find(|&k| p[k].abs() != n && c[k].abs() != n - 1)
        .map_or((0, 0), |k| (k, p[k]))
}

/// Distance of an edge sticker from the middle of its edge (0 = midge).
fn edge_offset(geom: &Geometry, sticker: usize) -> i32 {
    edge_axis(geom, sticker).1.abs()
}

/// Outward unit normal of a sticker.
fn normal(geom: &Geometry, sticker: usize) -> [i32; 3] {
    let n = i32::from(geom.n());
    let p = geom.positions()[sticker];
    [0, 1, 2].map(|k| if p[k].abs() == n { p[k].signum() } else { 0 })
}

fn det(a: [i32; 3], b: [i32; 3], c: [i32; 3]) -> i32 {
    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0])
}

/// Order a wing slot's two stickers by chirality, so that the two wings of one edge get
/// opposite orders and therefore distinct ordered colour pairs. Midges keep index order.
fn order_edge_stickers(geom: &Geometry, mut pair: Vec<usize>) -> [usize; 2] {
    pair.sort_unstable();
    let [a, b] = [pair[0], pair[1]];
    let (axis, offset) = edge_axis(geom, a);
    if offset == 0 {
        return [a, b];
    }
    let mut along = [0, 0, 0];
    along[axis] = offset.signum();
    if det(normal(geom, a), normal(geom, b), along) > 0 {
        [a, b]
    } else {
        [b, a]
    }
}

fn kind_of(m: [i32; 2]) -> CentreKind {
    match m {
        [0, 0] => CentreKind::Fixed,
        [0, _] => CentreKind::T,
        [a, b] if a == b => CentreKind::X,
        _ => CentreKind::Oblique,
    }
}

impl Slots {
    pub fn new(n: u8, moves: Vec<Move>) -> Self {
        let geom = Geometry::new(n);
        let perms: Vec<Vec<u16>> = moves.iter().map(|&m| geom.move_permutation(m)).collect();
        let solved: Vec<u8> = (0..geom.sticker_count())
            .map(|i| (i / usize::from(n).pow(2)) as u8)
            .collect();

        // Centre orbits, grouped by in-face coordinate magnitudes.
        let mut by_kind: HashMap<[i32; 2], Vec<usize>> = HashMap::new();
        for s in 0..geom.sticker_count() {
            if sticker_count(&geom, s) == 1 {
                let m = in_face(&geom, s);
                if kind_of(m) != CentreKind::Fixed {
                    by_kind.entry(m).or_default().push(s);
                }
            }
        }
        let mut keys: Vec<[i32; 2]> = by_kind.keys().copied().collect();
        keys.sort_unstable();
        let centres = keys
            .into_iter()
            .map(|k| {
                let facelets = by_kind.remove(&k).expect("key exists");
                let index: HashMap<usize, u8> = facelets
                    .iter()
                    .enumerate()
                    .map(|(i, &f)| (f, i as u8))
                    .collect();
                let perms = perms
                    .iter()
                    .map(|src| {
                        facelets
                            .iter()
                            .map(|&f| index[&usize::from(src[f])])
                            .collect::<Vec<u8>>()
                    })
                    .collect();
                CentreOrbit {
                    kind: kind_of(k),
                    facelets,
                    perms,
                }
            })
            .collect();

        // Edge orbits, grouped by distance from the middle of the edge, then by cubie.
        let mut edges: HashMap<i32, HashMap<[i32; 3], Vec<usize>>> = HashMap::new();
        for s in 0..geom.sticker_count() {
            if sticker_count(&geom, s) == 2 {
                edges
                    .entry(edge_offset(&geom, s))
                    .or_default()
                    .entry(geom.cubies()[s])
                    .or_default()
                    .push(s);
            }
        }
        let build_edge_orbit = |facelets: Vec<[usize; 2]>| -> EdgeOrbit {
            let index: HashMap<[usize; 2], u8> = facelets
                .iter()
                .enumerate()
                .flat_map(|(i, &[a, b])| [([a, b], i as u8), ([b, a], i as u8)])
                .collect();
            let perms = perms
                .iter()
                .map(|src| {
                    facelets
                        .iter()
                        .map(|&[a, b]| index[&[usize::from(src[a]), usize::from(src[b])]])
                        .collect::<Vec<u8>>()
                })
                .collect();
            let solved_colours: Vec<[u8; 2]> = facelets
                .iter()
                .map(|&[a, b]| [solved[a], solved[b]])
                .collect();
            let piece_by_colours: HashMap<[u8; 2], u8> = solved_colours
                .iter()
                .enumerate()
                .map(|(i, &c)| (c, i as u8))
                .collect();
            assert_eq!(
                piece_by_colours.len(),
                facelets.len(),
                "each edge piece must have a unique ordered colour pair"
            );
            EdgeOrbit {
                facelets,
                perms,
                solved_colours,
                piece_by_colours,
            }
        };
        let mut offsets: Vec<i32> = edges.keys().copied().collect();
        offsets.sort_unstable();
        let mut midges = None;
        let mut wings = Vec::new();
        for off in offsets {
            let mut slots: Vec<[usize; 2]> = edges
                .remove(&off)
                .expect("key exists")
                .into_values()
                .map(|v| {
                    debug_assert_eq!(v.len(), 2, "an edge cubie has two stickers");
                    order_edge_stickers(&geom, v)
                })
                .collect();
            slots.sort_unstable();
            let orbit = build_edge_orbit(slots);
            if off == 0 {
                midges = Some(orbit);
            } else {
                wings.push(orbit);
            }
        }
        Self {
            n,
            moves,
            centres,
            wings,
            midges,
        }
    }

    pub fn centre_orbit(&self, kind: CentreKind) -> Option<&CentreOrbit> {
        self.centres.iter().find(|o| o.kind == kind)
    }
}

impl CentreOrbit {
    /// Bit mask of the slots currently showing one of `colours`.
    pub fn mask(&self, facelets: &[u8], colours: [u8; 2]) -> u32 {
        self.facelets.iter().enumerate().fold(0, |m, (i, &f)| {
            if facelets[f] == colours[0] || facelets[f] == colours[1] {
                m | 1 << i
            } else {
                m
            }
        })
    }

    /// Apply a move to a slot mask: bit i afterwards is bit `perm[i]` before.
    #[inline]
    pub fn move_mask(&self, mask: u32, mv: usize) -> u32 {
        self.perms[mv]
            .iter()
            .enumerate()
            .fold(0, |m, (i, &p)| m | (mask >> p & 1) << i)
    }

    pub fn len(&self) -> usize {
        self.facelets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.facelets.is_empty()
    }
}

impl EdgeOrbit {
    /// Which piece sits in each slot, identified by its ordered sticker colours.
    pub fn pieces(&self, facelets: &[u8]) -> Option<Vec<u8>> {
        self.facelets
            .iter()
            .map(|&[a, b]| {
                self.piece_by_colours
                    .get(&[facelets[a], facelets[b]])
                    .copied()
            })
            .collect()
    }

    /// Like [`EdgeOrbit::pieces`], but accepting either sticker order. Only meaningful for
    /// midges, whose colour pairs are unique; the two wings of an edge share a pair.
    pub fn pieces_unordered(&self, facelets: &[u8]) -> Option<Vec<u8>> {
        self.facelets
            .iter()
            .map(|&[a, b]| {
                let (x, y) = (facelets[a], facelets[b]);
                self.piece_by_colours
                    .get(&[x, y])
                    .or_else(|| self.piece_by_colours.get(&[y, x]))
                    .copied()
            })
            .collect()
    }

    pub fn solved_colours(&self) -> &[[u8; 2]] {
        &self.solved_colours
    }

    pub fn len(&self) -> usize {
        self.facelets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.facelets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rg_cube::Cube;

    fn slots(n: u8) -> Slots {
        Slots::new(
            n,
            rg_cube::generator_moves(n)
                .iter()
                .flat_map(|g| (1..=3).map(move |t| Move { turns: t, ..*g }))
                .collect(),
        )
    }

    #[test]
    fn orbit_shapes_match_the_pieces_of_each_cube() {
        let four = slots(4);
        assert_eq!(four.centres.len(), 1);
        assert_eq!(four.centres[0].kind, CentreKind::X);
        assert_eq!(four.centres[0].len(), 24);
        assert_eq!(four.wings.len(), 1);
        assert_eq!(four.wings[0].len(), 24);
        assert!(four.midges.is_none());

        let five = slots(5);
        assert_eq!(five.centres.len(), 2);
        assert_eq!(five.centre_orbit(CentreKind::X).unwrap().len(), 24);
        assert_eq!(five.centre_orbit(CentreKind::T).unwrap().len(), 24);
        assert_eq!(five.wings[0].len(), 24);
        assert_eq!(five.midges.as_ref().unwrap().len(), 12);

        let three = slots(3);
        assert!(three.centres.is_empty()); // only fixed centres
        assert_eq!(three.midges.as_ref().unwrap().len(), 12);
    }

    #[test]
    fn permutations_are_permutations() {
        for n in [4u8, 5] {
            let s = slots(n);
            for orbit in &s.centres {
                for p in &orbit.perms {
                    let mut sorted = p.clone();
                    sorted.sort_unstable();
                    assert_eq!(sorted, (0..orbit.len() as u8).collect::<Vec<_>>());
                }
            }
            for orbit in s.wings.iter().chain(s.midges.iter()) {
                for p in &orbit.perms {
                    let mut sorted = p.clone();
                    sorted.sort_unstable();
                    assert_eq!(sorted, (0..orbit.len() as u8).collect::<Vec<_>>());
                }
            }
        }
    }

    #[test]
    fn slot_permutations_track_the_facelet_model() {
        for n in [4u8, 5] {
            let s = slots(n);
            let mut cube = Cube::new(n).unwrap();
            let centre = &s.centres[0];
            let wings = &s.wings[0];
            // Track a centre-colour mask and the wing pieces through a random sequence.
            let mut mask = centre.mask(cube.facelets(), [1, 4]);
            let mut pieces = wings.pieces(cube.facelets()).unwrap();
            for (step, m) in rg_cube::random_move_scramble(n, 40, 11)
                .into_iter()
                .enumerate()
            {
                let mv = s
                    .moves
                    .iter()
                    .position(|x| *x == m)
                    .expect("move is in the set");
                cube.apply_move(m);
                mask = centre.move_mask(mask, mv);
                pieces = centre_free_apply(&wings.perms[mv], &pieces);
                assert_eq!(
                    mask,
                    centre.mask(cube.facelets(), [1, 4]),
                    "n={n} step {step}"
                );
                assert_eq!(
                    pieces,
                    wings.pieces(cube.facelets()).unwrap(),
                    "n={n} step {step}"
                );
            }
        }
    }

    fn centre_free_apply(perm: &[u8], state: &[u8]) -> Vec<u8> {
        perm.iter().map(|&p| state[usize::from(p)]).collect()
    }
}
