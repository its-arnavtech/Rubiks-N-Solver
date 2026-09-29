//! Macro discovery (ARCHITECTURE §6.2): commutators `[X, Y] = X·Y·X⁻¹·Y⁻¹` over a type's
//! local generators, evaluated on a labeled probe cube, kept when their net effect is a pure
//! 3-cycle (or twist / flip pair) inside the target orbit.

use std::collections::HashMap;

use nx_sim::{LabeledCube, Orbit, OrbitKind, SlotMap, cancel, orbits, sticker_count};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::effect::{Effect, MacroKind, OrbitProbe};
use crate::sym::{self, Binding, LayerRef, SymMove};

/// Search limits for one type. Serialized into `library.json` under `generator.search`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Probe cube size and the probe instance's indices.
    pub probe_n: u32,
    pub a: u32,
    pub b: u32,
    /// Commutator length pairs `(|X|, |Y|)`. `(k, l)` also yields `(l, k)` as inverses.
    pub pairs: Vec<(usize, usize)>,
    /// Longest setup `S` in the action expansion `S·M·S⁻¹`.
    pub setup_len: usize,
}

impl SearchConfig {
    pub fn default_for(kind: OrbitKind) -> Self {
        let std_pairs = vec![(1, 1), (1, 2), (1, 3), (2, 1), (2, 2), (3, 1)];
        let (probe_n, a, b, pairs) = match kind {
            // Core types use few generators, so a much wider search is cheap. Pure twist and
            // flip pairs need longer X.
            OrbitKind::Corner | OrbitKind::MidEdge => {
                let mut p = std_pairs;
                p.extend([(3, 2), (3, 3), (4, 1), (4, 2), (5, 1), (6, 1)]);
                (13, 0, 0, p)
            }
            OrbitKind::Wing => (12, 2, 0, std_pairs),
            OrbitKind::XCenter => (12, 2, 2, std_pairs),
            OrbitKind::PlusCenter => (13, 2, 6, std_pairs),
            OrbitKind::ObliqueA => (12, 2, 4, std_pairs),
            OrbitKind::ObliqueB => (12, 4, 2, std_pairs),
            OrbitKind::FixedCenter => panic!("FixedCenter has no macros"),
        };
        Self {
            probe_n,
            a,
            b,
            pairs,
            setup_len: 2,
        }
    }
}

/// A discovered macro: its symbolic moves (cancelled) and its slot effect at the probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub moves: Vec<SymMove>,
    pub effect: Effect,
    pub kind: MacroKind,
    pub cycle: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchStats {
    pub candidates: u64,
    pub survivors: u64,
}

/// Is this a core type, whose macros only need to preserve other core pieces?
pub fn is_core(kind: OrbitKind) -> bool {
    matches!(kind, OrbitKind::Corner | OrbitKind::MidEdge)
}

/// The labeled probe: generator permutations restricted to the stickers that matter.
pub struct Probe {
    pub kind: OrbitKind,
    pub n: u32,
    pub binding: Binding,
    pub gens: Vec<SymMove>,
    /// Sticker ids of the evaluation domain (all stickers, or core stickers for core types).
    domain: Vec<u32>,
    /// Gather permutation of each generator over domain indices.
    gen_perms: Vec<Vec<u16>>,
    /// Domain index → belongs to the target orbit.
    in_target: Vec<bool>,
    pub target: OrbitProbe,
}

impl Probe {
    pub fn new(kind: OrbitKind, cfg: &SearchConfig) -> Self {
        let n = cfg.probe_n;
        let os = orbits(n);
        let target_orbit = find_orbit(&os, kind, cfg.a, cfg.b)
            .unwrap_or_else(|| panic!("probe N={n} has no {kind} orbit ({}, {})", cfg.a, cfg.b));
        let binding = Binding::for_orbit(n, target_orbit);
        let len = sticker_count(n);
        let domain: Vec<u32> = if is_core(kind) {
            let mut d: Vec<u32> = os
                .iter()
                .filter(|o| {
                    matches!(
                        o.kind,
                        OrbitKind::Corner | OrbitKind::MidEdge | OrbitKind::FixedCenter
                    )
                })
                .flat_map(|o| o.stickers.iter().copied())
                .collect();
            d.sort_unstable();
            d
        } else {
            (0..len as u32).collect()
        };
        assert!(domain.len() < usize::from(u16::MAX));
        let mut index_of = vec![u16::MAX; len];
        for (k, &s) in domain.iter().enumerate() {
            index_of[s as usize] = k as u16;
        }
        let gens = SymMove::generators(LayerRef::for_kind(kind));
        let gen_perms = gens
            .iter()
            .map(|g| {
                let mut c = LabeledCube::solved(n);
                c.apply(g.bind(binding));
                domain
                    .iter()
                    .map(|&s| index_of[c.facelets()[s as usize] as usize])
                    .collect()
            })
            .collect();
        let target = OrbitProbe::new(SlotMap::for_orbit(n, target_orbit), len);
        let in_target = domain
            .iter()
            .map(|&s| target.contains(s as usize))
            .collect();
        Self {
            kind,
            n,
            binding,
            gens,
            domain,
            gen_perms,
            in_target,
            target,
        }
    }

