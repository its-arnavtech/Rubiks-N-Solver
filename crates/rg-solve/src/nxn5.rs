//! 5x5 reduction (docs/08 §6). EXPERIMENTAL: phases 1, 2a and 2b are fast; phase 3 (both
//! centre orbits plus tredge pairing) is correct but can take minutes (ADR-015).
//!
//! 5x5 reduction (docs/08 §6). The fixed centres define the frame, so every goal is a
//! single configuration: no colour-pair choice, no mirrored schemes. Two centre orbits
//! (x- and t-centres) and three-piece edges (a midge between two wings) replace the 4x4's
//! single centre orbit and two-piece edges.

use crate::macros5::{Discovery, MacroOp};
use rg_cube::{Cube, Geometry, Move};
use rg_graph::bfs::DistanceTable;
use rg_graph::combinatorics::{is_odd, mask_rank, mask_unrank};
use rg_graph::ida::{Flow, SearchCtl, SearchSpace, search_exact};

use crate::nxn::{BitPerm, preserving, slot_components, subset_table};
use crate::slots::{CentreKind, CentreOrbit, Slots};

/// The 5x5 move set: six faces plus six two-layer wide turns, each × {1, 2, 3} (36),
/// plus the six inner-slice half turns. Those are products of two block turns
/// (`2U2 = Uw2 U2`), so they add no new positions; they only let searches that must
/// restore centres go shallower. `expand_obtm` rewrites them for the output.
pub fn moves_5x5() -> Vec<Move> {
    rg_cube::parse_alg(
        5,
        "U U2 U' D D2 D' L L2 L' R R2 R' F F2 F' B B2 B' \
         Uw Uw2 Uw' Dw Dw2 Dw' Lw Lw2 Lw' Rw Rw2 Rw' Fw Fw2 Fw' Bw Bw2 Bw' \
         2U2 2D2 2L2 2R2 2F2 2B2",
    )
    .expect("valid notation")
}

/// Rewrite inner-slice turns as outer block turns (`2U2` → `Uw2 U2`), so solution lengths
/// are honest in the outer-block-turn metric.
pub fn expand_obtm(moves: &[Move]) -> Vec<Move> {
    let mut out = Vec::with_capacity(moves.len());
    for &m in moves {
        // Anything that already turns an outer layer is an outer block turn.
        if m.layers & (1 | 16) != 0 {
            out.push(m);
            continue;
        }
        // A single inner layer next to face layer f: (f + inner) then f undone.
        let face = if m.layers & 0b00010 != 0 { 1 } else { 16 };
        out.push(Move {
            layers: m.layers | face,
            ..m
        });
        out.push(Move {
            layers: face,
            turns: (4 - m.turns % 4) % 4,
            ..m
        });
    }
    out
}

const RL: [u8; 2] = [1, 4];
const UD: [u8; 2] = [0, 3];
const PER_FACE: usize = 25;

/// Which wings (class, tredge) are attached to their midges.
type Attached = [[u8; 12]; 2];
/// Triple constraints (class, triple) and fully attached tredge pairs.
type Constraints = (Vec<(usize, usize)>, Vec<(usize, usize)>);
/// Phase-3 heuristic weight, in halves (3 = ×1.5): see `Phase3Space::weight_halves`.
/// Phase 1 and phase 2b trade a few moves for a far smaller search (3 = 1.5x).
const PHASE1_WEIGHT_HALVES: u16 = 3;
const PHASE3_WEIGHT_HALVES: u16 = 2;
const PHASE2B_WEIGHT_HALVES: u16 = 3;

/// Tredges split into triples; pairing stages work through `BASE` in order.
/// Macro discovery: sequences up to this length are enumerated, so macros are up to twice
/// as long; they may move at most `MACRO_MAX_SUPPORT` wings, and are conjugated by setup
/// sequences of up to `MACRO_SETUP_DEPTH` centre-fixing turns.
const MACRO_HALF_DEPTH: usize = 4;
const MACRO_MAX_SUPPORT: usize = 6;
const MACRO_SETUP_DEPTH: usize = 3;
/// How many operators the pairing walk may apply before giving up.
const MACRO_STEP_LIMIT: usize = 400;

const TRIPLES: [[u8; 3]; 16] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [9, 10, 11],
    [0, 4, 8],
    [1, 5, 9],
    [2, 6, 10],
    [3, 7, 11],
    [0, 5, 10],
    [1, 6, 11],
    [2, 7, 9],
    [3, 4, 8],
    [0, 7, 11],
    [1, 4, 10],
    [2, 5, 8],
    [3, 6, 9],
];

fn rank8_tables() -> ([u8; 256], [u8; 70]) {
    let mut rank8 = [0u8; 256];
    let mut unrank8 = [0u8; 70];
    for mask in 0u32..256 {
        if mask.count_ones() == 4 {
            let r = mask_rank(mask) as u8;
            rank8[mask as usize] = r;
            unrank8[usize::from(r)] = mask as u8;
        }
    }
    (rank8, unrank8)
}

/// Restrict a gather permutation to the slots in `keep`, renumbered in slot order.
fn restrict(perm: &[u8], keep: u32) -> Vec<u8> {
    let kept: Vec<usize> = (0..perm.len()).filter(|&i| keep >> i & 1 == 1).collect();
    let mut index = vec![u8::MAX; perm.len()];
    for (i, &s) in kept.iter().enumerate() {
        index[s] = i as u8;
    }
    kept.iter().map(|&s| index[usize::from(perm[s])]).collect()
}

fn compress(mask: u32, keep: &[usize]) -> u32 {
    keep.iter()
        .enumerate()
        .fold(0, |m, (i, &s)| m | (mask >> s & 1) << i)
}

/// Phase-3 view of one centre orbit: its slots grouped by axis, packed one byte per axis.
struct AxisCentres {
    axis_slots: [[u8; 8]; 3],
    /// Colour of each axis' first face: a slot's bit is set when it shows that colour.
    reference: [u8; 3],
    bits: Vec<BitPerm>,
    /// Phase 3: distances to "solved" under the phase-3 moves.
    table: DistanceTable,
    /// The solved state, packed.
    goal: u32,
    /// Phase 2b: distances, under the larger phase-2b move set, to any arrangement that
    /// phase 3 can still solve (that set is only part of the graph).
    to_solvable: DistanceTable,
}

