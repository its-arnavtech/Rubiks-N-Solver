//! Big-cube reduction (docs/08). Built phase by phase; each phase is a shortest-path
//! problem in a quotient graph whose distances are precomputed with BFS.

use rg_cube::{Cube, Move};
use rg_graph::bfs::DistanceTable;
use rg_graph::combinatorics::{binomial, mask_rank, mask_unrank};
use rg_graph::ida::{Flow, SearchCtl, SearchSpace, search_exact};

use crate::slots::{CentreKind, CentreOrbit, Slots};

/// Opposite colour pairs: U/D, R/L, F/B.
pub const COLOUR_PAIRS: [[u8; 2]; 3] = [[0, 3], [1, 4], [2, 5]];
/// Index of the pair that must end up on the R/L axis after phase 1.
pub const RL_PAIR: usize = 1;

/// The partition whose triples are paired one stage at a time.
const BASE_PARTITION: [[u8; 3]; 4] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]];

/// Four partitions of the twelve dedges into triples. One pairing database serves every
/// triple, so taking the maximum over all sixteen is a much stronger admissible heuristic
/// than using a single partition, and each partition alone already covers all 12 dedges
/// (so h = 0 implies every dedge is paired).
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

/// The 4x4 move set: six faces plus Uw, Rw, Fw, each × {1, 2, 3} = 27 moves.
pub fn moves_4x4() -> Vec<Move> {
    rg_cube::parse_alg(
        4,
        "U U2 U' D D2 D' L L2 L' R R2 R' F F2 F' B B2 B' Uw Uw2 Uw' Rw Rw2 Rw' Fw Fw2 Fw'",
    )
    .expect("valid notation")
}

/// A slot permutation compiled into byte lookup tables, so permuting a 24-bit slot mask
/// costs three lookups instead of a loop. Table building walks millions of masks.
#[derive(Clone)]
pub struct BitPerm {
    tables: [[u32; 256]; 3],
}

impl BitPerm {
    /// `perm` is in gather form (`new[i] = old[perm[i]]`), matching the slot orbits.
    pub fn new(perm: &[u8]) -> Self {
        assert!(perm.len() <= 24, "slot masks are 24 bits");
        let mut inv = [0u8; 24];
        for (i, &p) in perm.iter().enumerate() {
            inv[usize::from(p)] = i as u8;
        }
        let mut tables = [[0u32; 256]; 3];
        for (b, table) in tables.iter_mut().enumerate() {
            for (v, out) in table.iter_mut().enumerate() {
                for j in 0..8 {
                    let source = b * 8 + j;
                    if v >> j & 1 == 1 && source < perm.len() {
                        *out |= 1 << inv[source];
                    }
                }
            }
        }
        Self { tables }
    }

    #[inline]
    pub fn apply(&self, mask: u32) -> u32 {
        self.tables[0][(mask & 0xFF) as usize]
            | self.tables[1][(mask >> 8 & 0xFF) as usize]
            | self.tables[2][(mask >> 16 & 0xFF) as usize]
    }
}

/// Moves that leave a slot mask fixed: exactly the ones that preserve a phase goal
/// expressed by that mask (gate V1 in docs/08 §2).
pub fn preserving(perms: &[BitPerm], moves: &[usize], mask: u32) -> Vec<usize> {
    moves
        .iter()
        .copied()
        .filter(|&m| perms[m].apply(mask) == mask)
        .collect()
}

/// Connected components of the slots under a set of moves (union-find over the
/// permutations). Used to derive the wing classes of a phase group.
pub fn slot_components(perms: &[Vec<u8>], moves: &[usize], slots: usize) -> Vec<u8> {
    let mut parent: Vec<usize> = (0..slots).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &m in moves {
        for (i, &p) in perms[m].iter().enumerate() {
            let (a, b) = (root(&mut parent, i), root(&mut parent, usize::from(p)));
            parent[a] = b;
        }
    }
    let mut label = vec![u8::MAX; slots];
    let mut next = 0u8;
    for i in 0..slots {
        let r = root(&mut parent, i);
        if label[r] == u8::MAX {
            label[r] = next;
            next += 1;
        }
        label[i] = label[r];
    }
    label
}

/// Exact distances over k-subsets of the slots, under a subset of the move set.
pub fn subset_table(
    perms: &[BitPerm],
    moves: &[usize],
    goals: &[u32],
    slots: u32,
    k: usize,
) -> DistanceTable {
    let size = binomial(slots, k as u32) as usize;
    DistanceTable::build_queued(size, goals, |v, push| {
        let mask = mask_unrank(v, slots, k);
        for &m in moves {
            push(mask_rank(perms[m].apply(mask)));
        }
    })
}

/// Restrict a 24-slot gather permutation to the slots kept by `keep`, renumbering them in
/// increasing slot order. Only valid for moves that map the kept set onto itself.
fn restrict(perm: &[u8], keep: u32) -> Vec<u8> {
    let kept: Vec<usize> = (0..perm.len()).filter(|&i| keep >> i & 1 == 1).collect();
    let mut index = vec![u8::MAX; perm.len()];
    for (i, &s) in kept.iter().enumerate() {
        index[s] = i as u8;
    }
    kept.iter()
        .map(|&s| {
            let src = usize::from(perm[s]);
            assert_ne!(index[src], u8::MAX, "move does not preserve the kept slots");
            index[src]
        })
        .collect()
}

pub struct Reduction4 {
    pub slots: Slots,
    pub moves: Vec<Move>,
    all_moves: Vec<usize>,
    /// (axis, layer mask) per move, for the canonical-sequence filter.
    shape: Vec<(u8, u8)>,
    centre_bits: Vec<BitPerm>,
    wing_bits: Vec<BitPerm>,
    /// C(24,8) distances to "this colour pair occupies the R and L faces".
    p1: DistanceTable,
    /// Phase-2 generators: the moves that keep the R/L centres on the R/L axis.
    gens_a: Vec<u8>,
    /// Phase-3 generators: also keep the U/D centres on the U/D axis.
    gens_b: Vec<u8>,
    /// Centre slots not on the R/L axis, renumbered 0..16, under `gens_a`.
    kept16: Vec<usize>,
    centre16_bits: Vec<BitPerm>,
    /// C(16,8) distances to "the U/D pair occupies the U and D faces".
    p2_centres: DistanceTable,
    /// Wing class (0 or 1) of each wing slot under `gens_b`.
    wing_class: Vec<u8>,
    class_goal: u32,
    /// Distances to "every wing sits in a slot of its own class, with even permutation
    /// parity"; indexed by `mask_rank * 2 + parity`.
    p2_wings: DistanceTable,
    /// Whether each move flips the parity of the wing permutation.
    wing_parity_flip: Vec<bool>,
    /// Whether each move flips "corner parity equals dedge parity".
    p3_parity_flip: Vec<bool>,
    /// Centre slots of each axis (R/L, U/D, F/B), eight per axis.
    axis_slots: [[u8; 8]; 3],
    /// Packed-axis bit permutation per move (only meaningful for `gens_b`).
    axis_bits: Vec<BitPerm>,
    rank8: [u8; 256],
    /// Centre slots of each axis as a mask.
    axis_mask: [u32; 3],
    /// Dedge (0..12) of each wing slot.
    dedge_of: [u8; 24],
    /// Class-local position mapping per class and move.
    class_step: [Vec<[u8; 12]>; 2],
    triples: Vec<[u8; 3]>,
    /// 343,000 distances to "every centre face is uniform".
    p3_centres: [DistanceTable; 2],
    /// The face holding slot 0 of each axis (the "first" face of the axis).
    first_face: [usize; 3],
    /// Pairing pattern database for any three dedges (positions only).
    p3_pair: DistanceTable,
    /// Face-to-colour maps reachable by a real cube.
    schemes: Vec<[u8; 6]>,
    /// The parity bit of the solved cube shown in each scheme (same order as `schemes`).
    scheme_bits: Vec<u32>,
}