    fn compose(&self, seq: &[u16]) -> Vec<u16> {
        let mut g: Vec<u16> = (0..self.domain.len() as u16).collect();
        for &i in seq {
            let p = &self.gen_perms[usize::from(i)];
            g = p.iter().map(|&j| g[usize::from(j)]).collect();
        }
        g
    }

    /// Slot effect on the target orbit of a gather permutation over the domain.
    fn effect_of(&self, perm: &[u16]) -> Option<Effect> {
        let mut facelets: Vec<u32> = (0..sticker_count(self.n) as u32).collect();
        for (k, &src) in perm.iter().enumerate() {
            facelets[self.domain[k] as usize] = self.domain[usize::from(src)];
        }
        self.target.effect(&facelets)
    }

    /// Slot effect of a symbolic sequence on the target orbit (sequence need not be pure).
    pub fn effect_of_seq(&self, seq: &[SymMove]) -> Effect {
        let idx: Vec<u16> = seq
            .iter()
            .map(|m| {
                self.gens
                    .iter()
                    .position(|g| g == m)
                    .expect("generator of this type") as u16
            })
            .collect();
        self.effect_of(&self.compose(&idx))
            .expect("moves keep the orbit closed")
    }
}

fn find_orbit(os: &[Orbit], kind: OrbitKind, a: u32, b: u32) -> Option<&Orbit> {
    os.iter().find(|o| o.kind == kind && (o.a, o.b) == (a, b))
}

/// Canonical generator sequences of length `len`: no two adjacent moves on one layer, and
/// adjacent same-axis moves in increasing layer-ref order (they commute).
pub fn sequences(gens: &[SymMove], len: usize) -> Vec<Vec<u16>> {
    let mut out = vec![vec![]];
    for _ in 0..len {
        let mut next = Vec::with_capacity(out.len() * gens.len());
        for s in &out {
            for (i, g) in gens.iter().enumerate() {
                if let Some(&last) = s.last() {
                    let p = gens[usize::from(last)];
                    if p.axis == g.axis && p.layer >= g.layer {
                        continue;
                    }
                }
                let mut t = s.clone();
                t.push(i as u16);
                next.push(t);
            }
        }
        out = next;
    }
    out
}

fn inverse_perm(p: &[u16]) -> Vec<u16> {
    let mut inv = vec![0u16; p.len()];
    for (i, &v) in p.iter().enumerate() {
        inv[usize::from(v)] = i as u16;
    }
    inv
}

/// Largest number of stickers a kept macro may move (a corner 3-cycle).
const MAX_MOVED: usize = 9;

/// Search one type. Returns the shortest macro per distinct slot effect, sorted by
/// (length, moves), plus counters.
pub fn discover(kind: OrbitKind, cfg: &SearchConfig) -> (Vec<Found>, SearchStats) {
    let probe = Probe::new(kind, cfg);
    let gens = &probe.gens;
    let max_len = cfg.pairs.iter().map(|&(x, y)| x.max(y)).max().unwrap_or(0);
    let seqs: Vec<Vec<Vec<u16>>> = (0..=max_len).map(|l| sequences(gens, l)).collect();
    let mut best: HashMap<Effect, Found> = HashMap::new();
    let mut stats = SearchStats::default();
    for &(lx, ly) in &cfg.pairs {
        // (k, l) with k < l is the inverse of (l, k); evaluate it only if (l, k) is absent.
        if lx < ly && cfg.pairs.contains(&(ly, lx)) {
            continue;
        }
        let ys: Vec<(Vec<u16>, Vec<u16>)> = seqs[ly]
            .iter()
            .map(|y| {
                let p = probe.compose(y);
                let inv = inverse_perm(&p);
                (p, inv)
            })
            .collect();
        let results: Vec<(u64, Vec<Found>)> = seqs[lx]
            .par_iter()
            .map(|x| {
                let xp = probe.compose(x);
                let xi = inverse_perm(&xp);
                let x_axis = gens[usize::from(x[0])].axis;
                let x_one_axis = x.iter().all(|&i| gens[usize::from(i)].axis == x_axis);
                let mut found = Vec::new();
                let mut count = 0u64;
                for (yseq, (yp, yi)) in seqs[ly].iter().zip(&ys) {
                    if x_one_axis && yseq.iter().all(|&i| gens[usize::from(i)].axis == x_axis) {
                        continue; // everything commutes
                    }
                    count += 1;
                    if let Some(f) = evaluate(&probe, x, &xp, &xi, yseq, yp, yi) {
                        found.extend(f);
                    }
                }
                (count, found)
            })
            .collect();
        for (count, found) in results {
            stats.candidates += count;
            stats.survivors += found.len() as u64;
            for f in found {
                keep_best(&mut best, f);
            }
        }
    }
    let m = probe.target.orientation_mod();
    if m > 1 && !best.values().any(|f| f.kind == MacroKind::OrientPair) {
        // No pure orientation pair is a short commutator (corners: R, U, F have no opposite
        // pair). Products of two pure 3-cycle macros are pure too; keep those that are pairs.
        let cycles: Vec<Found> = best.values().cloned().collect();
        let composed: Vec<Found> = cycles
            .par_iter()
            .flat_map_iter(|f1| {
                cycles.iter().filter_map(move |f2| {
                    let effect = f1.effect.then(&f2.effect, m);
                    let (kind, cycle) = effect.classify()?;
                    (kind == MacroKind::OrientPair).then(|| {
                        let mut s = f1.moves.clone();
                        s.extend_from_slice(&f2.moves);
                        Found {
                            moves: cancel(&s),
                            effect,
                            kind,
                            cycle,
                        }
                    })
                })
            })
            .collect();
        stats.survivors += composed.len() as u64;
        for f in composed {
            keep_best(&mut best, f);
        }
    }
    let mut out: Vec<Found> = best.into_values().collect();
    out.sort_by(|a, b| (a.moves.len(), &a.moves).cmp(&(b.moves.len(), &b.moves)));
    (out, stats)
}