impl AxisCentres {
    /// `bit_moves` must keep every centre on its axis (bits are built for them);
    /// `gens` ⊆ `bit_moves` are the phase-3 moves, and `back` ⊆ `bit_moves` the phase-2b
    /// moves that walk back into the part of the graph phase 3 can solve.
    fn new(
        orbit: &CentreOrbit,
        bit_moves: &[usize],
        gens: &[usize],
        back: &[usize],
        rank8: &[u8; 256],
        unrank8: &[u8; 70],
    ) -> Self {
        let axis_of_face = |f: usize| match f {
            1 | 4 => 0,
            0 | 3 => 1,
            _ => 2,
        };
        let mut axis_slots = [[0u8; 8]; 3];
        let mut filled = [0usize; 3];
        for (s, &f) in orbit.facelets.iter().enumerate() {
            let a = axis_of_face(f / PER_FACE);
            axis_slots[a][filled[a]] = s as u8;
            filled[a] += 1;
        }
        assert_eq!(filled, [8, 8, 8]);
        let reference: [u8; 3] = std::array::from_fn(|a| {
            (orbit.facelets[usize::from(axis_slots[a][0])] / PER_FACE) as u8
        });
        let mut local = vec![(0usize, 0usize); orbit.len()];
        for (a, slots) in axis_slots.iter().enumerate() {
            for (i, &s) in slots.iter().enumerate() {
                local[usize::from(s)] = (a, i);
            }
        }
        let bits: Vec<BitPerm> = orbit
            .perms
            .iter()
            .enumerate()
            .map(|(m, p)| {
                if !bit_moves.contains(&m) {
                    return BitPerm::new(&(0..24).collect::<Vec<u8>>());
                }
                let mut gather = vec![0u8; 24];
                for (a, slots) in axis_slots.iter().enumerate() {
                    for (i, &s) in slots.iter().enumerate() {
                        let (a2, j) = local[usize::from(p[usize::from(s)])];
                        assert_eq!(a, a2, "phase-3 moves keep centres on their axis");
                        gather[a * 8 + i] = (a2 * 8 + j) as u8;
                    }
                }
                BitPerm::new(&gather)
            })
            .collect();
        let face_of = |s: u8| orbit.facelets[usize::from(s)] / PER_FACE;
        let goal: u32 = (0..3).fold(0, |acc, a| {
            let first = face_of(axis_slots[a][0]);
            let byte = axis_slots[a].iter().enumerate().fold(0u32, |m, (i, &s)| {
                if face_of(s) == first { m | 1 << i } else { m }
            });
            acc | byte << (8 * a)
        });
        let index = |packed: u32| {
            (usize::from(rank8[(packed & 0xFF) as usize]) * 70
                + usize::from(rank8[(packed >> 8 & 0xFF) as usize]))
                * 70
                + usize::from(rank8[(packed >> 16 & 0xFF) as usize])
        };
        let pack = |v: u32| {
            let (a, b, c) = (v as usize / 4900, v as usize / 70 % 70, v as usize % 70);
            u32::from(unrank8[a]) | u32::from(unrank8[b]) << 8 | u32::from(unrank8[c]) << 16
        };
        let table = DistanceTable::build_queued(343_000, &[index(goal) as u32], |v, push| {
            let packed = pack(v);
            for &m in gens {
                push(index(bits[m].apply(packed)) as u32);
            }
        });
        let solvable: Vec<u32> = (0..343_000u32)
            .filter(|&v| table.get(v as usize) != rg_graph::bfs::UNREACHED)
            .collect();
        let to_solvable = DistanceTable::build_queued(343_000, &solvable, |v, push| {
            let packed = pack(v);
            for &m in back {
                push(index(bits[m].apply(packed)) as u32);
            }
        });
        Self {
            axis_slots,
            reference,
            bits,
            table,
            goal,
            to_solvable,
        }
    }

    fn goal_byte(&self, axis: usize) -> u32 {
        self.goal >> (8 * axis) & 0xFF
    }

    /// `None` when some axis does not hold exactly its two colours, four each.
    fn packed(&self, orbit: &CentreOrbit, facelets: &[u8]) -> Option<u32> {
        let mut acc = 0u32;
        for a in 0..3 {
            let byte = self.axis_slots[a]
                .iter()
                .enumerate()
                .fold(0u32, |m, (i, &s)| {
                    if facelets[orbit.facelets[usize::from(s)]] == self.reference[a] {
                        m | 1 << i
                    } else {
                        m
                    }
                });
            if byte.count_ones() != 4 {
                return None;
            }
            acc |= byte << (8 * a);
        }
        Some(acc)
    }
}

/// A short sequence of centre-fixing turns, used to conjugate a macro so that its effect
/// lands on other wings. Conjugation preserves what matters: centres restored, midges home.
struct Setup {
    moves: Vec<u8>,
    undo: Vec<u8>,
    /// Where the wing in each slot ends up.
    dest: [u8; 24],
}

fn compose24(a: &[u8; 24], step: &[u8; 24]) -> [u8; 24] {
    std::array::from_fn(|i| step[usize::from(a[i])])
}

pub struct Reduction5 {
    pub moves: Vec<Move>,
    slots: Slots,
    shape: Vec<(u8, u8)>,
    x_bits: Vec<BitPerm>,
    t_bits: Vec<BitPerm>,
    wing_bits: Vec<BitPerm>,
    midge_face_bits: Vec<BitPerm>,
    /// How each move permutes the 24 midge stickers (position and orientation).
    midge_sticker_perms: Vec<Vec<u8>>,
    rank8: [u8; 256],
    // Phase 1
    p1_x: DistanceTable,
    p1_t: DistanceTable,
    // Phase 2
    gens_a: Vec<u8>,
    kept_x: Vec<usize>,
    kept_t: Vec<usize>,
    x16_bits: Vec<BitPerm>,
    t16_bits: Vec<BitPerm>,
    /// Phase 2a: U/D x-centres onto U/D, with the wing parity bit.
    p2a_x: DistanceTable,
    p2a_t: DistanceTable,
    wing_parity_flip: Vec<bool>,
    // Phase 2b
    gens_c: Vec<u8>,
    wing_class: Vec<u8>,
    p2b_wings: DistanceTable,
    /// Which of the 24 midge stickers are orientation-class 0 (the reference stickers).
    midge_goal: u32,
    p2b_midges: DistanceTable,
    // Phase 3
    gens_b: Vec<u8>,
    x_axes: AxisCentres,
    t_axes: AxisCentres,
    /// Edge (0..12) of each wing slot and of each midge slot.
    wing_edge: [u8; 24],
    midge_edge: [u8; 12],
    /// Per class (0, 1 = wings; 2 = midges) and move: edge-position mapping.
    steps: [Vec<[u8; 12]>; 3],
    /// Pairing databases for class-0 and class-1 wings with their midges (+ parity bit).
    p3_pair: [DistanceTable; 2],
    /// Joint centre tables per axis: (x byte, t byte) of that axis, 70 × 70 each.
    axis_joint: [DistanceTable; 3],
    /// Two tredges, both wing classes and their midges, plus both parity bits: the one
    /// table that sees class-0 and class-1 wings together (144³ × 4 entries).
    joint_pair: DistanceTable,
    /// Phase-3 moves without the inner-slice turns (for the centres stage).
    gens_b_plain: Vec<u8>,
    pair_parity_flip: [Vec<bool>; 2],
    /// Phase 3b: the discovered macros and the setups they are conjugated by.
    macros: Vec<MacroOp>,
    setups: Vec<Setup>,
}

