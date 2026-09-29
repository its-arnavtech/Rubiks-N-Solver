//! Library verification (ARCHITECTURE §6.3): purity and invariance of every action at every
//! instance of its type for N = min..=18 plus spot checks, and coverage.

use std::time::Instant;

use nx_sim::rng::{below, rng};
use nx_sim::{LabeledCube, Move, Orbit, OrbitKind, SlotMap, orbits, sticker_count};
use rayon::prelude::*;

use crate::discover::is_core;
use crate::effect::{Effect, MacroKind, OrbitProbe};
use crate::library::{Library, TypeLibrary};
use crate::slots_fixed_corner;
use crate::sym::{self, Binding};

#[derive(Clone, Debug)]
pub struct VerifyOptions {
    /// Every instance of every N in `min_n..=max_full_n` is checked.
    pub max_full_n: u32,
    /// Sizes where a sample of instances is checked.
    pub spot_ns: Vec<u32>,
    /// Instances sampled per type at each spot size.
    pub spot_samples: usize,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            max_full_n: 18,
            spot_ns: vec![31, 64, 101],
            spot_samples: 10,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypeReport {
    pub kind: OrbitKind,
    pub actions: usize,
    pub sizes: Vec<u32>,
    pub instances: usize,
    pub checks: u64,
    pub failures: Vec<String>,
    pub seconds: f64,
}

/// Smallest N on which a type exists.
pub fn min_n(kind: OrbitKind) -> u32 {
    match kind {
        OrbitKind::Corner => 2,
        OrbitKind::MidEdge | OrbitKind::FixedCenter => 3,
        OrbitKind::Wing | OrbitKind::XCenter => 4,
        OrbitKind::PlusCenter => 5,
        OrbitKind::ObliqueA | OrbitKind::ObliqueB => 6,
    }
}

/// Does the type exist at N?
pub fn exists_at(kind: OrbitKind, n: u32) -> bool {
    let odd_only = matches!(
        kind,
        OrbitKind::MidEdge | OrbitKind::PlusCenter | OrbitKind::FixedCenter
    );
    n >= min_n(kind) && (!odd_only || n % 2 == 1)
}

/// Verify every type. Failures are collected, never hidden.
pub fn verify(lib: &Library, opts: &VerifyOptions) -> Vec<TypeReport> {
    lib.types
        .iter()
        .map(|(name, t)| match t.kind(name) {
            Some(kind) => verify_type(kind, t, opts),
            None => TypeReport {
                kind: OrbitKind::FixedCenter,
                actions: 0,
                sizes: vec![],
                instances: 0,
                checks: 0,
                failures: vec![format!("unknown type {name} / id {}", t.type_id)],
                seconds: 0.0,
            },
        })
        .collect()
}

fn verify_type(kind: OrbitKind, t: &TypeLibrary, opts: &VerifyOptions) -> TypeReport {
    let start = Instant::now();
    let mut failures = Vec::new();
    let mut actions: Vec<(u32, Vec<sym::SymMove>, Effect)> = Vec::new();
    for a in &t.actions {
        match t.action_moves(a) {
            Ok(moves) => {
                if moves.len() as u32 != a.cost {
                    failures.push(format!(
                        "action {}: cost {} but {} moves",
                        a.id,
                        a.cost,
                        moves.len()
                    ));
                }
                let e = TypeLibrary::effect(a);
                if e.perm.len() != kind.slot_count() || e.classify().is_none() {
                    failures.push(format!("action {}: effect is not a 3-cycle or pair", a.id));
                }
                actions.push((a.id, moves, e));
            }
            Err(e) => failures.push(e),
        }
    }
    failures.extend(coverage(kind, t));

    let mut sizes: Vec<u32> = (min_n(kind)..=opts.max_full_n)
        .filter(|&n| exists_at(kind, n))
        .collect();
    let spot: Vec<u32> = opts
        .spot_ns
        .iter()
        .copied()
        .filter(|&n| exists_at(kind, n))
        .collect();
    sizes.extend(&spot);
    let mut instances = 0;
    let mut checks = 0u64;
    for &n in &sizes {
        let os: Vec<Orbit> = orbits(n).into_iter().filter(|o| o.kind == kind).collect();
        let chosen = if n > opts.max_full_n {
            sample(&os, opts.spot_samples, n)
        } else {
            os
        };
        for orbit in &chosen {
            instances += 1;
            checks += actions.len() as u64;
            failures.extend(check_instance(kind, n, orbit, &actions));
        }
    }
    TypeReport {
        kind,
        actions: t.actions.len(),
        sizes,
        instances,
        checks,
        failures,
        seconds: start.elapsed().as_secs_f64(),
    }
}

/// First three, last three and a few seeded random instances.
fn sample(os: &[Orbit], k: usize, n: u32) -> Vec<Orbit> {
    if os.len() <= k {
        return os.to_vec();
    }
    let mut idx: Vec<usize> = (0..3).chain(os.len() - 3..os.len()).collect();
    let mut r = rng(u64::from(n));
    while idx.len() < k {
        idx.push(below(&mut r, os.len() as u64) as usize);
    }
    idx.sort_unstable();
    idx.dedup();
    idx.into_iter().map(|i| os[i].clone()).collect()
}

/// Purity and invariance of every action at one instance.
fn check_instance(
    kind: OrbitKind,
    n: u32,
    orbit: &Orbit,
    actions: &[(u32, Vec<sym::SymMove>, Effect)],
) -> Vec<String> {
    let probe = OrbitProbe::new(SlotMap::for_orbit(n, orbit), sticker_count(n));
    let binding = Binding::for_orbit(n, orbit);
    // Stickers that must not move: everything outside the target orbit (core types: every
    // other core sticker).
    let guard: Vec<u32> = if is_core(kind) {
        orbits(n)
            .into_iter()
            .filter(|o| {
                o.id != orbit.id
                    && matches!(
                        o.kind,
                        OrbitKind::Corner | OrbitKind::MidEdge | OrbitKind::FixedCenter
                    )
            })
            .flat_map(|o| o.stickers)
            .collect()
    } else {
        (0..sticker_count(n) as u32)
            .filter(|&s| !probe.contains(s as usize))
            .collect()
    };
    actions
        .par_iter()
        .map_init(
            || LabeledCube::solved(n),
            |cube, (id, moves, want)| {
                let concrete: Vec<Move> = sym::instantiate(moves, binding);
                if let Some(bad) = concrete.iter().find(|m| !m.is_allowed(n)) {
                    return Some(format!(
                        "{kind} action {id} at N={n}: move {bad} not allowed"
                    ));
                }
                cube.apply_all(&concrete);
                let f = cube.facelets();
                let moved = guard.iter().find(|&&s| f[s as usize] != s);
                let got = probe.effect(f);
                let result = if let Some(&s) = moved {
                    Some(format!(
                        "{kind} action {id} at N={n} ({}, {}): impure, sticker {s} moved",
                        orbit.a, orbit.b
                    ))
                } else if got.as_ref() != Some(want) {
                    Some(format!(
                        "{kind} action {id} at N={n} ({}, {}): effect {:?} != declared {:?}",
                        orbit.a, orbit.b, got, want
                    ))
                } else {
                    None
                };
                cube.apply_all(&nx_sim::invert(&concrete));
                result
            },
        )
        .flatten()
        .collect()
}

/// Coverage (ARCHITECTURE §6.3): the 3-cycles connect every movable slot (so they generate the
/// alternating group), and for orientation types some pure orientation change is generated.
pub fn coverage(kind: OrbitKind, t: &TypeLibrary) -> Vec<String> {
    let slots = kind.slot_count();
    let fixed = if kind == OrbitKind::Corner {
        Some(slots_fixed_corner())
    } else {
        None
    };
    let mut parent: Vec<usize> = (0..slots).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let m = t.orientation_mod;
    let mut pure_orientation = false;
    for a in &t.actions {
        let e = TypeLibrary::effect(a);
        match e.classify() {
            Some((MacroKind::ThreeCycle, c)) => {
                for w in c.windows(2) {
                    let (x, y) = (
                        find(&mut parent, usize::from(w[0])),
                        find(&mut parent, usize::from(w[1])),
                    );
                    parent[x] = y;
                }
                // a³ twists each cycled piece by the cycle's orientation sum.
                let sum: u32 = c.iter().map(|&s| u32::from(e.ori[usize::from(s)])).sum();
                if m > 1 && sum % u32::from(m) != 0 {
                    pure_orientation = true;
                }
            }
            Some((MacroKind::OrientPair, _)) => pure_orientation = true,
            None => {}
        }
    }
    let movable: Vec<usize> = (0..slots).filter(|&s| Some(s) != fixed).collect();
    let root = find(&mut parent, movable[0]);
    let mut out = Vec::new();
    if movable.iter().any(|&s| find(&mut parent, s) != root) {
        out.push(format!("{kind}: 3-cycles do not connect all movable slots"));
    }
    if m > 1 && !pure_orientation {
        out.push(format!("{kind}: no pure orientation change is generated"));
    }
    if let Some(f) = fixed {
        if t.actions
            .iter()
            .any(|a| usize::from(a.perm[f]) != f || a.ori_delta[f] != 0)
        {
            out.push(format!("{kind}: an action moves the fixed corner"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{SearchConfig, discover};
    use crate::library::expand;

    fn lib_for(kind: OrbitKind, pairs: Vec<(usize, usize)>, setup_len: usize) -> TypeLibrary {
        let cfg = SearchConfig {
            pairs,
            setup_len,
            ..SearchConfig::default_for(kind)
        };
        let (found, stats) = discover(kind, &cfg);
        expand(kind, &cfg, &found, stats).0
    }

    fn opts() -> VerifyOptions {
        VerifyOptions {
            max_full_n: 9,
            spot_ns: vec![15],
            spot_samples: 4,
        }
    }

    #[test]
    fn verifier_accepts_good_and_catches_bad_actions() {
        let kind = OrbitKind::XCenter;
        let good = lib_for(kind, vec![(3, 1)], 1);
        let r = verify_type(kind, &good, &opts());
        assert!(
            r.failures.is_empty(),
            "{:?}",
            &r.failures[..r.failures.len().min(3)]
        );
        assert!(r.instances > 5 && r.checks > 0);

        // Wrong declared effect: swap two perm entries.
        let mut bad = good.clone();
        bad.actions[3].perm.swap(0, 1);
        let r = verify_type(kind, &bad, &opts());
        assert!(
            r.failures.iter().any(|f| f.contains("action 3")),
            "{:?}",
            r.failures
        );

        // Impure moves: a setup without its undo changes other orbits.
        let mut bad = good.clone();
        let m = &mut bad.macros[bad.actions[5].macro_id as usize];
        m.moves.push("y:OUTER:1".to_string());
        let r = verify_type(kind, &bad, &opts());
        assert!(
            r.failures
                .iter()
                .any(|f| f.contains("impure") || f.contains("cost")),
            "{:?}",
            r.failures
        );
    }

    #[test]
    fn coverage_catches_missing_generators() {
        let kind = OrbitKind::XCenter;
        let good = lib_for(kind, vec![(3, 1)], 1);
        assert!(coverage(kind, &good).is_empty());
        // Keep only 3-cycles inside slots 0..12: slots 12..24 become unreachable.
        let mut bad = good.clone();
        bad.actions.retain(|a| {
            a.perm
                .iter()
                .enumerate()
                .all(|(i, &p)| i < 12 || usize::from(p) == i)
        });
        assert!(!coverage(kind, &bad).is_empty());
        // Corners without any orientation-changing generator fail orientation coverage.
        let corner = lib_for(OrbitKind::Corner, vec![(3, 1)], 0);
        let mut flat = corner.clone();
        flat.actions.retain(|a| {
            let e = TypeLibrary::effect(a);
            matches!(e.classify(), Some((MacroKind::ThreeCycle, c))
                if c.iter().map(|&s| u32::from(e.ori[usize::from(s)])).sum::<u32>() % 3 == 0)
        });
        assert!(
            coverage(OrbitKind::Corner, &flat)
                .iter()
                .any(|f| f.contains("orientation"))
        );
    }
}
