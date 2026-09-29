//! Kociemba's two-phase algorithm with the small table tier (docs/07 §4).
//!
//! Phase 1 searches the Schreier coset graph of G1 = ⟨U, D, R2, L2, F2, B2⟩ (2.2·10⁹
//! vertices); phase 2 searches G1 itself (1.95·10¹⁰ vertices). Neither graph is stored:
//! both are walked with IDA*, guided by exact BFS distances in four ~1M-vertex quotient
//! graphs. Phase-1 paths are enumerated in increasing length and each is completed by a
//! bounded phase-2 search, keeping the shortest total found before the deadline.

use rg_graph::bfs::DistanceTable;
use rg_graph::ida::{Flow, SearchCtl, SearchSpace, ida_star, search_exact};

use crate::cubie::CubieCube;

pub const N_MOVES: usize = 18;
const N_TWIST: usize = 2187;
const N_FLIP: usize = 2048;
const N_SLICE: usize = 495;
const N_PERM8: usize = 40320;
const N_SPERM: usize = 24;
const MAX_PHASE1: u8 = 20;
const MAX_PHASE2: u8 = 18;

/// Move ids: face (U R F D L B) × 3 + (0: quarter, 1: half, 2: inverse).
const ALL_MOVES: [u8; 18] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];
/// U, U2, U', R2, F2, D, D2, D', L2, B2: the generators of G1.
pub const PHASE2_MOVES: [u8; 10] = [0, 1, 2, 4, 7, 9, 10, 11, 13, 16];

fn is_phase2(m: u8) -> bool {
    PHASE2_MOVES.contains(&m)
}

/// Canonical sequences: never the same face twice; opposite faces only in U-before-D order.
fn canonical(prev: Option<u8>, m: u8) -> bool {
    match prev {
        None => true,
        Some(p) => {
            let (pf, f) = (p / 3, m / 3);
            pf != f && pf != f + 3
        }
    }
}

fn move_table(size: usize, moves: &[u8], f: impl Fn(usize, u8) -> u16) -> Vec<u16> {
    let mut t = vec![0u16; size * N_MOVES];
    for c in 0..size {
        for &m in moves {
            t[c * N_MOVES + usize::from(m)] = f(c, m);
        }
    }
    t
}

pub struct Kociemba {
    cubies: Vec<CubieCube>,
    twist_mv: Vec<u16>,
    flip_mv: Vec<u16>,
    slice_mv: Vec<u16>,
    cperm_mv: Vec<u16>,
    ud_mv: Vec<u16>,
    sperm_mv: Vec<u16>,
    twist_slice: DistanceTable,
    flip_slice: DistanceTable,
    cperm_sperm: DistanceTable,
    ud_sperm: DistanceTable,
}

pub struct TwoPhaseResult {
    pub moves: Vec<u8>,
    pub phase1_len: usize,
    pub nodes: u64,
}

impl Kociemba {
    /// Build move and pruning tables. `cubies` are the 18 move effects in id order.
    pub fn new(cubies: Vec<CubieCube>) -> Self {
        assert_eq!(cubies.len(), N_MOVES);
        let apply = |c: CubieCube, m: u8| c.mul(&cubies[usize::from(m)]);
        let with = |f: &dyn Fn(&mut CubieCube)| {
            let mut c = CubieCube::SOLVED;
            f(&mut c);
            c
        };

        let twist_mv = move_table(N_TWIST, &ALL_MOVES, |t, m| {
            apply(with(&|c| c.set_twist(t as u16)), m).twist()
        });
        let flip_mv = move_table(N_FLIP, &ALL_MOVES, |f, m| {
            apply(with(&|c| c.set_flip(f as u16)), m).flip()
        });
        let slice_mv = move_table(N_SLICE, &ALL_MOVES, |s, m| {
            apply(with(&|c| c.set_slice(s as u16)), m).slice()
        });
        let cperm_mv = move_table(N_PERM8, &PHASE2_MOVES, |p, m| {
            apply(with(&|c| c.set_corner_perm(p as u16)), m).corner_perm()
        });
        let ud_mv = move_table(N_PERM8, &PHASE2_MOVES, |p, m| {
            apply(with(&|c| c.set_ud_edges(p as u16)), m).ud_edges()
        });
        let sperm_mv = move_table(N_SPERM, &PHASE2_MOVES, |p, m| {
            u16::from(apply(with(&|c| c.set_slice_perm(p as u8)), m).slice_perm())
        });

        let slice_goal = u32::from(CubieCube::SOLVED.slice());
        let pair = |a: &[u16], b: &[u16], nb: usize, moves: &[u8]| {
            let (a, b) = (a.to_vec(), b.to_vec());
            let moves = moves.to_vec();
            move |v: u32, k: usize| {
                let (x, y) = (v as usize / nb, v as usize % nb);
                let m = usize::from(moves[k]);
                u32::from(a[x * N_MOVES + m]) * nb as u32 + u32::from(b[y * N_MOVES + m])
            }
        };
        let twist_slice = DistanceTable::build(
            N_TWIST * N_SLICE,
            &[slice_goal],
            N_MOVES,
            pair(&twist_mv, &slice_mv, N_SLICE, &ALL_MOVES),
        );
        let flip_slice = DistanceTable::build(
            N_FLIP * N_SLICE,
            &[slice_goal],
            N_MOVES,
            pair(&flip_mv, &slice_mv, N_SLICE, &ALL_MOVES),
        );
        let cperm_sperm = DistanceTable::build(
            N_PERM8 * N_SPERM,
            &[0],
            PHASE2_MOVES.len(),
            pair(&cperm_mv, &sperm_mv, N_SPERM, &PHASE2_MOVES),
        );
        let ud_sperm = DistanceTable::build(
            N_PERM8 * N_SPERM,
            &[0],
            PHASE2_MOVES.len(),
            pair(&ud_mv, &sperm_mv, N_SPERM, &PHASE2_MOVES),
        );
        Self {
            cubies,
            twist_mv,
            flip_mv,
            slice_mv,
            cperm_mv,
            ud_mv,
            sperm_mv,
            twist_slice,
            flip_slice,
            cperm_sperm,
            ud_sperm,
        }
    }