impl Reduction5 {
    pub fn new() -> Self {
        let moves = moves_5x5();
        let slots = Slots::new(5, moves.clone());
        let geom = Geometry::new(5);
        let shape = moves.iter().map(|m| (m.axis as u8, m.layers)).collect();
        let solved = Cube::new(5).expect("5x5").facelets().to_vec();
        let xs = slots
            .centre_orbit(CentreKind::X)
            .expect("x-centres")
            .clone();
        let ts = slots
            .centre_orbit(CentreKind::T)
            .expect("t-centres")
            .clone();
        let wings = slots.wings[0].clone();
        let midges = slots.midges.clone().expect("midges");
        let (rank8, unrank8) = rank8_tables();

        let x_bits: Vec<BitPerm> = xs.perms.iter().map(|p| BitPerm::new(p)).collect();
        let t_bits: Vec<BitPerm> = ts.perms.iter().map(|p| BitPerm::new(p)).collect();
        let wing_bits: Vec<BitPerm> = wings.perms.iter().map(|p| BitPerm::new(p)).collect();

        // ---- Phase 1: R/L x- and t-centres onto the R/L axis ----
        let x_rl = xs.mask(&solved, RL);
        let t_rl = ts.mask(&solved, RL);
        let block: Vec<usize> = (0..36).collect();
        let p1_x = subset_table(&x_bits, &block, &[mask_rank(x_rl)], 24, 8);
        let p1_t = subset_table(&t_bits, &block, &[mask_rank(t_rl)], 24, 8);

        // ---- Phase 2 ----
        // Phases 1, 2a and 2b search the 36 block turns; the slice turns join in phase 3.
        let gens_a_idx: Vec<usize> = preserving(&x_bits, &preserving(&t_bits, &block, t_rl), x_rl);
        let gens_a: Vec<u8> = gens_a_idx.iter().map(|&m| m as u8).collect();
        let keep_x = x_rl ^ 0xFF_FFFF;
        let keep_t = t_rl ^ 0xFF_FFFF;
        let kept_x: Vec<usize> = (0..24).filter(|i| keep_x >> i & 1 == 1).collect();
        let kept_t: Vec<usize> = (0..24).filter(|i| keep_t >> i & 1 == 1).collect();
        let restricted = |orbit: &CentreOrbit, keep: u32| -> Vec<BitPerm> {
            orbit
                .perms
                .iter()
                .enumerate()
                .map(|(m, p)| {
                    if gens_a_idx.contains(&m) {
                        BitPerm::new(&restrict(p, keep))
                    } else {
                        BitPerm::new(&(0..16).collect::<Vec<u8>>())
                    }
                })
                .collect()
        };
        let x16_bits = restricted(&xs, keep_x);
        let t16_bits = restricted(&ts, keep_t);
        let x_ud16 = compress(xs.mask(&solved, UD), &kept_x);
        let t_ud16 = compress(ts.mask(&solved, UD), &kept_t);
        // Wing permutation parity never changes under the phase-2b/3 moves, and a paired
        // cube needs it even, so it rides along in phase 2a's x-centre table.
        let wing_parity_flip: Vec<bool> = wings.perms.iter().map(|p| is_odd(p)).collect();
        let p2a_x = DistanceTable::build_queued(12_870 * 2, &[mask_rank(x_ud16) * 2], |v, push| {
            let (mask, parity) = (mask_unrank(v / 2, 16, 8), v & 1);
            for &m in &gens_a_idx {
                let p = parity ^ u32::from(wing_parity_flip[m]);
                push(mask_rank(x16_bits[m].apply(mask)) * 2 + p);
            }
        });
        let p2a_t = subset_table(&t16_bits, &gens_a_idx, &[mask_rank(t_ud16)], 16, 8);

        // Phase 2b's moves keep every centre on its axis; phase 3's also drop the F/B
        // quarter turns (a design choice: it gives wings and midges an orientation).
        let x_ud = xs.mask(&solved, UD);
        let t_ud = ts.mask(&solved, UD);
        let gens_c_idx: Vec<usize> =
            preserving(&x_bits, &preserving(&t_bits, &gens_a_idx, t_ud), x_ud);
        let gens_c: Vec<u8> = gens_c_idx.iter().map(|&m| m as u8).collect();
        let gens_b_idx: Vec<usize> = gens_c_idx
            .iter()
            .copied()
            .filter(|&m| moves[m].axis != rg_cube::Axis::Z || moves[m].turns == 2)
            .chain(36..moves.len())
            .collect();
        let gens_b: Vec<u8> = gens_b_idx.iter().map(|&m| m as u8).collect();

        // Wing classes and midge-sticker classes: orbits under the phase-3 group.
        let wing_class = slot_components(&wings.perms, &gens_b_idx, 24);
        let class_goal = (0..24).fold(0u32, |m, i| if wing_class[i] == 0 { m | 1 << i } else { m });
        let k = class_goal.count_ones() as usize;
        let p2b_wings = subset_table(&wing_bits, &gens_c_idx, &[mask_rank(class_goal)], 24, k);

        // Midge orientation, as which of the 24 midge stickers hold a "reference" sticker.
        let midge_facelets: Vec<usize> =
            midges.facelets.iter().flat_map(|&[a, b]| [a, b]).collect();
        let midge_index: std::collections::HashMap<usize, u8> = midge_facelets
            .iter()
            .enumerate()
            .map(|(i, &f)| (f, i as u8))
            .collect();
        let facelet_perms: Vec<Vec<u8>> = moves
            .iter()
            .map(|&m| {
                let src = geom.move_permutation(m);
                midge_facelets
                    .iter()
                    .map(|&f| midge_index[&usize::from(src[f])])
                    .collect()
            })
            .collect();
        let midge_face_bits: Vec<BitPerm> = facelet_perms.iter().map(|p| BitPerm::new(p)).collect();
        let midge_sticker_perms = facelet_perms.clone();
        let sticker_class = slot_components(&facelet_perms, &gens_b_idx, 24);
        let midge_goal = (0..24).fold(
            0u32,
            |m, i| if sticker_class[i] == 0 { m | 1 << i } else { m },
        );
        assert!(
            (0..12).all(|s| (midge_goal >> (2 * s) & 1) != (midge_goal >> (2 * s + 1) & 1)),
            "each midge has one sticker in each orientation class"
        );
        let km = midge_goal.count_ones() as usize;
        let p2b_midges = subset_table(
            &midge_face_bits,
            &gens_c_idx,
            &[mask_rank(midge_goal)],
            24,
            km,
        );

        // ---- Phase 3 ----
        // Bits are needed for every move phase 2b or phase 3 may make; the inner-slice
        // half turns move centres between the two faces of one axis, so they qualify.
        let bit_moves: Vec<usize> = gens_c_idx
            .iter()
            .copied()
            .chain(36..moves.len())
            .collect();
        let x_axes = AxisCentres::new(&xs, &bit_moves, &gens_b_idx, &gens_c_idx, &rank8, &unrank8);
        let t_axes = AxisCentres::new(&ts, &bit_moves, &gens_b_idx, &gens_c_idx, &rank8, &unrank8);

        // Edges: wings and midges grouped by the cube edge they sit on.
        let edge_key = |f: usize| {
            let c = geom.cubies()[f];
            std::array::from_fn::<i32, 3, _>(|k| if c[k].abs() == 4 { c[k] } else { 0 })
        };
        let mut keys: Vec<[i32; 3]> = midges.facelets.iter().map(|&[a, _]| edge_key(a)).collect();
        keys.dedup();
        assert_eq!(keys.len(), 12);
        let midge_edge: [u8; 12] = std::array::from_fn(|i| i as u8);
        let wing_edge: [u8; 24] = std::array::from_fn(|s| {
            let key = edge_key(wings.facelets[s][0]);
            keys.iter()
                .position(|k| *k == key)
                .expect("every wing is on an edge") as u8
        });
        let mut slot_of = [[u8::MAX; 12]; 2];
        for s in 0..24 {
            let (c, e) = (usize::from(wing_class[s]), usize::from(wing_edge[s]));
            assert_eq!(
                slot_of[c][e],
                u8::MAX,
                "each edge has one wing slot per class"
            );
            slot_of[c][e] = s as u8;
        }
        let inverse = |p: &[u8]| {
            let mut inv = vec![0u8; p.len()];
            for (i, &q) in p.iter().enumerate() {
                inv[usize::from(q)] = i as u8;
            }
            inv
        };
        let wing_step = |c: usize| -> Vec<[u8; 12]> {
            wings
                .perms
                .iter()
                .map(|p| {
                    let inv = inverse(p);
                    std::array::from_fn(|e| wing_edge[usize::from(inv[usize::from(slot_of[c][e])])])
                })
                .collect()
        };
        let midge_step: Vec<[u8; 12]> = midges
            .perms
            .iter()
            .map(|p| {
                let inv = inverse(p);
                std::array::from_fn(|e| midge_edge[usize::from(inv[e])])
            })
            .collect();
        let steps = [wing_step(0), wing_step(1), midge_step];

        // Pairing: a triple of class-c wings against their midges, plus the parity bit
        // "wing permutation parity equals midge permutation parity", which pairing forces.
        let pair_parity_flip: [Vec<bool>; 2] = std::array::from_fn(|c| {
            (0..moves.len())
                .map(|m| is_odd(&steps[c][m]) != is_odd(&steps[2][m]))
                .collect()
        });
        let mut goals = Vec::new();
        for a in 0..12u32 {
            for b in 0..12u32 {
                for c in 0..12u32 {
                    if a != b && b != c && a != c {
                        let p = (a * 12 + b) * 12 + c;
                        goals.push((p * 1728 + p) * 2);
                    }
                }
            }
        }
        let p3_pair: [DistanceTable; 2] = std::array::from_fn(|c| {
            let (ws, ms, flip) = (&steps[c], &steps[2], &pair_parity_flip[c]);
            DistanceTable::build_queued(1728 * 1728 * 2, &goals, |v, push| {
                let digits = |x: u32| [x / 144 % 12, x / 12 % 12, x % 12];
                let (positions, parity) = (v / 2, v & 1);
                let (dw, dm) = (digits(positions / 1728), digits(positions % 1728));
                for &m in &gens_b_idx {
                    let f = |step: &[u8; 12], d: [u32; 3]| {
                        d.iter()
                            .fold(0u32, |acc, &x| acc * 12 + u32::from(step[x as usize]))
                    };
                    let p = parity ^ u32::from(flip[m]);
                    push((f(&ws[m], dw) * 1728 + f(&ms[m], dm)) * 2 + p);
                }
            })
        });

        // Joint (x, t) centre state of one axis: a move changes an axis' centres using only
        // that axis' centres, so this projection is a quotient graph too.
        let axis_joint: [DistanceTable; 3] = std::array::from_fn(|a| {
            let shift = 8 * a;
            let goal_x = x_axes.goal_byte(a);
            let goal_t = t_axes.goal_byte(a);
            let idx =
                |x: u32, t: u32| u32::from(rank8[x as usize]) * 70 + u32::from(rank8[t as usize]);
            DistanceTable::build_queued(4900, &[idx(goal_x, goal_t)], |v, push| {
                let (x, t) = (
                    u32::from(unrank8[(v / 70) as usize]),
                    u32::from(unrank8[(v % 70) as usize]),
                );
                for &m in &gens_b_idx {
                    let x2 = x_axes.bits[m].apply(x << shift) >> shift & 0xFF;
                    let t2 = t_axes.bits[m].apply(t << shift) >> shift & 0xFF;
                    push(idx(x2, t2));
                }
            })
        });
        let joint_pair = {
            let mut goals = Vec::new();
            for x in 0..12u32 {
                for y in 0..12u32 {
                    if x != y {
                        let pp = x * 12 + y;
                        goals.push(((pp * 144 + pp) * 144 + pp) * 4);
                    }
                }
            }
            let (f0, f1) = (&pair_parity_flip[0], &pair_parity_flip[1]);
            DistanceTable::build_queued(144 * 144 * 144 * 4, &goals, |v, push| {
                let (p, par) = (v / 4, v % 4);
                let (a, b, c) = (p / 20_736, p / 144 % 144, p % 144);
                for &m in &gens_b_idx {
                    let f = |step: &[u8; 12], x: u32| {
                        u32::from(step[(x / 12) as usize]) * 12 + u32::from(step[(x % 12) as usize])
                    };
                    let (a2, b2, c2) = (f(&steps[0][m], a), f(&steps[1][m], b), f(&steps[2][m], c));
                    let par2 = par ^ u32::from(f0[m]) ^ (u32::from(f1[m]) << 1);
                    push(((a2 * 144 + b2) * 144 + c2) * 4 + par2);
                }
            })
        };
        let gens_b_plain: Vec<u8> = gens_b.iter().copied().filter(|&m| m < 36).collect();

        let mut built = Self {
            moves,
            slots,
            shape,
            x_bits,
            t_bits,
            wing_bits,
            midge_face_bits,
            midge_sticker_perms,
            rank8,
            p1_x,
            p1_t,
            gens_a,
            kept_x,
            kept_t,
            x16_bits,
            t16_bits,
            p2a_x,
            p2a_t,
            wing_parity_flip,
            gens_c,
            wing_class,
            p2b_wings,
            midge_goal,
            p2b_midges,
            gens_b,
            x_axes,
            t_axes,
            wing_edge,
            midge_edge,
            steps,
            p3_pair,
            axis_joint,
            joint_pair,
            gens_b_plain,
            pair_parity_flip,
            macros: Vec::new(),
            setups: Vec::new(),
        };
        built.macros = built.macro_library(MACRO_HALF_DEPTH, MACRO_MAX_SUPPORT);
        built.setups = built.build_setups(MACRO_SETUP_DEPTH);
        built
    }