impl Reduction4 {
    pub fn new() -> Self {
        let moves = moves_4x4();
        let slots = Slots::new(4, moves.clone());
        let all_moves: Vec<usize> = (0..moves.len()).collect();
        let shape = moves.iter().map(|m| (m.axis as u8, m.layers)).collect();
        let solved = Cube::new(4).expect("4x4").facelets().to_vec();
        let centres = slots
            .centre_orbit(CentreKind::X)
            .expect("4x4 centres")
            .clone();
        let wings = slots.wings[0].clone();

        let centre_bits: Vec<BitPerm> = centres.perms.iter().map(|p| BitPerm::new(p)).collect();
        let wing_bits: Vec<BitPerm> = wings.perms.iter().map(|p| BitPerm::new(p)).collect();

        // Phase 1: one colour pair onto the R/L axis.
        let rl_goal = centres.mask(&solved, COLOUR_PAIRS[RL_PAIR]);
        let p1 = subset_table(&centre_bits, &all_moves, &[mask_rank(rl_goal)], 24, 8);

        // Phase 2 generators are derived, not listed: the moves that fix the R/L centre set.
        let gens_a: Vec<u8> = preserving(&centre_bits, &all_moves, rl_goal)
            .iter()
            .map(|&m| m as u8)
            .collect();
        // Phase 3's group also fixes the U/D centres, and — a design choice, exactly as in
        // Thistlethwaite's chain — drops the F/B quarter turns. That restriction is what
        // gives the wings an orientation invariant for phase 2 to fix.
        let ud_goal = centres.mask(&solved, COLOUR_PAIRS[0]);
        let gens_b: Vec<u8> = preserving(
            &centre_bits,
            &gens_a.iter().map(|&m| usize::from(m)).collect::<Vec<_>>(),
            ud_goal,
        )
        .into_iter()
        .filter(|&m| moves[m].axis != rg_cube::Axis::Z || moves[m].turns == 2)
        .map(|m| m as u8)
        .collect();

        // Phase 2a: the U/D pair onto the U/D axis, seen only through the 16 non-R/L slots.
        let kept16: Vec<usize> = (0..24).filter(|i| rl_goal >> i & 1 == 0).collect();
        let centre16_bits: Vec<BitPerm> = centres
            .perms
            .iter()
            .enumerate()
            .map(|(m, p)| {
                if gens_a.contains(&(m as u8)) {
                    BitPerm::new(&restrict(p, rl_goal ^ 0xFF_FFFF))
                } else {
                    BitPerm::new(&(0..16).collect::<Vec<u8>>())
                }
            })
            .collect();
        let to16 = |mask24: u32| {
            kept16
                .iter()
                .enumerate()
                .fold(0u32, |m, (i, &s)| m | (mask24 >> s & 1) << i)
        };
        let ud_goal16 = to16(ud_goal);
        // Either remaining pair may take the U/D axis, so both placements are goals.
        let p2_centres = subset_table(
            &centre16_bits,
            &gens_a.iter().map(|&m| usize::from(m)).collect::<Vec<_>>(),
            &[mask_rank(ud_goal16), mask_rank(!ud_goal16 & 0xFFFF)],
            16,
            8,
        );

        // Phase 2b: wing classes are the orbits of the wing slots under the phase-3 group.
        let wing_class = slot_components(
            &wings.perms,
            &gens_b.iter().map(|&m| usize::from(m)).collect::<Vec<_>>(),
            24,
        );
        let class_goal = (0..24).fold(0u32, |m, i| if wing_class[i] == 0 { m | 1 << i } else { m });

        // A paired 4x4 always has an even wing permutation, and phase-3 moves never change
        // its parity — the "OLL parity" of 4x4 solving. Phase 2 therefore has to deliver an
        // even permutation, so the parity rides along in its wing coordinate.
        let wing_parity_flip: Vec<bool> = wings
            .perms
            .iter()
            .map(|p| rg_graph::combinatorics::is_odd(p))
            .collect();
        let k = class_goal.count_ones() as usize;
        let gens_a_idx: Vec<usize> = gens_a.iter().map(|&m| usize::from(m)).collect();
        let p2_wings = {
            let size = binomial(24, k as u32) as usize * 2;
            DistanceTable::build_queued(size, &[mask_rank(class_goal) * 2], |v, push| {
                let (mask, parity) = (mask_unrank(v / 2, 24, k), v & 1);
                for &m in &gens_a_idx {
                    let p = parity ^ u32::from(wing_parity_flip[m]);
                    push(mask_rank(wing_bits[m].apply(mask)) * 2 + p);
                }
            })
        };

        // ---- Phase 3 ----
        let geom = rg_cube::Geometry::new(4);
        let gens_b_idx: Vec<usize> = gens_b.iter().map(|&m| usize::from(m)).collect();

        // Centre slots grouped by axis: R/L, U/D, F/B.
        let mut axis_slots = [[0u8; 8]; 3];
        let mut filled = [0usize; 3];
        for (s, &f) in centres.facelets.iter().enumerate() {
            let axis = match f / 16 {
                1 | 4 => 0,
                0 | 3 => 1,
                _ => 2,
            };
            axis_slots[axis][filled[axis]] = s as u8;
            filled[axis] += 1;
        }
        assert_eq!(filled, [8, 8, 8]);
        let axis_index: Vec<(usize, usize)> = {
            let mut v = vec![(0, 0); 24];
            for (a, slots) in axis_slots.iter().enumerate() {
                for (i, &s) in slots.iter().enumerate() {
                    v[usize::from(s)] = (a, i);
                }
            }
            v
        };
        let axis_bits: Vec<BitPerm> = centres
            .perms
            .iter()
            .enumerate()
            .map(|(m, p)| {
                if !gens_b.contains(&(m as u8)) {
                    return BitPerm::new(&(0..24).collect::<Vec<u8>>());
                }
                let mut gather = vec![0u8; 24];
                for (a, slots) in axis_slots.iter().enumerate() {
                    for (i, &s) in slots.iter().enumerate() {
                        let (a2, j) = axis_index[usize::from(p[usize::from(s)])];
                        assert_eq!(a, a2, "phase-3 moves keep centres on their axis");
                        gather[a * 8 + i] = (a2 * 8 + j) as u8;
                    }
                }
                BitPerm::new(&gather)
            })
            .collect();
        let mut rank8 = [0u8; 256];
        let mut unrank8 = [0u8; 70];
        for mask in 0u32..256 {
            if mask.count_ones() == 4 {
                let r = mask_rank(mask) as u8;
                rank8[mask as usize] = r;
                unrank8[usize::from(r)] = mask as u8;
            }
        }

        // A dedge is a cube edge: the two wing cubies along it are different pieces, so
        // group by the two face coordinates and ignore the position along the edge.
        let mut edge_ids: Vec<[i32; 3]> = Vec::new();
        let mut dedge_of = [0u8; 24];
        for (s, &[a, _]) in wings.facelets.iter().enumerate() {
            let cubie = geom.cubies()[a];
            let key: [i32; 3] =
                std::array::from_fn(|k| if cubie[k].abs() == 3 { cubie[k] } else { 0 });
            let d = edge_ids.iter().position(|c| *c == key).unwrap_or_else(|| {
                edge_ids.push(key);
                edge_ids.len() - 1
            });
            dedge_of[s] = d as u8;
        }
        assert_eq!(edge_ids.len(), 12, "a 4x4 has twelve dedges");
        let mut slot_of = [[u8::MAX; 12]; 2];
        for s in 0..24 {
            let c = usize::from(wing_class[s]);
            let d = usize::from(dedge_of[s]);
            assert_eq!(slot_of[c][d], u8::MAX, "each dedge has one slot per class");
            slot_of[c][d] = s as u8;
        }
        let class_step: [Vec<[u8; 12]>; 2] = std::array::from_fn(|c| {
            wings
                .perms
                .iter()
                .map(|p| {
                    let mut inv = [0u8; 24];
                    for (i, &q) in p.iter().enumerate() {
                        inv[usize::from(q)] = i as u8;
                    }
                    std::array::from_fn(|d| dedge_of[usize::from(inv[usize::from(slot_of[c][d])])])
                })
                .collect()
        });

        // Centres fully solved: each axis byte is one of its two faces' slot sets.
        let axis_goal_bytes: [[u32; 2]; 3] = std::array::from_fn(|a| {
            let face_of = |s: u8| centres.facelets[usize::from(s)] / 16;
            let first = face_of(axis_slots[a][0]);
            let byte = axis_slots[a].iter().enumerate().fold(0u32, |m, (i, &s)| {
                if face_of(s) == first { m | 1 << i } else { m }
            });
            [byte, !byte & 0xFF]
        });
        // The eight "every face uniform" arrangements split into two classes by how many
        // axes are flipped. Flipping one axis mirrors the scheme, so exactly one class is
        // made of proper (non-mirrored) arrangements; which one depends on the colours, so
        // both tables are built and the solver picks per cube.
        let mut centre_goals = [Vec::new(), Vec::new()];
        for (fx, x) in axis_goal_bytes[0].into_iter().enumerate() {
            for (fy, y) in axis_goal_bytes[1].into_iter().enumerate() {
                for (fz, z) in axis_goal_bytes[2].into_iter().enumerate() {
                    let packed = x | y << 8 | z << 16;
                    let idx = (usize::from(rank8[(packed & 0xFF) as usize]) * 70
                        + usize::from(rank8[(packed >> 8 & 0xFF) as usize]))
                        * 70
                        + usize::from(rank8[(packed >> 16 & 0xFF) as usize]);
                    centre_goals[(fx + fy + fz) % 2].push(idx as u32);
                }
            }
        }
        let p3_centres = centre_goals.map(|goals| {
            let pack = |v: u32| {
                let (a, b, c) = (v as usize / 4900, v as usize / 70 % 70, v as usize % 70);
                u32::from(unrank8[a]) | u32::from(unrank8[b]) << 8 | u32::from(unrank8[c]) << 16
            };
            let index = |packed: u32| {
                (usize::from(rank8[(packed & 0xFF) as usize]) * 70
                    + usize::from(rank8[(packed >> 8 & 0xFF) as usize]))
                    * 70
                    + usize::from(rank8[(packed >> 16 & 0xFF) as usize])
            };
            DistanceTable::build_queued(343_000, &goals, |v, push| {
                let packed = pack(v);
                for &m in &gens_b_idx {
                    push(index(axis_bits[m].apply(packed)) as u32);
                }
            })
        });
        let first_face: [usize; 3] =
            std::array::from_fn(|a| centres.facelets[usize::from(axis_slots[a][0])] / 16);

        // Pairing database: where three dedges' wings sit, and how far from being paired.
        // The transitions and the goal depend only on positions, never on which pieces
        // they are, so one table serves every triple.
        let triples = TRIPLES.to_vec();
        let mut goals = Vec::new();
        for a in 0..12u32 {
            for b in 0..12u32 {
                for c in 0..12u32 {
                    if a != b && b != c && a != c {
                        let p = (a * 12 + b) * 12 + c;
                        goals.push(p * 1728 + p);
                    }
                }
            }
        }
        // A reduced 4x4 is only a legal 3x3 when the corner and dedge permutations have the
        // same parity (the 4x4 "PLL parity"). Phase-3 moves can flip that relation, so the
        // bit rides along in the pairing database and the heuristic can see it.
        let p3_parity_flip: Vec<bool> = (0..moves.len())
            .map(|m| {
                let mut c = Cube::new(4).expect("4x4");
                c.apply_move(moves[m]);
                let (cp, _) = crate::cubie::corners_from_facelets(&geom, c.facelets())
                    .expect("a turn keeps corners legal");
                let corners = rg_graph::combinatorics::is_odd(&cp);
                let wings = rg_graph::combinatorics::is_odd(&class_step[0][m]);
                corners != wings
            })
            .collect();
        let parity_goals: Vec<u32> = goals.iter().map(|&g| g * 2).collect();
        let p3_pair = DistanceTable::build_queued(1728 * 1728 * 2, &parity_goals, |v, push| {
            let digits = |x: u32| [x / 144 % 12, x / 12 % 12, x % 12];
            let (positions, parity) = (v / 2, v & 1);
            let (d0, d1) = (digits(positions / 1728), digits(positions % 1728));
            for &m in &gens_b_idx {
                let f = |step: &[u8; 12], d: [u32; 3]| {
                    d.iter()
                        .fold(0u32, |acc, &x| acc * 12 + u32::from(step[x as usize]))
                };
                let p = parity ^ u32::from(p3_parity_flip[m]);
                push((f(&class_step[0][m], d0) * 1728 + f(&class_step[1][m], d1)) * 2 + p);
            }
        });

        let mut reduction = Self {
            slots,
            moves,
            all_moves,
            shape,
            centre_bits,
            wing_bits,
            p1,
            gens_a,
            gens_b,
            kept16,
            centre16_bits,
            p2_centres,
            wing_class,
            class_goal,
            p2_wings,
            wing_parity_flip,
            axis_slots,
            axis_bits,
            rank8,
            axis_mask: std::array::from_fn(|a| axis_slots[a].iter().fold(0u32, |m, &s| m | 1 << s)),
            dedge_of,
            class_step,
            triples,
            p3_centres,
            first_face,
            p3_pair,
            p3_parity_flip,
            schemes: Self::proper_schemes(),
            scheme_bits: Vec::new(),
        };
        // Same rotation order as `proper_schemes`.
        reduction.scheme_bits = rg_cube::whole_cube_rotations(4)
            .into_iter()
            .map(|rot| {
                let mut c = Cube::new(4).expect("4x4");
                c.apply_moves(&rot);
                reduction
                    .parity_bit(c.facelets())
                    .expect("a solved cube reads cleanly")
            })
            .collect();
        reduction
    }