fn keep_best(best: &mut HashMap<Effect, Found>, f: Found) {
    match best.get(&f.effect) {
        Some(old) if (old.moves.len(), &old.moves) <= (f.moves.len(), &f.moves) => {}
        _ => {
            best.insert(f.effect.clone(), f);
        }
    }
}

/// Evaluate `[X, Y]`; on success return it and its inverse `[Y, X]`.
fn evaluate(
    probe: &Probe,
    x: &[u16],
    xp: &[u16],
    xi: &[u16],
    y: &[u16],
    yp: &[u16],
    yi: &[u16],
) -> Option<[Found; 2]> {
    // Gather of X·Y·X⁻¹·Y⁻¹ is xp[yp[xi[yi[j]]]]; bail out as soon as too much moves.
    let len = xp.len();
    let mut moved = 0;
    for j in 0..len {
        let c = xp[usize::from(yp[usize::from(xi[usize::from(yi[j])])])];
        if usize::from(c) != j {
            if !probe.in_target[j] {
                return None;
            }
            moved += 1;
            if moved > MAX_MOVED {
                return None;
            }
        }
    }
    if moved == 0 {
        return None;
    }
    let perm: Vec<u16> = (0..len)
        .map(|j| xp[usize::from(yp[usize::from(xi[usize::from(yi[j])])])])
        .collect();
    let effect = probe.effect_of(&perm)?;
    let (kind, cycle) = effect.classify()?;
    let to_sym = |s: &[u16]| {
        s.iter()
            .map(|&i| probe.gens[usize::from(i)])
            .collect::<Vec<_>>()
    };
    let (xs, ys) = (to_sym(x), to_sym(y));
    let comm = |a: &[SymMove], b: &[SymMove]| {
        let mut s = a.to_vec();
        s.extend_from_slice(b);
        s.extend(sym::invert(a));
        s.extend(sym::invert(b));
        cancel(&s)
    };
    let m = probe.target.orientation_mod();
    let inv = effect.inverse(m);
    let (_, inv_cycle) = inv.classify()?;
    Some([
        Found {
            moves: comm(&xs, &ys),
            effect,
            kind,
            cycle,
        },
        Found {
            moves: comm(&ys, &xs),
            effect: inv,
            kind,
            cycle: inv_cycle,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_are_canonical() {
        let gens = SymMove::generators(&[LayerRef::Outer, LayerRef::A]);
        assert_eq!(sequences(&gens, 1).len(), 18);
        let two = sequences(&gens, 2);
        // First move on (axis, OUTER): 3 same-axis A moves or 12 other-axis moves follow.
        // First move on (axis, A): only the 12 other-axis moves follow.
        assert_eq!(two.len(), 9 * 15 + 9 * 12);
    }

    #[test]
    fn found_macros_reproduce_their_effect() {
        let cfg = SearchConfig {
            pairs: vec![(3, 1)],
            ..SearchConfig::default_for(OrbitKind::XCenter)
        };
        let (found, stats) = discover(OrbitKind::XCenter, &cfg);
        assert!(stats.candidates > 0);
        assert!(!found.is_empty());
        let probe = Probe::new(OrbitKind::XCenter, &cfg);
        for f in found.iter().take(50) {
            assert_eq!(probe.effect_of_seq(&f.moves), f.effect);
            assert_eq!(f.kind, MacroKind::ThreeCycle);
        }
    }
}