    fn xs(&self) -> &CentreOrbit {
        self.slots.centre_orbit(CentreKind::X).expect("x-centres")
    }

    fn ts(&self) -> &CentreOrbit {
        self.slots.centre_orbit(CentreKind::T).expect("t-centres")
    }

    fn canonical(&self, prev: Option<u8>, m: u8) -> bool {
        match prev {
            None => true,
            Some(p) => {
                let (a, b) = (self.shape[usize::from(p)], self.shape[usize::from(m)]);
                a.0 != b.0 || a.1 < b.1
            }
        }
    }

    pub fn report(&self) -> String {
        format!(
            "phase 1: x {} / t {} vertices, depths {} / {}\n\
             gens A: {}, gens B: {}\n\
             phase 2a: x+parity {} / t {} vertices; 2b: gens {}, wings {} (depth {}), midges {} (depth {}), centres-to-solvable depths {} / {}\n\
             phase 3: centres x {} / t {} (depths {} / {}), pairing {} / {} (depths {} / {})",
            self.p1_x.reached(),
            self.p1_t.reached(),
            self.p1_x.max_depth(),
            self.p1_t.max_depth(),
            self.gens_a.len(),
            self.gens_b.len(),
            self.p2a_x.reached(),
            self.p2a_t.reached(),
            self.gens_c.len(),
            self.p2b_wings.reached(),
            self.p2b_wings.max_depth(),
            self.p2b_midges.reached(),
            self.p2b_midges.max_depth(),
            self.x_axes.to_solvable.max_depth(),
            self.t_axes.to_solvable.max_depth(),
            self.x_axes.table.reached(),
            self.t_axes.table.reached(),
            self.x_axes.table.max_depth(),
            self.t_axes.table.max_depth(),
            self.p3_pair[0].reached(),
            self.p3_pair[1].reached(),
            self.p3_pair[0].max_depth(),
            self.p3_pair[1].max_depth(),
        )
    }

    /// Phase 1: every R/L x- and t-centre onto the R/L faces.
    pub fn phase1(&self, facelets: &[u8], time_up: &dyn Fn() -> bool) -> Option<Vec<usize>> {
        let node = (self.xs().mask(facelets, RL), self.ts().mask(facelets, RL));
        let space = Phase1Space {
            r: self,
            weight_halves: PHASE1_WEIGHT_HALVES,
        };
        let ctl = SearchCtl::with_deadline(time_up);
        rg_graph::ida::ida_star_any_depth(&space, node, 30, None, &ctl)
            .map(|p| p.into_iter().map(usize::from).collect())
    }

    /// Where the midges' reference stickers are (a mask over the 24 midge stickers).
    fn midge_mask(&self, facelets: &[u8]) -> Option<u32> {
        let orbit = self.slots.midges.as_ref()?;
        let homes = orbit.pieces_unordered(facelets)?;
        let mut mask = 0u32;
        for (slot, &home) in homes.iter().enumerate() {
            let home = usize::from(home);
            let [ha, hb] = orbit.facelets[home];
            let reference_home = if self.midge_goal >> (2 * home) & 1 == 1 {
                ha
            } else {
                hb
            };
            let colour = (reference_home / PER_FACE) as u8;
            let [a, _] = orbit.facelets[slot];
            mask |= 1
                << if facelets[a] == colour {
                    2 * slot
                } else {
                    2 * slot + 1
                };
        }
        Some(mask)
    }

    /// Which wing slots hold a class-0 wing.
    fn wing_mask(&self, facelets: &[u8]) -> Option<u32> {
        let pieces = self.slots.wings[0].pieces(facelets)?;
        Some((0..24).fold(0u32, |m, i| {
            if self.wing_class[usize::from(pieces[i])] == 0 {
                m | 1 << i
            } else {
                m
            }
        }))
    }