    /// Mask of the class-0 wing slots: the phase-2 wing goal.
    pub fn wing_class_goal(&self) -> u32 {
        self.class_goal
    }

    fn centres(&self) -> &CentreOrbit {
        self.slots.centre_orbit(CentreKind::X).expect("4x4 centres")
    }

    /// Packed phase-3 centre state: one byte per axis, each bit a slot of that axis set
    /// when it shows the axis' reference colour.
    fn centre_axes(&self, facelets: &[u8], axis_colour: [u8; 3]) -> u32 {
        let orbit = self.centres();
        self.axis_slots
            .iter()
            .enumerate()
            .fold(0u32, |acc, (a, slots)| {
                let byte = slots.iter().enumerate().fold(0u32, |m, (i, &s)| {
                    if facelets[orbit.facelets[usize::from(s)]] == axis_colour[a] {
                        m | 1 << i
                    } else {
                        m
                    }
                });
                acc | byte << (8 * a)
            })
    }

    fn centre_index(&self, packed: u32) -> usize {
        let r = |byte: u32| usize::from(self.rank8[(byte & 0xFF) as usize]);
        (r(packed) * 70 + r(packed >> 8)) * 70 + r(packed >> 16)
    }

    /// Class-local positions of every wing piece: `pos[c][home dedge] = dedge slot it is in`.
    fn wing_positions(&self, facelets: &[u8]) -> Option<[[u8; 12]; 2]> {
        let pieces = self.slots.wings[0].pieces(facelets)?;
        let mut pos = [[0u8; 12]; 2];
        for (slot, &piece) in pieces.iter().enumerate() {
            let class = usize::from(self.wing_class[usize::from(piece)]);
            pos[class][usize::from(self.dedge_of[usize::from(piece)])] = self.dedge_of[slot];
        }
        Some(pos)
    }

