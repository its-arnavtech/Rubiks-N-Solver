//! The baseline pipeline (ARCHITECTURE §7): validate, phases, core, all other orbits, emit,
//! cancel, verify. Produces a `SolveResult` (CONVENTIONS §8).

use std::fmt;
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use nx_macro::{Binding, sym};
use nx_sim::{
    Cube, Move, Orbit, OrbitKind, SlotMap, ValidationError, cancel, encode_moves, orbits, validate,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::orbit_solver::SolverLib;
use crate::phases;

/// A contiguous range of the raw move list with what produced it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    /// `parity | core_frame | core | orbits | fallback`.
    pub phase: String,
    pub orbit_id: Option<u32>,
    pub orbit_type: Option<String>,
    pub round: Option<u32>,
    pub action_id: Option<u32>,
    pub q: Option<f32>,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub orbits_total: u32,
    pub orbits_fallback: u32,
    pub rounds: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timings {
    pub extract: f64,
    pub plan: f64,
    pub emit: f64,
    pub verify: f64,
    pub total: f64,
}

/// CONVENTIONS §8. `moves_b64` is the raw list (segments index it); `cancelled_moves_b64` is
/// the same solution after cancellation. Both are verified.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolveResult {
    pub n: u32,
    pub solver: String,
    pub verified: bool,
    pub moves_b64: String,
    pub cancelled_moves_b64: String,
    pub raw_len: u64,
    pub cancelled_len: u64,
    pub segments: Vec<Segment>,
    pub stats: Stats,
    pub timings_ms: Timings,
    pub checkpoint: Option<String>,
    pub library_sha256: String,
}

/// The result plus the move lists themselves.
#[derive(Clone, Debug)]
pub struct SolveOutput {
    pub result: SolveResult,
    pub raw: Vec<Move>,
    pub cancelled: Vec<Move>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SolveError {
    Invalid(ValidationError),
    Plan(String),
    /// The move list did not solve the cube. Never reported as a solve (ADR-007).
    Unverified,
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "invalid cube: {e}"),
            Self::Plan(e) => write!(f, "planning failed: {e}"),
            Self::Unverified => write!(f, "solution failed verification"),
        }
    }
}

impl std::error::Error for SolveError {}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// Collects raw moves and their segments.
struct Emitter {
    raw: Vec<Move>,
    segments: Vec<Segment>,
}

impl Emitter {
    fn push(
        &mut self,
        phase: &str,
        orbit: Option<&Orbit>,
        round: Option<u32>,
        action: Option<u32>,
        moves: &[Move],
    ) {
        let start = self.raw.len() as u64;
        self.raw.extend_from_slice(moves);
        self.segments.push(Segment {
            phase: phase.to_string(),
            orbit_id: orbit.map(|o| o.id),
            orbit_type: orbit.map(|o| o.kind.name().to_string()),
            round,
            action_id: action,
            q: None,
            start,
            end: self.raw.len() as u64,
        });
    }
}