    /// Layer histograms of the four quotient graphs (for tests and visualization).
    pub fn table_histograms(&self) -> [&[u64]; 4] {
        [
            self.twist_slice.histogram(),
            self.flip_slice.histogram(),
            self.cperm_sperm.histogram(),
            self.ud_sperm.histogram(),
        ]
    }

    /// Anytime two-phase search. Stops at the first total ≤ `target_length`, or once
    /// `time_up()` reports the budget spent (never before a first solution exists).
    pub fn solve(
        &self,
        start: &CubieCube,
        target_length: usize,
        time_up: &dyn Fn() -> bool,
    ) -> TwoPhaseResult {
        let p1 = Phase1(self);
        let p2 = Phase2(self);
        let ctl = SearchCtl::new(time_up);
        let n1 = [start.twist(), start.flip(), start.slice()];
        let mut best: Option<(Vec<u8>, usize)> = None;
        let mut path1 = Vec::new();

        for d1 in p1.h(n1)..=MAX_PHASE1 {
            if best
                .as_ref()
                .is_some_and(|(b, _)| b.len() <= usize::from(d1))
            {
                break;
            }
            let mut on_phase1 = |p: &[u8]| -> Flow {
                // A phase-1 path ending in a G1 move reaches G1 one move earlier as well.
                if p.last().is_some_and(|&m| is_phase2(m)) {
                    return Flow::Continue;
                }
                let best_len = best.as_ref().map_or(usize::MAX, |(b, _)| b.len());
                let Some(budget) = best_len.checked_sub(p.len() + 1) else {
                    return Flow::Stop;
                };
                let c = p
                    .iter()
                    .fold(*start, |c, &m| c.mul(&self.cubies[usize::from(m)]));
                let n2 = [c.corner_perm(), c.ud_edges(), u16::from(c.slice_perm())];
                let max2 = budget.min(usize::from(MAX_PHASE2)) as u8;
                if p2.h(n2) <= max2 {
                    if let Some(q) = ida_star(&p2, n2, max2, p.last().copied(), &ctl) {
                        best = Some(([p, &q].concat(), p.len()));
                        ctl.arm();
                    }
                }
                let done = best.as_ref().is_some_and(|(b, _)| b.len() <= target_length);
                if done || ctl.aborted() {
                    Flow::Stop
                } else {
                    Flow::Continue
                }
            };
            if search_exact(&p1, n1, d1, None, &mut path1, &ctl, &mut on_phase1) == Flow::Stop {
                break;
            }
        }
        let (moves, phase1_len) = best.expect("phase 2 is always solvable within 18 moves");
        TwoPhaseResult {
            moves,
            phase1_len,
            nodes: ctl.nodes(),
        }
    }
}

struct Phase1<'a>(&'a Kociemba);

impl SearchSpace for Phase1<'_> {
    type Node = [u16; 3];

    fn moves(&self) -> &[u8] {
        &ALL_MOVES
    }

    #[inline]
    fn step(&self, [t, f, s]: [u16; 3], m: u8) -> [u16; 3] {
        let k = &self.0;
        let m = usize::from(m);
        [
            k.twist_mv[usize::from(t) * N_MOVES + m],
            k.flip_mv[usize::from(f) * N_MOVES + m],
            k.slice_mv[usize::from(s) * N_MOVES + m],
        ]
    }

    #[inline]
    fn h(&self, [t, f, s]: [u16; 3]) -> u8 {
        let k = &self.0;
        let s = usize::from(s);
        k.twist_slice
            .get(usize::from(t) * N_SLICE + s)
            .max(k.flip_slice.get(usize::from(f) * N_SLICE + s))
    }

    fn allowed(&self, prev: Option<u8>, m: u8) -> bool {
        canonical(prev, m)
    }
}

struct Phase2<'a>(&'a Kociemba);

impl SearchSpace for Phase2<'_> {
    type Node = [u16; 3];

    fn moves(&self) -> &[u8] {
        &PHASE2_MOVES
    }

    #[inline]
    fn step(&self, [c, e, s]: [u16; 3], m: u8) -> [u16; 3] {
        let k = &self.0;
        let m = usize::from(m);
        [
            k.cperm_mv[usize::from(c) * N_MOVES + m],
            k.ud_mv[usize::from(e) * N_MOVES + m],
            k.sperm_mv[usize::from(s) * N_MOVES + m],
        ]
    }

    #[inline]
    fn h(&self, [c, e, s]: [u16; 3]) -> u8 {
        let k = &self.0;
        let s = usize::from(s);
        k.cperm_sperm
            .get(usize::from(c) * N_SPERM + s)
            .max(k.ud_sperm.get(usize::from(e) * N_SPERM + s))
    }

    fn allowed(&self, prev: Option<u8>, m: u8) -> bool {
        canonical(prev, m)
    }
}
