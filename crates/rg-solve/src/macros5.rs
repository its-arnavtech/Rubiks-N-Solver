//! Macro-operator discovery (Korf 1985), used by the 5x5 final phase.
//!
//! A *macro* is a short move sequence, found by search, whose net effect is small: the
//! centres end exactly where they started, every midge ends where it started, and only a
//! handful of wings move. Chaining such sequences solves the last phase without ever
//! disturbing what is already done, which is what makes it fast.
//!
//! They are found by meet-in-the-middle on the centre state. Two sequences that leave the
//! centres in the *same* state compose, one forward and the other reversed, into a sequence
//! that restores them; of those, we keep the ones that also fix every midge and move few
//! wings. Nothing here is a written-down algorithm: the sequences come out of the search,
//! and the filter is a property of the resulting permutation (rule R3, docs/01 §2).

use std::collections::HashMap;

/// A discovered macro operator.
#[derive(Clone, Debug)]
pub struct MacroOp {
    /// Move indices, in order.
    pub moves: Vec<u8>,
    /// Where the wing in each slot ends up.
    pub wing: [u8; 24],
    /// The wing slots this macro moves.
    pub support: Vec<u8>,
}

/// Everything the discovery search needs to know about the puzzle.
pub struct Discovery<'a> {
    /// The moves to search over (indices into the caller's move list).
    pub gens: &'a [u8],
    /// Per move index: the index of its inverse move.
    pub inverse: &'a [u8],
    /// Per move index: where the wing in each slot, and each midge *sticker*, ends up.
    /// Midges are tracked by sticker so that a macro cannot flip one in place.
    pub wing_step: &'a [[u8; 24]],
    pub midge_step: &'a [[u8; 24]],
    /// The packed centre state (x orbit, t orbit) after a move.
    pub centre_step: &'a dyn Fn((u32, u32), u8) -> (u32, u32),
    /// The centre state the macros must preserve (the solved one).
    pub centre_start: (u32, u32),
    /// Canonical-sequence filter, as used by the phase searches.
    pub canonical: &'a dyn Fn(Option<u8>, u8) -> bool,
    /// Half the macro length: sequences up to this many moves are enumerated, and a macro
    /// is one of them followed by another reversed, so macros are up to twice as long.
    pub half_depth: usize,
    /// Keep macros that move at most this many wings.
    pub max_support: usize,
}

/// Sequences enumerated so far, and which of them reach each centre state.
type Halves = (Vec<Half>, HashMap<(u32, u32), Vec<u32>>);

struct Half {
    moves: Vec<u8>,
    wing: [u8; 24],
    midge: [u8; 24],
}

fn compose<const N: usize>(a: &[u8; N], step: &[u8; N]) -> [u8; N] {
    std::array::from_fn(|i| step[usize::from(a[i])])
}

fn invert<const N: usize>(p: &[u8; N]) -> [u8; N] {
    let mut inv = [0u8; N];
    for (i, &q) in p.iter().enumerate() {
        inv[usize::from(q)] = i as u8;
    }
    inv
}

impl Discovery<'_> {
    /// Enumerate canonical sequences up to `half_depth` moves, grouped by centre state.
    fn halves(&self) -> Halves {
        let mut halves = Vec::new();
        let mut by_centre: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        let mut seq = Vec::new();
        self.walk(
            &std::array::from_fn(|i| i as u8),
            &std::array::from_fn(|i| i as u8),
            self.centre_start,
            &mut seq,
            &mut halves,
            &mut by_centre,
        );
        (halves, by_centre)
    }

    fn walk(
        &self,
        wing: &[u8; 24],
        midge: &[u8; 24],
        centre: (u32, u32),
        seq: &mut Vec<u8>,
        halves: &mut Vec<Half>,
        by_centre: &mut HashMap<(u32, u32), Vec<u32>>,
    ) {
        for &mv in self.gens {
            if !(self.canonical)(seq.last().copied(), mv) {
                continue;
            }
            let m = usize::from(mv);
            let wing = compose(wing, &self.wing_step[m]);
            let midge = compose(midge, &self.midge_step[m]);
            let centre = (self.centre_step)(centre, mv);
            seq.push(mv);
            by_centre
                .entry(centre)
                .or_default()
                .push(halves.len() as u32);
            halves.push(Half {
                moves: seq.clone(),
                wing,
                midge,
            });
            if seq.len() < self.half_depth {
                self.walk(&wing, &midge, centre, seq, halves, by_centre);
            }
            seq.pop();
        }
    }

    /// Every macro this search finds, shortest first, one per distinct effect on the wings.
    pub fn run(&self, group_cap: usize) -> Vec<MacroOp> {
        let (halves, by_centre) = self.halves();
        if std::env::var_os("RG_TRACE").is_some() {
            let mut sizes: Vec<usize> = by_centre.values().map(|g| g.len()).collect();
            sizes.sort_unstable();
            eprintln!(
                "  macros: {} sequences, {} centre states, largest groups {:?}",
                halves.len(),
                sizes.len(),
                &sizes[sizes.len().saturating_sub(8)..]
            );
        }
        let mut best: HashMap<[u8; 24], Vec<u8>> = HashMap::new();
        for group in by_centre.values() {
            let group = &group[..group.len().min(group_cap)];
            for (i, &a) in group.iter().enumerate() {
                let ha = &halves[a as usize];
                for &b in &group[i + 1..] {
                    let hb = &halves[b as usize];
                    let (inv_w, inv_m) = (invert(&hb.wing), invert(&hb.midge));
                    // Fixing every midge is what lets macros be chained freely: tredges
                    // already attached stay attached.
                    let midge = compose(&ha.midge, &inv_m);
                    if midge.iter().enumerate().any(|(i, &d)| usize::from(d) != i) {
                        continue;
                    }
                    let wing = compose(&ha.wing, &inv_w);
                    let support = (0..24u8).filter(|&s| wing[usize::from(s)] != s).count();
                    if support == 0 || support > self.max_support {
                        continue;
                    }
                    for (wing, first, second) in [(wing, ha, hb), (invert(&wing), hb, ha)] {
                        let mut moves = first.moves.clone();
                        moves.extend(
                            second
                                .moves
                                .iter()
                                .rev()
                                .map(|&m| self.inverse[usize::from(m)]),
                        );
                        let entry = best.entry(wing).or_insert_with(|| moves.clone());
                        if moves.len() < entry.len() {
                            *entry = moves;
                        }
                    }
                }
            }
        }
        let mut out: Vec<MacroOp> = best
            .into_iter()
            .map(|(wing, moves)| MacroOp {
                support: (0..24u8).filter(|&s| wing[usize::from(s)] != s).collect(),
                wing,
                moves,
            })
            .collect();
        out.sort_by_key(|m| (m.moves.len(), m.support.len(), m.wing));
        out
    }
}
