//! Baseline orbit solver in orbit space (ARCHITECTURE §7): cycle-sort with library 3-cycles,
//! then orientation pairs; color types first choose a target assignment.

use std::collections::HashMap;

use nx_macro::{Effect, Library, MACRO_KINDS, MacroKind, SymMove, TypeLibrary};
use nx_sim::OrbitKind;
use nx_sim::slots::FIXED_CORNER_SLOT;

/// One action, ready to simulate and emit.
#[derive(Clone, Debug)]
pub struct ActionInfo {
    pub id: u32,
    pub effect: Effect,
    pub moves: Vec<SymMove>,
    pub cost: u32,
}

/// A type's actions with lookups for the baseline.
#[derive(Clone, Debug)]
pub struct KindLib {
    pub kind: OrbitKind,
    pub m: u8,
    pub actions: Vec<ActionInfo>,
    /// Directed 3-cycle `(a → b → c)`, rotated so `a` is smallest → cheapest action.
    cycles: HashMap<[u8; 3], usize>,
    /// `(slot a, slot b, delta at a)` of an orientation pair → cheapest action.
    pairs: HashMap<(u8, u8, u8), usize>,
}

fn norm_cycle(c: [u8; 3]) -> [u8; 3] {
    let r = (0..3).min_by_key(|&i| c[i]).expect("three slots");
    [c[r], c[(r + 1) % 3], c[(r + 2) % 3]]
}

impl KindLib {
    pub fn new(kind: OrbitKind, t: &TypeLibrary) -> Result<Self, String> {
        let mut actions = Vec::with_capacity(t.actions.len());
        let mut cycles: HashMap<[u8; 3], usize> = HashMap::new();
        let mut pairs: HashMap<(u8, u8, u8), usize> = HashMap::new();
        for a in &t.actions {
            let effect = TypeLibrary::effect(a);
            let moves = t.action_moves(a)?;
            let idx = actions.len();
            let cheaper = |old: &usize, acts: &[ActionInfo]| a.cost < acts[*old].cost;
            match effect.classify() {
                Some((MacroKind::ThreeCycle, c)) => {
                    let key = norm_cycle([c[0], c[1], c[2]]);
                    if cycles.get(&key).is_none_or(|o| cheaper(o, &actions)) {
                        cycles.insert(key, idx);
                    }
                }
                Some((MacroKind::OrientPair, c)) => {
                    for (x, y) in [(c[0], c[1]), (c[1], c[0])] {
                        let key = (x, y, effect.ori[usize::from(x)]);
                        if pairs.get(&key).is_none_or(|o| cheaper(o, &actions)) {
                            pairs.insert(key, idx);
                        }
                    }
                }
                None => return Err(format!("{kind} action {} is not a 3-cycle or pair", a.id)),
            }
            actions.push(ActionInfo {
                id: a.id,
                effect,
                moves,
                cost: a.cost,
            });
        }
        Ok(Self {
            kind,
            m: t.orientation_mod,
            actions,
            cycles,
            pairs,
        })
    }

    fn cycle(&self, c: [u8; 3]) -> Option<usize> {
        self.cycles.get(&norm_cycle(c)).copied()
    }

    /// Actions (in order) that realize the directed 3-cycle `x → y → z`, directly or as
    /// `(x → y → w)` then `(w → z → x)` when the library lacks the cycle itself.
    fn three_cycle(&self, x: u8, y: u8, z: u8) -> Option<Vec<usize>> {
        if let Some(a) = self.cycle([x, y, z]) {
            return Some(vec![a]);
        }
        let slots = self.kind.slot_count() as u8;
        (0..slots)
            .filter(|&w| ![x, y, z].contains(&w) && !self.is_fixed(w))
            .find_map(|w| Some(vec![self.cycle([x, y, w])?, self.cycle([w, z, x])?]))
    }

    fn is_fixed(&self, slot: u8) -> bool {
        self.kind == OrbitKind::Corner && usize::from(slot) == FIXED_CORNER_SLOT
    }

    fn apply(&self, state: &mut Vec<u8>, action: usize) {
        *state = self.actions[action].effect.apply(state, self.m);
    }