    fn phase2b_node(&self, facelets: &[u8]) -> Option<Phase2bNode> {
        Some(Phase2bNode {
            x: self.x_axes.packed(self.xs(), facelets)?,
            t: self.t_axes.packed(self.ts(), facelets)?,
            wings: self.wing_mask(facelets)?,
            midges: self.midge_mask(facelets)?,
        })
    }

    /// Phase 2a: U/D x- and t-centres onto the U/D axis with an even wing permutation, only
    /// accepting cubes that phase 2b can take on (connectivity lifting, docs/08 §3).
    pub fn phase2a(&self, facelets: &[u8], time_up: &dyn Fn() -> bool) -> Option<Vec<usize>> {
        let pieces = self.slots.wings[0].pieces(facelets)?;
        let start = Phase2aNode {
            x16: compress(self.xs().mask(facelets, UD), &self.kept_x),
            t16: compress(self.ts().mask(facelets, UD), &self.kept_t),
            parity: u32::from(is_odd(&pieces)),
        };
        let space = Phase2aSpace { r: self };
        let ctl = SearchCtl::with_deadline(time_up);
        let mut probe = Cube::new(5).ok()?;
        let mut buffer = Vec::new();
        for depth in space.h(start)..=18 {
            let mut found = None;
            search_exact(&space, start, depth, None, &mut buffer, &ctl, &mut |p| {
                probe.set_facelets(facelets).expect("same size");
                for &m in p {
                    probe.apply_move(self.moves[usize::from(m)]);
                }
                let ok = self
                    .phase2b_node(probe.facelets())
                    .is_some_and(|n| {
                        (Phase2bSpace {
                            r: self,
                            weight_halves: 2,
                        })
                        .h(n) != rg_graph::bfs::UNREACHED
                    });
                if ok {
                    found = Some(p.to_vec());
                    Flow::Stop
                } else {
                    Flow::Continue
                }
            });
            if let Some(p) = found {
                return Some(p.into_iter().map(usize::from).collect());
            }
            if ctl.aborted() {
                return None;
            }
            if std::env::var_os("RG_TRACE").is_some() {
                eprintln!("  phase 2a: depth {depth} exhausted");
            }
        }
        None
    }

    /// Phase 2b: wings into their classes, midges oriented, and both centre orbits into
    /// arrangements phase 3 can solve, all with moves that keep centres on their axes.
    pub fn phase2b(&self, facelets: &[u8], time_up: &dyn Fn() -> bool) -> Option<Vec<usize>> {
        let start = self.phase2b_node(facelets)?;
        let space = Phase2bSpace {
            r: self,
            weight_halves: PHASE2B_WEIGHT_HALVES,
        };
        let ctl = SearchCtl::with_deadline(time_up);
        rg_graph::ida::ida_star_any_depth(&space, start, 40, None, &ctl)
            .map(|p| p.into_iter().map(usize::from).collect())
    }

    fn centre_index(&self, packed: u32) -> usize {
        let r = |b: u32| usize::from(self.rank8[(b & 0xFF) as usize]);
        (r(packed) * 70 + r(packed >> 8)) * 70 + r(packed >> 16)
    }

    /// Edge positions of every class-0 wing, class-1 wing and midge (by home edge).
    fn edge_positions(&self, facelets: &[u8]) -> Option<[[u8; 12]; 3]> {
        let wings = self.slots.wings[0].pieces(facelets)?;
        let midges = self.slots.midges.as_ref()?.pieces_unordered(facelets)?;
        let mut pos = [[0u8; 12]; 3];
        for (slot, &piece) in wings.iter().enumerate() {
            let class = usize::from(self.wing_class[usize::from(piece)]);
            pos[class][usize::from(self.wing_edge[usize::from(piece)])] = self.wing_edge[slot];
        }
        for (slot, &piece) in midges.iter().enumerate() {
            pos[2][usize::from(self.midge_edge[usize::from(piece)])] = self.midge_edge[slot];
        }
        Some(pos)
    }

    fn parity_bits(pos: &[[u8; 12]; 3]) -> [u32; 2] {
        let m = is_odd(&pos[2]);
        [
            u32::from(is_odd(&pos[0]) != m),
            u32::from(is_odd(&pos[1]) != m),
        ]
    }

    /// Phase 3: solve both centre orbits, then attach every wing to its midge, one wing
    /// class of one triple at a time, keeping the centres and all earlier attachments.
    pub fn phase3(
        &self,
        facelets: &[u8],
        now_ms: &dyn Fn() -> f64,
        deadline_ms: f64,
    ) -> Option<Vec<usize>> {
        let trace = std::env::var_os("RG_TRACE").is_some();
        let mut cube = Cube::new(5).ok()?;
        cube.set_facelets(facelets).ok()?;
        let mut path = Vec::new();
        let apply = |cube: &mut Cube, found: Vec<usize>, path: &mut Vec<usize>| {
            for m in found {
                cube.apply_move(self.moves[m]);
                path.push(m);
            }
        };

        // Both centre orbits first, by search; then the wings, by chaining macros. Every
        // macro restores the centres and leaves the midges alone, so the pairing walk can
        // never undo the centre stage.
        let t0 = now_ms();
        let solved = [[0u8; 12]; 2];
        let found = self.phase3_attempt(cube.facelets(), &solved, [true, true], &|| {
            now_ms() > deadline_ms
        })?;
        if trace {
            eprintln!("  centres: {} moves, {:.0} ms", found.len(), now_ms() - t0);
        }
        apply(&mut cube, found, &mut path);

        let t0 = now_ms();
        let found = self.pair_with_macros(cube.facelets(), &|| now_ms() > deadline_ms)?;
        if trace {
            eprintln!("  pairing: {} moves, {:.0} ms", found.len(), now_ms() - t0);
        }
        apply(&mut cube, found, &mut path);
        Some(path)
    }

    fn phase3_node(&self, facelets: &[u8]) -> Option<Phase3Node> {
        let pos = self.edge_positions(facelets)?;
        Some(Phase3Node {
            x: self.x_axes.packed(self.xs(), facelets)?,
            t: self.t_axes.packed(self.ts(), facelets)?,
            pos,
            parity: Self::parity_bits(&pos),
        })
    }

    /// Pairing constraints for the attached wings: every triple (of the sixteen) whose
    /// tredges are all attached, per wing class, and every pair of fully attached tredges.
    fn phase3_constraints(attached: &Attached) -> Constraints {
        let mut triples = Vec::new();
        for (c, row) in attached.iter().enumerate() {
            for (t, triple) in TRIPLES.iter().enumerate() {
                if triple.iter().all(|&d| row[usize::from(d)] == 1) {
                    triples.push((c, t));
                }
            }
        }
        // Pairs of tredges whose wings of both classes must be attached.
        let full: Vec<usize> = (0..12)
            .filter(|&d| attached[0][d] == 1 && attached[1][d] == 1)
            .collect();
        let mut pairs = Vec::new();
        for (i, &d1) in full.iter().enumerate() {
            for &d2 in &full[i + 1..] {
                pairs.push((d1, d2));
            }
        }
        (triples, pairs)
    }

    /// One phase-3 search: centres solved and `attached` wings attached.
    fn phase3_attempt(
        &self,
        facelets: &[u8],
        attached: &[[u8; 12]; 2],
        centres: [bool; 2],
        time_up: &dyn Fn() -> bool,
    ) -> Option<Vec<usize>> {
        let (triples, pairs) = Self::phase3_constraints(attached);
        let nothing_attached = attached.iter().flatten().all(|&a| a == 0);
        let space = Phase3Space {
            r: self,
            // Slice turns only help when centres must be restored after pairing moves.
            moves: if nothing_attached {
                &self.gens_b_plain
            } else {
                &self.gens_b
            },
            triples: &triples,
            pairs: &pairs,
            centres,
            weight_halves: PHASE3_WEIGHT_HALVES,
        };
        let ctl = SearchCtl::with_deadline(time_up);
        rg_graph::ida::ida_star_any_depth(&space, self.phase3_node(facelets)?, 60, None, &ctl)
            .map(|p| p.into_iter().map(usize::from).collect())
    }