/// Solve with the baseline everywhere.
pub fn solve_baseline(input: &Cube, lib: &SolverLib) -> Result<SolveOutput, SolveError> {
    let total = Instant::now();
    validate(input).map_err(SolveError::Invalid)?;
    let n = input.n();
    let os = orbits(n);
    let find = |k: OrbitKind| os.iter().find(|o| o.kind == k);
    let mut work = input.clone();
    let mut em = Emitter {
        raw: Vec::new(),
        segments: Vec::new(),
    };
    let mut rounds = 0u32;

    // Phase 0: wing parity.
    for (oid, m) in phases::wing_parity(&work, &os) {
        em.push("parity", Some(&os[oid as usize]), None, None, &[m]);
        work.apply(m);
    }
    // Phase 1a: fixed-center frame; 1b: corner parity.
    if let Some(frame) = find(OrbitKind::FixedCenter) {
        let path = phases::core_frame(&work, frame)
            .ok_or_else(|| SolveError::Plan("frame BFS failed".into()))?;
        if !path.is_empty() {
            em.push("core_frame", Some(frame), None, None, &path);
            work.apply_all(&path);
        }
    }
    let corner = find(OrbitKind::Corner).expect("every cube has corners");
    if let Some(m) = phases::corner_parity(&work, corner) {
        em.push("parity", Some(corner), None, None, &[m]);
        work.apply(m);
    }
    // Phase 1c: corners, then middle edges. Their actions may disturb non-core orbits, so the
    // work cube follows along before phase 2 is extracted.
    for kind in [OrbitKind::Corner, OrbitKind::MidEdge] {
        let Some(orbit) = find(kind) else { continue };
        let content = SlotMap::for_orbit(n, orbit)
            .extract(&work)
            .map_err(|e| SolveError::Plan(e.to_string()))?;
        let plan = lib.plan(kind, &content).map_err(SolveError::Plan)?;
        rounds = rounds.max(plan.len() as u32);
        let kl = lib.kind(kind);
        let b = Binding::for_orbit(n, orbit);
        for (round, &a) in plan.iter().enumerate() {
            let moves = sym::instantiate(&kl.actions[a].moves, b);
            em.push(
                "core",
                Some(orbit),
                Some(round as u32),
                Some(kl.actions[a].id),
                &moves,
            );
            work.apply_all(&moves);
        }
    }

    // Phase 2: every other orbit, independently (actions are pure).
    let t = Instant::now();
    let rest: Vec<&Orbit> = os
        .iter()
        .filter(|o| {
            !matches!(
                o.kind,
                OrbitKind::Corner | OrbitKind::MidEdge | OrbitKind::FixedCenter
            )
        })
        .collect();
    let contents: Vec<Vec<u8>> = rest
        .par_iter()
        .map(|o| {
            SlotMap::for_orbit(n, o)
                .extract(&work)
                .map_err(|e| SolveError::Plan(e.to_string()))
        })
        .collect::<Result<_, _>>()?;
    let extract_ms = ms(t);
    let t = Instant::now();
    let plans: Vec<Vec<usize>> = rest
        .par_iter()
        .zip(&contents)
        .map(|(o, c)| {
            lib.plan(o.kind, c)
                .map_err(|e| SolveError::Plan(format!("orbit {}: {e}", o.id)))
        })
        .collect::<Result<_, _>>()?;
    let plan_ms = ms(t);
    let t = Instant::now();
    for (o, plan) in rest.iter().zip(&plans) {
        rounds = rounds.max(plan.len() as u32);
        let kl = lib.kind(o.kind);
        let b = Binding::for_orbit(n, o);
        for (round, &a) in plan.iter().enumerate() {
            let moves = sym::instantiate(&kl.actions[a].moves, b);
            em.push(
                "orbits",
                Some(o),
                Some(round as u32),
                Some(kl.actions[a].id),
                &moves,
            );
        }
    }
    let cancelled = cancel(&em.raw);
    let emit_ms = ms(t);

    // Verify both lists by replay on the input (ADR-007).
    let t = Instant::now();
    let replay = |moves: &[Move]| {
        let mut c = input.clone();
        c.apply_all(moves);
        c.is_solved()
    };
    let (raw_ok, cancelled_ok) = rayon::join(|| replay(&em.raw), || replay(&cancelled));
    if !(raw_ok && cancelled_ok) {
        return Err(SolveError::Unverified);
    }
    let verify_ms = ms(t);

    let result = SolveResult {
        n,
        solver: "baseline".to_string(),
        verified: true,
        moves_b64: STANDARD.encode(encode_moves(&em.raw)),
        cancelled_moves_b64: STANDARD.encode(encode_moves(&cancelled)),
        raw_len: em.raw.len() as u64,
        cancelled_len: cancelled.len() as u64,
        segments: em.segments,
        stats: Stats {
            orbits_total: os.len() as u32,
            orbits_fallback: 0,
            rounds,
        },
        timings_ms: Timings {
            extract: extract_ms,
            plan: plan_ms,
            emit: emit_ms,
            verify: verify_ms,
            total: ms(total),
        },
        checkpoint: None,
        library_sha256: lib.sha256.clone(),
    };
    Ok(SolveOutput {
        result,
        raw: em.raw,
        cancelled,
    })
}