    fn pair_index(&self, pos: &[[u8; 12]; 2], parity: u32, triple: usize) -> usize {
        let t = self.triples[triple];
        let pack = |c: usize| {
            t.iter().fold(0usize, |acc, &d| {
                acc * 12 + usize::from(pos[c][usize::from(d)])
            })
        };
        (pack(0) * 1728 + pack(1)) * 2 + parity as usize
    }

    /// Phase Lab summary (docs/08 §4).
    pub fn report(&self) -> String {
        let class_sizes = (0..2)
            .map(|c| self.wing_class.iter().filter(|&&x| x == c).count())
            .collect::<Vec<_>>();
        format!(
            "phase 1: {} vertices, depth {}\n\
             gens A: {} moves, gens B: {} moves\n\
             phase 2 centres: {} vertices, depth {}\n\
             wing classes: {:?}\n\
             phase 2 wings: {} vertices, depth {}\n\
             phase 3 centres: {} vertices, depth {}\n\
             phase 3 pairing PDB: {} vertices reached, depth {}",
            self.p1.reached(),
            self.p1.max_depth(),
            self.gens_a.len(),
            self.gens_b.len(),
            self.p2_centres.reached(),
            self.p2_centres.max_depth(),
            class_sizes,
            self.p2_wings.reached(),
            self.p2_wings.max_depth(),
            self.p3_centres[0].reached() + self.p3_centres[1].reached(),
            self.p3_centres[0]
                .max_depth()
                .max(self.p3_centres[1].max_depth()),
            self.p3_pair.reached(),
            self.p3_pair.max_depth(),
        )
    }

    /// Layer histogram of the phase-1 quotient graph (Phase Lab / tests).
    pub fn phase1_histogram(&self) -> &[u64] {
        self.p1.histogram()
    }

    /// Distance to "some opposite colour pair occupies the R/L axis", and the best pair.
    pub fn phase1_distance(&self, facelets: &[u8]) -> (u8, usize) {
        let orbit = self.centres();
        (0..COLOUR_PAIRS.len())
            .map(|p| {
                (
                    self.p1
                        .get(mask_rank(orbit.mask(facelets, COLOUR_PAIRS[p])) as usize),
                    p,
                )
            })
            .min()
            .expect("three pairs")
    }

    /// Canonical sequences: same-axis turns commute, so a run on one axis must be in
    /// increasing layer-mask order, and a layer set never turns twice in a row.
    fn canonical(&self, prev: Option<u8>, m: u8) -> bool {
        match prev {
            None => true,
            Some(p) => {
                let (a, b) = (self.shape[usize::from(p)], self.shape[usize::from(m)]);
                a.0 != b.0 || a.1 < b.1
            }
        }
    }

    fn centre16_mask(&self, facelets: &[u8], pair: usize) -> u32 {
        let orbit = self.centres();
        self.kept16.iter().enumerate().fold(0u32, |m, (i, &s)| {
            let c = facelets[orbit.facelets[s]];
            if COLOUR_PAIRS[pair].contains(&c) {
                m | 1 << i
            } else {
                m
            }
        })
    }

    /// Mask of the wing slots currently holding a class-0 wing.
    fn wing_class_mask(&self, facelets: &[u8]) -> Option<u32> {
        let pieces = self.slots.wings[0].pieces(facelets)?;
        Some((0..24).fold(0u32, |m, i| {
            if self.wing_class[usize::from(pieces[i])] == 0 {
                m | 1 << i
            } else {
                m
            }
        }))
    }

    /// Colour masks over the centre slots: `mask[c]` holds the slots showing colour `c`.
    fn colour_masks(&self, facelets: &[u8]) -> [u32; 6] {
        let orbit = self.centres();
        let mut masks = [0u32; 6];
        for (i, &f) in orbit.facelets.iter().enumerate() {
            masks[usize::from(facelets[f])] |= 1 << i;
        }
        masks
    }