    /// Solve distinguishable contents (`piece·m + ori`; piece = home slot). The permutation
    /// must be even. Returns action indices into `self.actions`.
    pub fn solve_pieces(&self, content: &[u8]) -> Result<Vec<usize>, String> {
        let m = self.m;
        let mut state = content.to_vec();
        let mut plan = Vec::new();
        let piece = |s: &[u8], i: usize| s[i] / m;
        let limit = 4 * state.len() + 8;
        for _ in 0..limit {
            let misplaced: Vec<u8> = (0..state.len() as u8)
                .filter(|&i| piece(&state, usize::from(i)) != i)
                .collect();
            let Some(&a) = misplaced.first() else { break };
            let p1 = piece(&state, usize::from(a));
            let p2 = piece(&state, usize::from(p1));
            let (x, y, z) = if p2 != a {
                (a, p1, p2)
            } else {
                let d = *misplaced
                    .iter()
                    .find(|&&d| d != a && d != p1)
                    .ok_or_else(|| format!("{}: odd permutation", self.kind))?;
                (a, p1, d)
            };
            let acts = self
                .three_cycle(x, y, z)
                .ok_or_else(|| format!("{}: no actions for 3-cycle {x}->{y}->{z}", self.kind))?;
            for act in acts {
                self.apply(&mut state, act);
                plan.push(act);
            }
        }
        if m > 1 {
            for i in 0..state.len() as u8 {
                let o = state[usize::from(i)] % m;
                if o == 0 {
                    continue;
                }
                // Pair slot i with the next slot that is also turned, else any movable slot.
                let j = (i + 1..state.len() as u8)
                    .find(|&j| state[usize::from(j)] % m != 0)
                    .or_else(|| (0..state.len() as u8).find(|&j| j != i && !self.is_fixed(j)))
                    .ok_or_else(|| format!("{}: no partner slot", self.kind))?;
                let act = *self
                    .pairs
                    .get(&(i, j, (m - o) % m))
                    .ok_or_else(|| format!("{}: no orientation pair ({i}, {j})", self.kind))?;
                self.apply(&mut state, act);
                plan.push(act);
            }
        }
        let solved = state
            .iter()
            .enumerate()
            .all(|(i, &x)| usize::from(x) == i * usize::from(m));
        if !solved {
            return Err(format!("{}: baseline did not solve the orbit", self.kind));
        }
        Ok(plan)
    }

    /// Solve color contents (24 slots, 4 per face color; home color of slot i is i / 4).
    pub fn solve_colors(&self, colors: &[u8]) -> Result<Vec<usize>, String> {
        let labels = assign_targets(colors);
        self.solve_pieces(&labels)
    }
}

/// Target assignment for a color orbit: each slot's piece gets a target slot of its color,
/// pieces already home stay, and the permutation is made even by swapping the targets of two
/// same-colored pieces. Returns `target[slot]`, usable as distinguishable piece ids.
pub fn assign_targets(colors: &[u8]) -> Vec<u8> {
    let slots = colors.len();
    let home = |i: usize| (i / 4) as u8;
    let mut target = vec![u8::MAX; slots];
    let mut free: Vec<Vec<u8>> = vec![Vec::new(); 6];
    for i in 0..slots {
        if colors[i] == home(i) {
            target[i] = i as u8;
        } else {
            free[usize::from(home(i))].push(i as u8);
        }
    }
    for i in 0..slots {
        if target[i] == u8::MAX {
            // Prefer a free home slot whose own piece wants to come here (a 2-cycle).
            let c = usize::from(colors[i]);
            let pick = free[c]
                .iter()
                .position(|&t| colors[usize::from(t)] == home(i))
                .unwrap_or(0);
            target[i] = free[c].remove(pick);
        }
    }
    if nx_sim::identity::permutation_parity(&target) {
        // Swap the targets of two same-colored pieces, preferring two that are not home.
        let same = |i: usize, j: usize| colors[i] == colors[j];
        let not_home = |i: usize| usize::from(target[i]) != i;
        let pair = (0..slots)
            .flat_map(|i| (i + 1..slots).map(move |j| (i, j)))
            .filter(|&(i, j)| same(i, j))
            .max_by_key(|&(i, j)| {
                (
                    u8::from(not_home(i)) + u8::from(not_home(j)),
                    std::cmp::Reverse((i, j)),
                )
            })
            .expect("four pieces per color");
        target.swap(pair.0, pair.1);
    }
    target
}

/// Every type's lookups.
#[derive(Clone, Debug)]
pub struct SolverLib {
    pub sha256: String,
    kinds: Vec<KindLib>,
}

impl SolverLib {
    pub fn new(lib: &Library) -> Result<Self, String> {
        let kinds = MACRO_KINDS
            .iter()
            .map(|&k| {
                let t = lib.get(k).ok_or_else(|| format!("library has no {k}"))?;
                KindLib::new(k, t)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            sha256: lib.sha256.clone(),
            kinds,
        })
    }

    pub fn kind(&self, k: OrbitKind) -> &KindLib {
        self.kinds
            .iter()
            .find(|x| x.kind == k)
            .expect("every macro kind is loaded")
    }

    /// Baseline plan for one orbit's content.
    pub fn plan(&self, k: OrbitKind, content: &[u8]) -> Result<Vec<usize>, String> {
        let kl = self.kind(k);
        if k.is_center() {
            kl.solve_colors(content)
        } else {
            kl.solve_pieces(content)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_is_even_color_preserving_and_keeps_home_pieces() {
        let mut r = nx_sim::rng::rng(5);
        for _ in 0..500 {
            let mut colors: Vec<u8> = (0..24).map(|i| i / 4).collect();
            nx_sim::rng::shuffle(&mut r, &mut colors);
            let t = assign_targets(&colors);
            let mut sorted = t.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..24).collect::<Vec<u8>>());
            assert!(!nx_sim::identity::permutation_parity(&t));
            for i in 0..24 {
                assert_eq!(colors[i], t[i] / 4, "target has the piece's color");
            }
        }
    }
}