    /// Where the piece in each slot ends up, per move, for one orbit's gather permutations.
    fn dest_maps<const N: usize>(perms: &[Vec<u8>]) -> Vec<[u8; N]> {
        perms
            .iter()
            .map(|p| {
                let mut dest = [0u8; N];
                for (i, &src) in p.iter().enumerate() {
                    dest[usize::from(src)] = i as u8;
                }
                dest
            })
            .collect()
    }

    /// Index of each move's inverse.
    fn inverse_moves(&self) -> Vec<u8> {
        self.moves
            .iter()
            .map(|m| {
                let want = Move {
                    turns: (4 - m.turns) % 4,
                    ..*m
                };
                self.moves
                    .iter()
                    .position(|&o| o == want)
                    .expect("every move's inverse is a move") as u8
            })
            .collect()
    }

    /// The macro operators for the final phase (see `macros5`).
    pub fn macro_library(&self, half_depth: usize, max_support: usize) -> Vec<MacroOp> {
        let solved = Cube::new(5).expect("5x5").facelets().to_vec();
        let wing_step: Vec<[u8; 24]> = Self::dest_maps(&self.slots.wings[0].perms);
        let midge_step: Vec<[u8; 24]> = Self::dest_maps(&self.midge_sticker_perms);
        let centre_step = |(x, t): (u32, u32), mv: u8| {
            let m = usize::from(mv);
            (self.x_axes.bits[m].apply(x), self.t_axes.bits[m].apply(t))
        };
        let inverse = self.inverse_moves();
        let canonical = |prev: Option<u8>, m: u8| self.canonical(prev, m);
        Discovery {
            gens: &self.gens_b,
            inverse: &inverse,
            wing_step: &wing_step,
            midge_step: &midge_step,
            centre_step: &centre_step,
            centre_start: (
                self.x_axes.packed(self.xs(), &solved).expect("solved"),
                self.t_axes.packed(self.ts(), &solved).expect("solved"),
            ),
            canonical: &canonical,
            half_depth,
            max_support,
        }
        .run(512)
    }

    /// Moves that leave solved centres solved, so they can set a macro up and undo it
    /// afterwards without disturbing anything. Found by testing, not assumed.
    fn centre_fixing_moves(&self) -> Vec<u8> {
        let solved = Cube::new(5).expect("5x5").facelets().to_vec();
        let (x0, t0) = (
            self.x_axes.packed(self.xs(), &solved).expect("solved"),
            self.t_axes.packed(self.ts(), &solved).expect("solved"),
        );
        self.gens_b
            .iter()
            .copied()
            .filter(|&m| {
                let m = usize::from(m);
                self.x_axes.bits[m].apply(x0) == x0 && self.t_axes.bits[m].apply(t0) == t0
            })
            .collect()
    }

    /// Short sequences of centre-fixing moves, used as setups. A macro conjugated by one
    /// of these is another macro, which is how a few hundred discovered sequences reach
    /// every part of the puzzle.
    fn build_setups(&self, depth: usize) -> Vec<Setup> {
        let gens = self.centre_fixing_moves();
        let dest_maps: Vec<[u8; 24]> = Self::dest_maps(&self.slots.wings[0].perms);
        let inverse = self.inverse_moves();
        let identity: [u8; 24] = std::array::from_fn(|i| i as u8);
        let mut setups = vec![(Vec::new(), identity)];
        let mut frontier = vec![(Vec::<u8>::new(), identity)];
        for _ in 0..depth {
            let mut next = Vec::new();
            for (moves, dest) in &frontier {
                for &m in &gens {
                    if !self.canonical(moves.last().copied(), m) {
                        continue;
                    }
                    let mut moves = moves.clone();
                    moves.push(m);
                    next.push((moves, compose24(dest, &dest_maps[usize::from(m)])));
                }
            }
            setups.extend(next.iter().cloned());
            frontier = next;
        }
        setups
            .into_iter()
            .map(|(moves, dest)| Setup {
                undo: moves
                    .iter()
                    .rev()
                    .map(|&m| inverse[usize::from(m)])
                    .collect(),
                moves,
                dest,
            })
            .collect()
    }

    /// Phase 3b: attach every wing to its midge by chaining macro operators (ADR-016).
    ///
    /// Every operator restores the centres and leaves every midge where it is, so nothing
    /// it does can undo the centre stage or detach a wing outside its own support. That
    /// makes the walk simple: take the operator that attaches the most wings per move, and
    /// when none does, take the shortest one that reaches an arrangement not seen before.
    fn pair_with_macros(&self, facelets: &[u8], time_up: &dyn Fn() -> bool) -> Option<Vec<usize>> {
        let mut pieces: [u8; 24] = self.slots.wings[0].pieces(facelets)?.try_into().ok()?;
        let pos = self.edge_positions(facelets)?;
        // Midges never move, so every wing's target edge is fixed for the whole walk.
        let target: [u8; 24] = std::array::from_fn(|p| pos[2][usize::from(self.wing_edge[p])]);
        let attached =
            |slot: usize, piece: u8| self.wing_edge[slot] == target[usize::from(piece)];
        let left = |pieces: &[u8; 24]| (0..24).filter(|&s| !attached(s, pieces[s])).count();
        let mut seen: std::collections::HashSet<[u8; 24]> = std::collections::HashSet::new();
        seen.insert(pieces);
        let mut path = Vec::new();
        for _ in 0..MACRO_STEP_LIMIT {
            if left(&pieces) == 0 {
                return Some(path);
            }
            if time_up() {
                return None;
            }
            // Best (most wings attached per move) over every macro under every setup,
            // composed on the fly: a conjugated macro moves setup(s) wherever the macro
            // moves s, so only its support needs looking at.
            // (rank, resulting arrangement), where rank orders by wings attached, then
            // by length, then by index so the choice is deterministic.
            type Choice = ((i64, usize, usize, usize), [u8; 24]);
            let mut best: Option<Choice> = None;
            for (si, setup) in self.setups.iter().enumerate() {
                for (mi, op) in self.macros.iter().enumerate() {
                    let mut gain = 0i32;
                    for &u in &op.support {
                        let from = setup.dest[usize::from(u)];
                        let to = setup.dest[usize::from(op.wing[usize::from(u)])];
                        let piece = pieces[usize::from(from)];
                        gain += i32::from(attached(usize::from(to), piece))
                            - i32::from(attached(usize::from(from), piece));
                    }
                    let length = setup.moves.len() * 2 + op.moves.len();
                    // Most wings attached first, shortest among equals.
                    let key = (i64::from(-gain), length, si, mi);
                    if best.as_ref().is_some_and(|(b, _)| *b <= key) {
                        continue;
                    }
                    let mut moved = pieces;
                    for &u in &op.support {
                        let from = setup.dest[usize::from(u)];
                        let to = setup.dest[usize::from(op.wing[usize::from(u)])];
                        moved[usize::from(to)] = pieces[usize::from(from)];
                    }
                    // A sideways step is only worth taking towards a new arrangement.
                    if gain <= 0 && seen.contains(&moved) {
                        continue;
                    }
                    best = Some((key, moved));
                }
            }
            let ((_, _, si, mi), moved) = best?;
            let (setup, op) = (&self.setups[si], &self.macros[mi]);
            // The bookkeeping above follows setup⁻¹ · macro · setup.
            path.extend(
                setup
                    .undo
                    .iter()
                    .chain(&op.moves)
                    .chain(&setup.moves)
                    .map(|&m| usize::from(m)),
            );
            pieces = moved;
            seen.insert(pieces);
        }
        None
    }