    /// Can phase 3 finish from here? Its centre arrangement must be in the reachable
    /// component and the wing permutation must be even (docs/08 §3).
    pub fn phase3_feasible(&self, facelets: &[u8]) -> bool {
        let Some(pieces) = self.slots.wings[0].pieces(facelets) else {
            return false;
        };
        !rg_graph::combinatorics::is_odd(&pieces)
            && self
                .phase3_centre_index(&self.colour_masks(facelets))
                .is_some()
    }

    /// Which centre table holds the proper (non-mirrored) goals, given each axis'
    /// (reference, other) colours. With no axis flipped, each axis' first face shows its
    /// reference colour; if that scheme is proper, the even class is the proper one.
    fn proper_class(&self, axis_pairs: [[u8; 2]; 3]) -> usize {
        let mut map = [0u8; 6];
        for (a, pair) in axis_pairs.iter().enumerate() {
            let f = self.first_face[a];
            map[f] = pair[0];
            map[(f + 3) % 6] = pair[1];
        }
        usize::from(!self.schemes.contains(&map))
    }

    /// Phase-3 centre index and proper table class of a centre colouring, or `None` when
    /// phase 3 could never solve it properly (another component of phase 3's graph).
    fn phase3_centre_index(&self, colours: &[u32; 6]) -> Option<(usize, usize)> {
        let mut packed = 0u32;
        let mut pairs = [[0u8; 2]; 3];
        for (a, pair) in pairs.iter_mut().enumerate() {
            let axis_mask = self.axis_mask[a];
            let present: Vec<u8> = (0..6u8)
                .filter(|&c| colours[usize::from(c)] & axis_mask != 0)
                .collect();
            let [reference, other] = present[..] else {
                return None;
            };
            *pair = [reference, other];
            let byte = self.axis_slots[a]
                .iter()
                .enumerate()
                .fold(0u32, |m, (i, &s)| {
                    if colours[usize::from(reference)] >> s & 1 == 1 {
                        m | 1 << i
                    } else {
                        m
                    }
                });
            if byte.count_ones() != 4 {
                return None;
            }
            packed |= byte << (8 * a);
        }
        let index = self.centre_index(packed);
        let class = self.proper_class(pairs);
        (self.p3_centres[class].get(index) != rg_graph::bfs::UNREACHED).then_some((index, class))
    }

    /// Phase 2: put a second colour pair on the U/D axis and every wing into a slot of its
    /// own class, without disturbing phase 1 — and only accept arrangements phase 3 can
    /// finish from (connectivity lifting, docs/08 §3).
    pub fn phase2(
        &self,
        facelets: &[u8],
        pair: usize,
        time_up: &dyn Fn() -> bool,
    ) -> Option<Vec<usize>> {
        let track = (0..COLOUR_PAIRS.len())
            .find(|&p| p != pair)
            .expect("two pairs remain");
        let pieces = self.slots.wings[0].pieces(facelets)?;
        let start = Phase2Node {
            c16: self.centre16_mask(facelets, track),
            wings: self.wing_class_mask(facelets)?,
            parity: u32::from(rg_graph::combinatorics::is_odd(&pieces)),
            colours: self.colour_masks(facelets),
        };
        let space = Phase2Space { r: self };
        let ctl = SearchCtl::new(time_up);
        let mut path = Vec::new();
        for depth in space.h(start)..=18 {
            let mut found = None;
            search_exact(&space, start, depth, None, &mut path, &ctl, &mut |p| {
                let node = p.iter().fold(start, |n, &m| space.step(n, m));
                if self.phase3_centre_index(&node.colours).is_some() {
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
        }
        None
    }

    /// 1 when the corner and dedge permutation parities differ (dedges as positions).
    fn parity_bit(&self, facelets: &[u8]) -> Option<u32> {
        let pos = self.wing_positions(facelets)?;
        let (cp, _) =
            crate::cubie::corners_from_facelets(&rg_cube::Geometry::new(4), facelets).ok()?;
        Some(u32::from(
            rg_graph::combinatorics::is_odd(&cp) != rg_graph::combinatorics::is_odd(&pos[0]),
        ))
    }

    /// The two colours on each axis' centres, lower colour first.
    fn axis_pairs(&self, facelets: &[u8]) -> [[u8; 2]; 3] {
        let orbit = self.centres();
        std::array::from_fn(|a| {
            let mut colours: Vec<u8> = self.axis_slots[a]
                .iter()
                .map(|&s| facelets[orbit.facelets[usize::from(s)]])
                .collect();
            colours.sort_unstable();
            [colours[0], colours[7]]
        })
    }

    /// The parity bit a finished reduction must have. Reading the reduced cube in a rotated
    /// colour scheme relabels its pieces, and a rotation can permute corners and edges with
    /// different parities (a quarter turn is even on corners, odd on edges), so the target
    /// depends on which scheme the centres end up in.
    fn target_bit(&self, pairs: [[u8; 2]; 3]) -> Option<u32> {
        let consistent = |m: &[u8; 6]| {
            (0..3).all(|a| {
                let f = self.first_face[a];
                let mut got = [m[f], m[(f + 3) % 6]];
                got.sort_unstable();
                got == pairs[a]
            })
        };
        let mut bits = self
            .schemes
            .iter()
            .zip(&self.scheme_bits)
            .filter(|(m, _)| consistent(m))
            .map(|(_, &b)| b);
        let first = bits.next()?;
        debug_assert!(
            bits.all(|b| b == first),
            "schemes of one axis assignment agree"
        );
        Some(first)
    }

    /// Face-to-colour maps of every proper orientation of a solved cube. Centres can be
    /// made uniform in a *mirrored* scheme too, which no real cube can reach, so phase 3
    /// only accepts arrangements from this set.
    fn proper_schemes() -> Vec<[u8; 6]> {
        rg_cube::whole_cube_rotations(4)
            .into_iter()
            .map(|rot| {
                let mut c = Cube::new(4).expect("4x4");
                c.apply_moves(&rot);
                std::array::from_fn(|f| c.facelets()[f * 16 + 5])
            })
            .collect()
    }

    /// The face-to-colour map of a cube whose centres are uniform, or `None` otherwise.
    fn face_colours(&self, facelets: &[u8]) -> Option<[u8; 6]> {
        let orbit = self.centres();
        let mut map = [u8::MAX; 6];
        for &f in &orbit.facelets {
            let face = f / 16;
            let colour = facelets[f];
            if map[face] == u8::MAX {
                map[face] = colour;
            } else if map[face] != colour {
                return None;
            }
        }
        Some(map)
    }

    fn centres_are_proper(&self, facelets: &[u8]) -> bool {
        self.face_colours(facelets)
            .is_some_and(|m| self.schemes.contains(&m))
    }

    /// Reference colour of each axis: the lower colour id present on that axis' centres.
    fn axis_colours(&self, facelets: &[u8]) -> [u8; 3] {
        let orbit = self.centres();
        std::array::from_fn(|a| {
            self.axis_slots[a]
                .iter()
                .map(|&s| facelets[orbit.facelets[usize::from(s)]])
                .min()
                .expect("eight slots")
        })
    }

    /// Phase 3: solve every centre and pair all twelve dedges. Pairing all twelve at once
    /// outruns the heuristic (the databases saturate at 7 moves), so it runs in stages of
    /// three dedges, each stage also restoring the centres and the dedges already paired.
    pub fn phase3(&self, facelets: &[u8], time_up: &dyn Fn() -> bool) -> Option<Vec<usize>> {
        let mut cube = Cube::new(4).ok()?;
        cube.set_facelets(facelets).ok()?;
        let mut path = Vec::new();
        for stage in 0..4 {
            let found = self.phase3_stage(cube.facelets(), stage, time_up)?;
            for m in found {
                cube.apply_move(self.moves[m]);
                path.push(m);
            }
        }
        Some(path)
    }

    /// One phase-3 stage: pair `BASE_PARTITION[stage]` while restoring the centres and
    /// everything paired so far.
    pub fn phase3_stage(
        &self,
        facelets: &[u8],
        stage: usize,
        time_up: &dyn Fn() -> bool,
    ) -> Option<Vec<usize>> {
        let mut cube = Cube::new(4).ok()?;
        cube.set_facelets(facelets).ok()?;
        {
            // Every triple whose dedges are all supposed to be paired by now.
            let done: Vec<u8> = (0..=stage).flat_map(|s| BASE_PARTITION[s]).collect();
            let active: Vec<usize> = (0..self.triples.len())
                .filter(|&t| self.triples[t].iter().all(|d| done.contains(d)))
                .collect();
            let colours = self.axis_colours(cube.facelets());
            let pos = self.wing_positions(cube.facelets())?;
            // The databases aim for bit 0, so measure the bit relative to the value the
            // final colour scheme requires.
            let parity = self.parity_bit(cube.facelets())?
                ^ self.target_bit(self.axis_pairs(cube.facelets()))?;
            let node = Phase3Node {
                centres: self.centre_axes(cube.facelets(), colours),
                pos,
                parity,
            };
            let (_, class) = self.phase3_centre_index(&self.colour_masks(cube.facelets()))?;
            let space = Phase3Space {
                r: self,
                active: &active,
                class,
            };
            let ctl = SearchCtl::with_deadline(time_up);
            let mut probe = cube.clone();
            let mut buffer = Vec::new();
            let mut found = None;
            for depth in space.h(node)..=18 {
                search_exact(&space, node, depth, None, &mut buffer, &ctl, &mut |p| {
                    probe.set_facelets(cube.facelets()).expect("same size");
                    for &m in p {
                        probe.apply_move(self.moves[usize::from(m)]);
                    }
                    // Every stage must keep the centres in a scheme a real cube can show.
                    // After the last one, the result must also be a legal 3x3: that rules
                    // out the reduction parities the 3x3 stage could never fix.
                    let ok = if stage == 3 {
                        self.reduce_to_3x3(probe.facelets())
                            .is_some_and(|f| crate::cubie::CubieCube::from_facelets(&f).is_ok())
                    } else {
                        self.centres_are_proper(probe.facelets())
                    };
                    if ok {
                        found = Some(p.to_vec());
                        Flow::Stop
                    } else {
                        Flow::Continue
                    }
                });
                if found.is_some() || ctl.aborted() {
                    break;
                }
            }
            Some(found?.into_iter().map(usize::from).collect())
        }
    }

    /// Turn a reduced 4x4 (centres solved, dedges paired) into the equivalent 3x3 facelet
    /// cube, recoloured so that each centre shows its own face's colour.
    pub fn reduce_to_3x3(&self, facelets: &[u8]) -> Option<Vec<u8>> {
        let map = self.face_colours(facelets)?;
        if !self.schemes.contains(&map) {
            return None;
        }
        // Relabel colours so colour c sits on the face where the scheme puts it.
        let mut relabel = [0u8; 6];
        for (face, &colour) in map.iter().enumerate() {
            relabel[usize::from(colour)] = face as u8;
        }
        // Rows and columns 0, 1, 2 of the 3x3 sample rows and columns 0, 1, 3 of the 4x4.
        const PICK: [usize; 3] = [0, 1, 3];
        let mut out = Vec::with_capacity(54);
        for face in 0..6 {
            for &r in &PICK {
                for &c in &PICK {
                    out.push(relabel[usize::from(facelets[face * 16 + r * 4 + c])]);
                }
            }
        }
        Some(out)
    }

    /// The 4x4 turn matching a 3x3 face turn.
    pub fn lift_3x3_move(m: Move) -> Option<Move> {
        let layers = match m.layers {
            1 => 1,
            4 => 8,
            _ => return None,
        };
        Some(Move { layers, ..m })
    }

    /// Phase 1: bring one colour pair's eight centres onto the R and L faces.
    /// The table is exact, so greedy descent gives a shortest phase-1 path.
    pub fn phase1(&self, facelets: &[u8]) -> (Vec<usize>, usize) {
        let orbit = self.centres();
        let (mut d, pair) = self.phase1_distance(facelets);
        let mut mask = orbit.mask(facelets, COLOUR_PAIRS[pair]);
        let mut path = Vec::new();
        while d > 0 {
            let (m, next) = self
                .all_moves
                .iter()
                .map(|&m| (m, self.centre_bits[m].apply(mask)))
                .find(|&(_, next)| self.p1.get(mask_rank(next) as usize) == d - 1)
                .expect("a BFS layer always has a neighbour one step closer");
            path.push(m);
            mask = next;
            d -= 1;
        }
        (path, pair)
    }
}

impl Default for Reduction4 {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct Phase2Node {
    /// Which of the 16 non-R/L slots hold the tracked colour pair.
    c16: u32,
    /// Which wing slots hold a class-0 wing.
    wings: u32,
    /// Parity of the wing permutation (must be even for phase 3 to be able to pair).
    parity: u32,
    /// Which centre slots show each colour: needed to test phase 3's reachability.
    colours: [u32; 6],
}

#[derive(Clone, Copy)]
struct Phase3Node {
    /// One byte per axis: which slots show that axis' reference colour.
    centres: u32,
    /// Class-local position of each wing piece.
    pos: [[u8; 12]; 2],
    /// 1 when the corner and dedge permutation parities disagree.
    parity: u32,
}

/// Phase-3 search space: centres and edge pairing advance together. The heuristic is the
/// largest of the exact centre distance and the four three-dedge pairing databases.
struct Phase3Space<'a> {
    r: &'a Reduction4,
    /// Indices into `r.triples` that this stage must bring to "paired".
    active: &'a [usize],
    /// Which centre table holds this cube's proper goals.
    class: usize,
}

impl SearchSpace for Phase3Space<'_> {
    type Node = Phase3Node;

    fn moves(&self) -> &[u8] {
        &self.r.gens_b
    }

    #[inline]
    fn step(&self, node: Self::Node, mv: u8) -> Self::Node {
        let m = usize::from(mv);
        let step = |c: usize| {
            let s = &self.r.class_step[c][m];
            std::array::from_fn(|d| s[usize::from(node.pos[c][d])])
        };
        Phase3Node {
            centres: self.r.axis_bits[m].apply(node.centres),
            pos: [step(0), step(1)],
            parity: node.parity ^ u32::from(self.r.p3_parity_flip[m]),
        }
    }

    #[inline]
    fn h(&self, node: Self::Node) -> u8 {
        let mut best = self.r.p3_centres[self.class].get(self.r.centre_index(node.centres));
        for &t in self.active {
            best = best.max(
                self.r
                    .p3_pair
                    .get(self.r.pair_index(&node.pos, node.parity, t)),
            );
        }
        best
    }

    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
        self.r.canonical(prev, mv)
    }
}

/// Phase-2 search space: the centre placement and the wing classes advance together, with
/// the heuristic being the larger of the two exact quotient-graph distances.
struct Phase2Space<'a> {
    r: &'a Reduction4,
}