    /// A reduced 5x5 as a 3x3: rows and columns 0, 2, 4 of every face.
    pub fn reduce_to_3x3(facelets: &[u8]) -> Vec<u8> {
        const PICK: [usize; 3] = [0, 2, 4];
        let mut out = Vec::with_capacity(54);
        for face in 0..6 {
            for &r in &PICK {
                for &c in &PICK {
                    out.push(facelets[face * PER_FACE + r * 5 + c]);
                }
            }
        }
        out
    }

    /// The 5x5 turn matching a 3x3 face turn.
    pub fn lift_3x3_move(m: Move) -> Option<Move> {
        let layers = match m.layers {
            1 => 1,
            4 => 16,
            _ => return None,
        };
        Some(Move { layers, ..m })
    }
}

impl Default for Reduction5 {
    fn default() -> Self {
        Self::new()
    }
}

struct Phase1Space<'a> {
    r: &'a Reduction5,
    weight_halves: u16,
}

impl SearchSpace for Phase1Space<'_> {
    type Node = (u32, u32);

    fn moves(&self) -> &[u8] {
        &ALL36
    }

    fn step(&self, (x, t): Self::Node, mv: u8) -> Self::Node {
        let m = usize::from(mv);
        (self.r.x_bits[m].apply(x), self.r.t_bits[m].apply(t))
    }

    fn h(&self, (x, t): Self::Node) -> u8 {
        let h = self
            .r
            .p1_x
            .get(mask_rank(x) as usize)
            .max(self.r.p1_t.get(mask_rank(t) as usize));
        ((u16::from(h) * self.weight_halves / 2).min(254)) as u8
    }

    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
        self.r.canonical(prev, mv)
    }
}

/// Phase 1 searches the 36 block turns (the slice turns add nothing there).
const ALL36: [u8; 36] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
];

#[derive(Clone, Copy)]
struct Phase2aNode {
    x16: u32,
    t16: u32,
    parity: u32,
}

struct Phase2aSpace<'a> {
    r: &'a Reduction5,
}

impl SearchSpace for Phase2aSpace<'_> {
    type Node = Phase2aNode;

    fn moves(&self) -> &[u8] {
        &self.r.gens_a
    }

    fn step(&self, n: Self::Node, mv: u8) -> Self::Node {
        let m = usize::from(mv);
        let r = self.r;
        Phase2aNode {
            x16: r.x16_bits[m].apply(n.x16),
            t16: r.t16_bits[m].apply(n.t16),
            parity: n.parity ^ u32::from(r.wing_parity_flip[m]),
        }
    }

    fn h(&self, n: Self::Node) -> u8 {
        let r = self.r;
        r.p2a_x
            .get((mask_rank(n.x16) * 2 + n.parity) as usize)
            .max(r.p2a_t.get(mask_rank(n.t16) as usize))
    }

    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
        self.r.canonical(prev, mv)
    }
}

#[derive(Clone, Copy)]
struct Phase2bNode {
    x: u32,
    t: u32,
    wings: u32,
    midges: u32,
}

struct Phase2bSpace<'a> {
    r: &'a Reduction5,
    /// Heuristic weight in halves (2 = admissible, so shortest; more dives towards the
    /// goal and trades a few extra moves for a much smaller search).
    weight_halves: u16,
}

impl SearchSpace for Phase2bSpace<'_> {
    type Node = Phase2bNode;

    fn moves(&self) -> &[u8] {
        &self.r.gens_c
    }

    fn step(&self, n: Self::Node, mv: u8) -> Self::Node {
        let m = usize::from(mv);
        let r = self.r;
        Phase2bNode {
            x: r.x_axes.bits[m].apply(n.x),
            t: r.t_axes.bits[m].apply(n.t),
            wings: r.wing_bits[m].apply(n.wings),
            midges: r.midge_face_bits[m].apply(n.midges),
        }
    }

    fn h(&self, n: Self::Node) -> u8 {
        let r = self.r;
        let h = r
            .x_axes
            .to_solvable
            .get(r.centre_index(n.x))
            .max(r.t_axes.to_solvable.get(r.centre_index(n.t)))
            .max(r.p2b_wings.get(mask_rank(n.wings) as usize))
            .max(r.p2b_midges.get(mask_rank(n.midges) as usize));
        if h == rg_graph::bfs::UNREACHED {
            return h;
        }
        ((u16::from(h) * self.weight_halves / 2).min(254)) as u8
    }

    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
        self.r.canonical(prev, mv)
    }
}

#[derive(Clone, Copy)]
struct Phase3Node {
    x: u32,
    t: u32,
    pos: [[u8; 12]; 3],
    parity: [u32; 2],
}

struct Phase3Space<'a> {
    r: &'a Reduction5,
    moves: &'a [u8],
    /// (wing class, triple) pairs that must end attached.
    triples: &'a [(usize, usize)],
    /// Pairs of tredges that must end fully attached (both wing classes).
    pairs: &'a [(usize, usize)],
    /// Which centre orbits (x, t) must end solved.
    centres: [bool; 2],
    /// Heuristic weight in halves (2 = admissible). Kept for experiments; weighting did
    /// not help here (the max of independent tables is too loose for it).
    weight_halves: u16,
}

impl Phase3Space<'_> {
    fn pair_index(pos: &[[u8; 12]; 3], parity: u32, class: usize, triple: usize) -> usize {
        let t = TRIPLES[triple];
        let pack = |c: usize| {
            t.iter().fold(0usize, |acc, &d| {
                acc * 12 + usize::from(pos[c][usize::from(d)])
            })
        };
        (pack(class) * 1728 + pack(2)) * 2 + parity as usize
    }
}