impl SearchSpace for Phase2Space<'_> {
    type Node = Phase2Node;

    fn moves(&self) -> &[u8] {
        &self.r.gens_a
    }

    #[inline]
    fn step(&self, node: Self::Node, mv: u8) -> Self::Node {
        let m = usize::from(mv);
        Phase2Node {
            c16: self.r.centre16_bits[m].apply(node.c16),
            wings: self.r.wing_bits[m].apply(node.wings),
            parity: node.parity ^ u32::from(self.r.wing_parity_flip[m]),
            colours: node.colours.map(|c| self.r.centre_bits[m].apply(c)),
        }
    }

    #[inline]
    fn h(&self, node: Self::Node) -> u8 {
        self.r.p2_centres.get(mask_rank(node.c16) as usize).max(
            self.r
                .p2_wings
                .get((mask_rank(node.wings) * 2 + node.parity) as usize),
        )
    }

    fn allowed(&self, prev: Option<u8>, mv: u8) -> bool {
        self.r.canonical(prev, mv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    /// The tables take seconds to build; every test shares one instance.
    fn shared() -> &'static Reduction4 {
        static TABLES: OnceLock<Reduction4> = OnceLock::new();
        TABLES.get_or_init(Reduction4::new)
    }

    #[test]
    fn tracked_masks_never_diverge_from_the_facelets() {
        let r = shared();
        let orbit = r.centres();
        let mut cube = Cube::new(4).unwrap();
        cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 0));
        let (mut d, pair) = r.phase1_distance(cube.facelets());
        let mut mask = orbit.mask(cube.facelets(), COLOUR_PAIRS[pair]);
        while d > 0 {
            let (m, next) = (0..r.moves.len())
                .map(|m| (m, orbit.move_mask(mask, m)))
                .find(|&(_, next)| r.p1.get(mask_rank(next) as usize) == d - 1)
                .expect("descent");
            cube.apply_move(r.moves[m]);
            (mask, d) = (next, d - 1);
            assert_eq!(
                mask,
                orbit.mask(cube.facelets(), COLOUR_PAIRS[pair]),
                "diverged at {m}"
            );
        }
        let solved = Cube::new(4).unwrap().facelets().to_vec();
        assert_eq!(mask, orbit.mask(&solved, COLOUR_PAIRS[RL_PAIR]));
    }

    #[test]
    fn phase1_puts_a_colour_pair_on_the_rl_axis() {
        let r = shared();
        // The quotient graph has C(24,8) vertices and is fully reachable.
        assert_eq!(r.phase1_histogram().iter().sum::<u64>(), 735_471);
        let depth = r.phase1_histogram().len() - 1;
        assert!(
            (5..=12).contains(&depth),
            "unexpected phase-1 diameter {depth}"
        );

        let mut total = 0;
        for seed in 0..25 {
            let mut cube = Cube::new(4).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(4, 40, seed));
            let (path, pair) = r.phase1(cube.facelets());
            total += path.len();
            for &m in &path {
                cube.apply_move(r.moves[m]);
            }
            // The centre slots of R and L (not whole faces: those also carry edges and
            // corners) now hold exactly the chosen pair's colours.
            let colours = COLOUR_PAIRS[pair];
            let orbit = r.centres();
            for (i, &f) in orbit.facelets.iter().enumerate() {
                let on_rl = (16..32).contains(&f) || (64..80).contains(&f);
                assert_eq!(
                    on_rl,
                    colours.contains(&cube.facelets()[f]),
                    "seed {seed}: centre slot {i} (facelet {f}) is on the wrong axis"
                );
            }
            assert_eq!(r.phase1_distance(cube.facelets()).0, 0);
        }
        println!("phase 1 mean length: {:.2}", total as f64 / 25.0);
    }

    #[test]
    fn phase2_reaches_its_goal() {
        let r = shared();
        println!("{}", r.report());
        // The wings split into two classes of twelve: the 4x4 "orientation" classes.
        assert_eq!(r.wing_class.iter().filter(|&&c| c == 0).count(), 12);
        assert_eq!(r.p2_centres.reached(), 12_870);
        // Both parities of each wing-class placement.
        assert_eq!(r.p2_wings.reached(), 2 * 2_704_156);

        let never = || false;
        let (mut p1_total, mut p2_total) = (0usize, 0usize);
        for seed in 0..5 {
            let mut cube = Cube::new(4).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 100 + seed));
            let (path1, pair) = r.phase1(cube.facelets());
            for &m in &path1 {
                cube.apply_move(r.moves[m]);
            }
            let path2 = r
                .phase2(cube.facelets(), pair, &never)
                .expect("phase 2 solvable");
            for &m in &path2 {
                cube.apply_move(r.moves[m]);
            }
            p1_total += path1.len();
            p2_total += path2.len();

            // Phase 1 still holds, the second pair is on U/D, and wings are in class.
            assert_eq!(
                r.phase1_distance(cube.facelets()).0,
                0,
                "seed {seed}: phase 1 broken"
            );
            let track = (0..3).find(|&p| p != pair).unwrap();
            let c = r.centre16_mask(cube.facelets(), track);
            assert_eq!(
                r.p2_centres.get(mask_rank(c) as usize),
                0,
                "seed {seed}: centres"
            );
            let w = r.wing_class_mask(cube.facelets()).unwrap();
            assert_eq!(
                w,
                r.wing_class_goal(),
                "seed {seed}: wings not in their classes"
            );
        }
        println!(
            "phase 1 mean {:.1}, phase 2 mean {:.1}",
            p1_total as f64 / 5.0,
            p2_total as f64 / 5.0
        );
    }

    #[test]
    fn phase2_always_hands_phase3_a_solvable_cube() {
        let r = shared();
        let never = || false;
        for seed in 0..10 {
            let mut cube = Cube::new(4).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 500 + seed));
            let (p1, pair) = r.phase1(cube.facelets());
            for &m in &p1 {
                cube.apply_move(r.moves[m]);
            }
            let p2 = r.phase2(cube.facelets(), pair, &never).expect("phase 2");
            for &m in &p2 {
                cube.apply_move(r.moves[m]);
            }
            assert!(
                r.phase3_feasible(cube.facelets()),
                "seed {seed}: phase 3 cannot finish"
            );
        }
    }

    #[test]
    fn reduced_cube_becomes_a_valid_3x3() {
        let r = shared();
        let never = || false;
        let mut cube = Cube::new(4).unwrap();
        cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 77));
        let (p1, pair) = r.phase1(cube.facelets());
        for &m in &p1 {
            cube.apply_move(r.moves[m]);
        }
        for &m in &r.phase2(cube.facelets(), pair, &never).unwrap() {
            cube.apply_move(r.moves[m]);
        }
        for &m in &r.phase3(cube.facelets(), &never).unwrap() {
            cube.apply_move(r.moves[m]);
        }
        let three = r.reduce_to_3x3(cube.facelets()).expect("reduced");
        // A genuine 3x3: it passes every solvability law, including parity.
        crate::cubie::CubieCube::from_facelets(&three).expect("valid 3x3");
    }

    #[test]
    fn parity_bookkeeping_matches_the_cube() {
        let r = shared();
        let geom = rg_cube::Geometry::new(4);
        let mut cube = Cube::new(4).unwrap();
        cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 9));
        let (p1, pair) = r.phase1(cube.facelets());
        for &m in &p1 {
            cube.apply_move(r.moves[m]);
        }
        for &m in &r.phase2(cube.facelets(), pair, &|| false).unwrap() {
            cube.apply_move(r.moves[m]);
        }
        let bit = |c: &Cube| {
            let (cp, _) = crate::cubie::corners_from_facelets(&geom, c.facelets()).unwrap();
            let pos = r.wing_positions(c.facelets()).unwrap();
            u32::from(
                rg_graph::combinatorics::is_odd(&cp) != rg_graph::combinatorics::is_odd(&pos[0]),
            )
        };
        // Walk a phase-3 path and check the tracked bit against the real cube each step.
        let mut expected = bit(&cube);
        for step in 0..40 {
            let m = usize::from(r.gens_b[step % r.gens_b.len()]);
            cube.apply_move(r.moves[m]);
            expected ^= u32::from(r.p3_parity_flip[m]);
            assert_eq!(
                expected,
                bit(&cube),
                "parity drifted after move {m} at step {step}"
            );
        }
    }

    #[test]
    fn wing_pairing_parity_is_invariant_under_phase3_moves() {
        let r = shared();
        let parity = |p: &[u8; 12]| rg_graph::combinatorics::is_odd(p);
        // If every phase-3 move changes both class permutations' parities together, then
        // "class 0 and class 1 have equal parity" is invariant, and phase 2 must deliver it.
        let flips: Vec<(bool, bool)> = r
            .gens_b
            .iter()
            .map(|&m| {
                let m = usize::from(m);
                (parity(&r.class_step[0][m]), parity(&r.class_step[1][m]))
            })
            .collect();
        let mixes = flips.iter().filter(|(a, b)| a != b).count();
        println!(
            "phase-3 moves whose class parities differ: {mixes} of {}",
            flips.len()
        );
        assert_eq!(mixes, 0, "expected the pairing parity to be an invariant");

        let corner_vs_dedge = r
            .gens_b
            .iter()
            .filter(|&&m| r.p3_parity_flip[usize::from(m)])
            .count();
        println!(
            "phase-3 moves that flip corner-vs-dedge parity: {corner_vs_dedge} of {}",
            r.gens_b.len()
        );
    }

    #[test]
    fn time_each_phase3_stage() {
        let r = shared();
        for seed in 0..20 {
            let mut cube = Cube::new(4).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 300 + seed));
            let (p1, pair) = r.phase1(cube.facelets());
            for &m in &p1 {
                cube.apply_move(r.moves[m]);
            }
            for &m in &r.phase2(cube.facelets(), pair, &|| false).unwrap() {
                cube.apply_move(r.moves[m]);
            }
            for stage in 0..4 {
                let t = std::time::Instant::now();
                let start = std::time::Instant::now();
                let deadline = move || start.elapsed().as_secs_f64() > 20.0;
                let path = r.phase3_stage(cube.facelets(), stage, &deadline);
                let secs = t.elapsed().as_secs_f64();
                let Some(path) = path else {
                    panic!("seed {seed} stage {stage}: failed after {secs:.2}s");
                };
                if secs > 0.5 {
                    println!(
                        "seed {seed} stage {stage}: {} moves in {secs:.2}s (slow)",
                        path.len()
                    );
                }
                for m in path {
                    cube.apply_move(r.moves[m]);
                }
            }
            let three = r.reduce_to_3x3(cube.facelets()).expect("reduced");
            crate::cubie::CubieCube::from_facelets(&three).expect("legal 3x3");
        }
    }

    #[test]
    fn phase3_solves_centres_and_pairs_edges() {
        let r = shared();
        println!("{}", r.report());
        let never = || false;
        for seed in 0..5 {
            let mut cube = Cube::new(4).unwrap();
            cube.apply_moves(&rg_cube::random_move_scramble(4, 40, 200 + seed));
            let (p1, pair) = r.phase1(cube.facelets());
            for &m in &p1 {
                cube.apply_move(r.moves[m]);
            }
            let p2 = r.phase2(cube.facelets(), pair, &never).expect("phase 2");
            for &m in &p2 {
                cube.apply_move(r.moves[m]);
            }
            let t0 = std::time::Instant::now();
            let budget = std::time::Instant::now();
            let deadline = move || budget.elapsed().as_secs_f64() > 60.0;
            let p3 = r.phase3(cube.facelets(), &deadline).expect("phase 3");
            for &m in &p3 {
                cube.apply_move(r.moves[m]);
            }
            println!(
                "seed {seed}: {} + {} + {} moves, phase 3 took {:.2}s",
                p1.len(),
                p2.len(),
                p3.len(),
                t0.elapsed().as_secs_f64()
            );

            // Every centre face is uniform and every dedge is paired.
            let f = cube.facelets();
            for face in 0..6 {
                let centres: Vec<u8> = r
                    .centres()
                    .facelets
                    .iter()
                    .filter(|&&s| s / 16 == face)
                    .map(|&s| f[s])
                    .collect();
                assert!(
                    centres.iter().all(|c| *c == centres[0]),
                    "seed {seed}: face {face}"
                );
            }
            let pos = r.wing_positions(f).unwrap();
            assert_eq!(pos[0], pos[1], "seed {seed}: dedges not paired");
        }
    }
}