impl SearchSpace for Phase3Space<'_> {
    type Node = Phase3Node;

    fn moves(&self) -> &[u8] {
        self.moves
    }

    fn step(&self, n: Self::Node, mv: u8) -> Self::Node {
        let m = usize::from(mv);
        let r = self.r;
        let step = |c: usize| {
            let s = &r.steps[c][m];
            std::array::from_fn(|d| s[usize::from(n.pos[c][d])])
        };
        Phase3Node {
            x: r.x_axes.bits[m].apply(n.x),
            t: r.t_axes.bits[m].apply(n.t),
            pos: [step(0), step(1), step(2)],
            parity: [
                n.parity[0] ^ u32::from(r.pair_parity_flip[0][m]),
                n.parity[1] ^ u32::from(r.pair_parity_flip[1][m]),
            ],
        }
    }

    fn h(&self, n: Self::Node) -> u8 {
        let r = self.r;
        let mut best = 0;
        if self.centres[0] {
            best = best.max(r.x_axes.table.get(r.centre_index(n.x)));
        }
        if self.centres[1] {
            best = best.max(r.t_axes.table.get(r.centre_index(n.t)));
        }
        for &(c, t) in self.triples {
            best = best.max(r.p3_pair[c].get(Self::pair_index(&n.pos, n.parity[c], c, t)));
        }
        for &(d1, d2) in self.pairs {
            let pack = |c: usize| usize::from(n.pos[c][d1]) * 12 + usize::from(n.pos[c][d2]);
            let par = n.parity[0] as usize | (n.parity[1] as usize) << 1;
            best = best.max(
                r.joint_pair
                    .get(((pack(0) * 144 + pack(1)) * 144 + pack(2)) * 4 + par),
            );
        }
        for a in (0..3).filter(|_| self.centres == [true, true]) {
            let shift = 8 * a;
            let (x, t) = (
                (n.x >> shift & 0xFF) as usize,
                (n.t >> shift & 0xFF) as usize,
            );
            let i = usize::from(r.rank8[x]) * 70 + usize::from(r.rank8[t]);
            best = best.max(r.axis_joint[a].get(i));
        }
        if best == rg_graph::bfs::UNREACHED {
            return best;
        }
        (u16::from(best) * self.weight_halves / 2).min(u16::from(rg_graph::bfs::UNREACHED - 1))
            as u8
    }

    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
        self.r.canonical(prev, mv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn shared() -> &'static Reduction5 {
        static TABLES: OnceLock<Reduction5> = OnceLock::new();
        TABLES.get_or_init(Reduction5::new)
    }

    #[test]
    fn discovered_macros_preserve_centres_and_midges() {
        let r = shared();
        let t0 = std::time::Instant::now();
        let library = r.macro_library(4, 6);
        eprintln!(
            "macros: {} in {:.1} s; shortest {:?}",
            library.len(),
            t0.elapsed().as_secs_f64(),
            library.first().map(|m| (m.moves.len(), m.support.len()))
        );
        assert!(!library.is_empty(), "the search found no macro operators");
        let solved = Cube::new(5).unwrap();
        for op in library.iter() {
            let mut cube = Cube::new(5).unwrap();
            for &m in &op.moves {
                cube.apply_move(r.moves[usize::from(m)]);
            }
            // Centres back where they were, every midge home, and only the wings the
            // macro claims to move have moved.
            assert_eq!(
                r.x_axes.packed(r.xs(), cube.facelets()),
                r.x_axes.packed(r.xs(), solved.facelets())
            );
            assert_eq!(
                r.t_axes.packed(r.ts(), cube.facelets()),
                r.t_axes.packed(r.ts(), solved.facelets())
            );
            let midges = r.slots.midges.as_ref().unwrap();
            let homes = midges.pieces_unordered(cube.facelets()).unwrap();
            assert!(
                homes.iter().enumerate().all(|(i, &h)| usize::from(h) == i),
                "a macro moved a midge"
            );
            for (slot, &[a, b]) in midges.facelets.iter().enumerate() {
                assert_eq!(
                    (cube.facelets()[a], cube.facelets()[b]),
                    (solved.facelets()[a], solved.facelets()[b]),
                    "a macro flipped midge {slot}"
                );
            }
            let wings = r.slots.wings[0].pieces(cube.facelets()).unwrap();
            let moved: Vec<u8> = (0..24u8)
                .filter(|&s| usize::from(wings[usize::from(s)]) != usize::from(s))
                .collect();
            assert_eq!(moved.len(), op.support.len(), "wing support");
        }
    }

    #[test]
    fn expanding_to_block_turns_keeps_the_permutation() {
        let geom = Geometry::new(5);
        for &m in &moves_5x5() {
            let mut a = Cube::new(5).unwrap();
            a.apply_move(m);
            let mut b = Cube::new(5).unwrap();
            b.apply_moves(&expand_obtm(&[m]));
            assert_eq!(a.facelets(), b.facelets(), "{m:?} expanded wrongly");
        }
        let _ = geom;
    }

    #[test]
    #[ignore = "timing survey, not a correctness check"]
    fn table_build_time() {
        let t = std::time::Instant::now();
        let r = Reduction5::new();
        eprintln!(
            "Reduction5::new: {:.2}s, {} macros, {} setups",
            t.elapsed().as_secs_f64(),
            r.macros.len(),
            r.setups.len()
        );
    }

    #[test]
    #[ignore = "timing survey, not a correctness check"]
    fn phase_timings() {
        let r = shared();
        for seed in 700..712 {
            let mut cube = Cube::new(5).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(5, 60, seed));
            let start = std::time::Instant::now();
            let deadline = move || start.elapsed().as_secs_f64() > 120.0;
            let mut times = Vec::new();
            for phase in 1..=3 {
                let t = std::time::Instant::now();
                let path = match phase {
                    1 => r.phase1(cube.facelets(), &deadline),
                    2 => r.phase2a(cube.facelets(), &deadline),
                    _ => r.phase2b(cube.facelets(), &deadline),
                }
                .unwrap();
                times.push(format!("{:.2}s/{}", t.elapsed().as_secs_f64(), path.len()));
                for m in path {
                    cube.apply_move(r.moves[m]);
                }
            }
            eprintln!("seed {seed}: 1 {} | 2a {} | 2b {}", times[0], times[1], times[2]);
        }
    }

    #[test]
    fn phase3_moves_track_the_centres() {
        let r = shared();
        let solved = Cube::new(5).unwrap().facelets().to_vec();
        for &m in &r.gens_b {
            let mut cube = Cube::new(5).unwrap();
            cube.apply_move(r.moves[usize::from(m)]);
            let real_x = r.x_axes.packed(r.xs(), cube.facelets()).unwrap();
            let real_t = r.t_axes.packed(r.ts(), cube.facelets()).unwrap();
            let start_x = r.x_axes.packed(r.xs(), &solved).unwrap();
            let start_t = r.t_axes.packed(r.ts(), &solved).unwrap();
            assert_eq!(
                r.x_axes.bits[usize::from(m)].apply(start_x),
                real_x,
                "x-centres after {:?}",
                r.moves[usize::from(m)]
            );
            assert_eq!(
                r.t_axes.bits[usize::from(m)].apply(start_t),
                real_t,
                "t-centres after {:?}",
                r.moves[usize::from(m)]
            );
        }
    }

    #[test]
    fn reduces_to_a_legal_3x3() {
        let r = shared();
        println!("{}", r.report());
        for seed in 0..6 {
            let mut cube = Cube::new(5).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(5, 60, 700 + seed));
            let start = std::time::Instant::now();
            let deadline = move || start.elapsed().as_secs_f64() > 300.0;
            let mut lengths = Vec::new();
            for phase in 1..=4 {
                let t = std::time::Instant::now();
                let path = match phase {
                    1 => r.phase1(cube.facelets(), &deadline),
                    2 => r.phase2a(cube.facelets(), &deadline),
                    3 => r.phase2b(cube.facelets(), &deadline),
                    _ => {
                        let clock = move || start.elapsed().as_secs_f64() * 1000.0;
                        r.phase3(cube.facelets(), &clock, clock() + 300_000.0)
                    }
                }
                .unwrap_or_else(|| panic!("seed {seed}: phase {phase} failed"));
                println!(
                    "seed {seed} phase {phase}: {} moves, {:.2}s",
                    path.len(),
                    t.elapsed().as_secs_f64()
                );
                lengths.push(path.len());
                for m in path {
                    cube.apply_move(r.moves[m]);
                }
            }
            let pos = r.edge_positions(cube.facelets()).unwrap();
            for (d, &midge) in pos[2].iter().enumerate() {
                assert_eq!(
                    (pos[0][d], pos[1][d]),
                    (midge, midge),
                    "seed {seed}: edge {d} is not paired"
                );
            }
            // Every tredge must show one colour on each of its two faces, and every
            // centre block one colour: that is what makes the 5x5 act like a 3x3.
            let geom = Geometry::new(5);
            let mut groups: std::collections::HashMap<[i32; 3], Vec<u8>> =
                std::collections::HashMap::new();
            for (f, &colour) in cube.facelets().iter().enumerate() {
                let c = geom.cubies()[f];
                let face = f / 25;
                let key: [i32; 3] = std::array::from_fn(|k| {
                    if c[k].abs() == 4 { c[k] } else { 0 }
                });
                groups
                    .entry([key[0], key[1], key[2] + 16 * face as i32])
                    .or_default()
                    .push(colour);
            }
            for (key, colours) in &groups {
                assert!(
                    colours.iter().all(|c| *c == colours[0]),
                    "seed {seed}: block {key:?} is not one colour: {colours:?}"
                );
            }
            let three = Reduction5::reduce_to_3x3(cube.facelets());
            crate::cubie::CubieCube::from_facelets(&three).expect("legal 3x3");
            println!("seed {seed}: reduction {lengths:?}");
        }
    }
}
