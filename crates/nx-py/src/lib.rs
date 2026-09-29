//! Python module `nxsim` (see `docs/nn/ARCHITECTURE.md` §11).
//!
//! Arrays cross the boundary as `bytes`: facelets are `u8` per sticker, move lists and orbit
//! maps are little-endian `u32` (CONVENTIONS §1–2). Python wraps them with `numpy.frombuffer`.
//! Training code never imports this module (ADR-008).

use std::collections::HashMap;
use std::sync::Mutex;

use nx_macro::{Binding, sym};
use nx_sim::{Cube, Move, Orbit, OrbitKind, SlotMap};
use nx_solve::SolverLib;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

fn err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn bytes<'py>(py: Python<'py>, v: &[u8]) -> Bound<'py, PyBytes> {
    PyBytes::new(py, v)
}

fn cube(n: u32, facelets: &[u8]) -> PyResult<Cube> {
    Cube::from_facelets(n, facelets.to_vec()).map_err(err)
}

fn decode(moves: &[u8]) -> PyResult<Vec<Move>> {
    nx_sim::decode_moves(moves)
        .ok_or_else(|| err("move bytes are not valid little-endian u32 move words"))
}

fn kind(name: &str) -> PyResult<OrbitKind> {
    OrbitKind::from_name(name).ok_or_else(|| err(format!("unknown orbit type {name:?}")))
}

/// Crate version, used by the import smoke test.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
fn sticker_count(n: u32) -> usize {
    nx_sim::sticker_count(n)
}

/// Solved facelets.
#[pyfunction]
fn solved(py: Python<'_>, n: u32) -> PyResult<Bound<'_, PyBytes>> {
    if n < 2 {
        return Err(err("N must be at least 2"));
    }
    Ok(bytes(py, Cube::solved(n).facelets()))
}

/// `(facelets, moves)` of a seeded move scramble (CONVENTIONS §9).
#[pyfunction]
fn scramble(
    py: Python<'_>,
    n: u32,
    length: usize,
    seed: u64,
) -> PyResult<(Bound<'_, PyBytes>, Bound<'_, PyBytes>)> {
    if n < 2 {
        return Err(err("N must be at least 2"));
    }
    let (c, moves) = Cube::scramble(n, length, seed);
    Ok((
        bytes(py, c.facelets()),
        bytes(py, &nx_sim::encode_moves(&moves)),
    ))
}

/// Uniform random reachable state (CONVENTIONS §6).
#[pyfunction]
fn random_state(py: Python<'_>, n: u32, seed: u64) -> PyResult<Bound<'_, PyBytes>> {
    if n < 2 {
        return Err(err("N must be at least 2"));
    }
    Ok(bytes(py, nx_sim::random_state(n, seed).facelets()))
}

/// Apply moves (any layer `0..=N−2`); returns the new facelets.
#[pyfunction]
fn apply_moves<'py>(
    py: Python<'py>,
    n: u32,
    facelets: &[u8],
    moves: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let mut c = cube(n, facelets)?;
    let ms = decode(moves)?;
    if let Some(m) = ms.iter().find(|m| !m.is_allowed(n)) {
        return Err(err(format!("move {m} is not allowed on N={n}")));
    }
    c.apply_all(&ms);
    Ok(bytes(py, c.facelets()))
}

/// Raise `ValueError` unless the state is reachable (CONVENTIONS §6 `validate`).
#[pyfunction]
fn validate(n: u32, facelets: &[u8]) -> PyResult<()> {
    nx_sim::validate(&cube(n, facelets)?).map_err(err)
}

#[pyfunction]
fn is_solved(n: u32, facelets: &[u8]) -> PyResult<bool> {
    Ok(cube(n, facelets)?.is_solved())
}

/// Replay `moves` from `start`; true iff the result is solved.
#[pyfunction]
fn verify(n: u32, start: &[u8], moves: &[u8]) -> PyResult<bool> {
    let mut c = cube(n, start)?;
    c.apply_all(&decode(moves)?);
    Ok(c.is_solved())
}

/// Merge same-layer turns (ARCHITECTURE §7 step 6).
#[pyfunction]
fn cancel<'py>(py: Python<'py>, moves: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    Ok(bytes(
        py,
        &nx_sim::encode_moves(&nx_sim::cancel(&decode(moves)?)),
    ))
}

/// Display notation of a move list.
#[pyfunction]
fn format_moves(moves: &[u8]) -> PyResult<String> {
    Ok(nx_sim::format_moves(&decode(moves)?))
}

/// `[(id, type, a, b), ...]` in id order (CONVENTIONS §3).
#[pyfunction]
fn orbits(n: u32) -> PyResult<Vec<(u32, &'static str, u32, u32)>> {
    if n < 2 {
        return Err(err("N must be at least 2"));
    }
    Ok(nx_sim::orbits(n)
        .iter()
        .map(|o| (o.id, o.kind.name(), o.a, o.b))
        .collect())
}

/// Orbit id of every sticker, as little-endian `u32` bytes.
#[pyfunction]
fn sticker_orbit(py: Python<'_>, n: u32) -> PyResult<Bound<'_, PyBytes>> {
    if n < 2 {
        return Err(err("N must be at least 2"));
    }
    let mut ids = vec![0u32; nx_sim::sticker_count(n)];
    for o in nx_sim::orbits(n) {
        for s in o.stickers {
            ids[s as usize] = o.id;
        }
    }
    Ok(bytes(
        py,
        &ids.iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<u8>>(),
    ))
}

/// Canonical slot contents of every orbit, grouped by type:
/// `{type: (orbit_ids, contents)}` where `contents` is `len(orbit_ids) × slots` bytes.
#[pyfunction]
fn extract<'py>(py: Python<'py>, n: u32, facelets: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let c = cube(n, facelets)?;
    let mut groups: Vec<(OrbitKind, Vec<u32>, Vec<u8>)> = Vec::new();
    for o in nx_sim::orbits(n) {
        let content = SlotMap::for_orbit(n, &o).extract(&c).map_err(err)?;
        match groups.iter_mut().find(|g| g.0 == o.kind) {
            Some(g) => {
                g.1.push(o.id);
                g.2.extend(content);
            }
            None => groups.push((o.kind, vec![o.id], content)),
        }
    }
    let d = PyDict::new(py);
    for (k, ids, contents) in groups {
        d.set_item(k.name(), (ids, bytes(py, &contents)))?;
    }
    Ok(d)
}

/// Write one orbit's slot contents into facelets (inverse of `extract` for that orbit).
#[pyfunction]
fn insert<'py>(
    py: Python<'py>,
    n: u32,
    facelets: &[u8],
    orbit_id: u32,
    content: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let mut c = cube(n, facelets)?;
    let os = nx_sim::orbits(n);
    let o = os
        .get(orbit_id as usize)
        .ok_or_else(|| err("orbit id out of range"))?;
    let map = SlotMap::for_orbit(n, o);
    if content.len() != map.slot_count() {
        return Err(err(format!("{} has {} slots", o.kind, map.slot_count())));
    }
    map.insert(&mut c, content);
    Ok(bytes(py, c.facelets()))
}

/// Code phases before the network (ARCHITECTURE §7): wing parity, fixed-center frame, corner
/// parity. Returns `(segments, facelets_after)` with segments `(phase, orbit_id, moves)`.
type PhaseSegments<'py> = Vec<(String, u32, Bound<'py, PyBytes>)>;

#[pyfunction]
fn phases<'py>(
    py: Python<'py>,
    n: u32,
    facelets: &[u8],
) -> PyResult<(PhaseSegments<'py>, Bound<'py, PyBytes>)> {
    let mut c = cube(n, facelets)?;
    nx_sim::validate(&c).map_err(err)?;
    let os = nx_sim::orbits(n);
    let mut segs: Vec<(String, u32, Vec<Move>)> = Vec::new();
    for (oid, m) in nx_solve::phases::wing_parity(&c, &os) {
        c.apply(m);
        segs.push(("parity".into(), oid, vec![m]));
    }
    if let Some(frame) = os.iter().find(|o| o.kind == OrbitKind::FixedCenter) {
        let path =
            nx_solve::phases::core_frame(&c, frame).ok_or_else(|| err("frame BFS failed"))?;
        if !path.is_empty() {
            c.apply_all(&path);
            segs.push(("core_frame".into(), frame.id, path));
        }
    }
    let corner = &os[0];
    if let Some(m) = nx_solve::phases::corner_parity(&c, corner) {
        c.apply(m);
        segs.push(("parity".into(), corner.id, vec![m]));
    }
    let out = segs
        .into_iter()
        .map(|(p, id, ms)| (p, id, bytes(py, &nx_sim::encode_moves(&ms))))
        .collect();
    Ok((out, bytes(py, c.facelets())))
}

/// The action library, for emission and the baseline fallback.
#[pyclass(frozen, module = "nxsim")]
struct Library {
    lib: nx_macro::Library,
    solver: SolverLib,
    orbits: Mutex<HashMap<u32, Vec<Orbit>>>,
}

impl Library {
    fn orbit(&self, n: u32, orbit_id: u32) -> PyResult<Orbit> {
        let mut cache = self.orbits.lock().expect("orbit cache lock");
        let os = cache.entry(n).or_insert_with(|| nx_sim::orbits(n));
        os.get(orbit_id as usize)
            .cloned()
            .ok_or_else(|| err(format!("N={n} has no orbit {orbit_id}")))
    }
}

#[pymethods]
impl Library {
    /// Load `library.json`; its sha256 must match its content.
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let lib = nx_macro::Library::load(std::path::Path::new(path)).map_err(err)?;
        let solver = SolverLib::new(&lib).map_err(err)?;
        Ok(Self {
            lib,
            solver,
            orbits: Mutex::new(HashMap::new()),
        })
    }

    #[getter]
    fn sha256(&self) -> String {
        self.lib.sha256.clone()
    }

    /// Concrete moves of a list of actions on one orbit instance, concatenated, plus the
    /// end offset of each action (for segments).
    fn emit<'py>(
        &self,
        py: Python<'py>,
        n: u32,
        orbit_id: u32,
        action_ids: Vec<u32>,
    ) -> PyResult<(Bound<'py, PyBytes>, Vec<u64>)> {
        let o = self.orbit(n, orbit_id)?;
        let kl = self.solver.kind(o.kind);
        let b = Binding::for_orbit(n, &o);
        let mut moves = Vec::new();
        let mut ends = Vec::with_capacity(action_ids.len());
        for id in action_ids {
            let a = kl
                .actions
                .get(id as usize)
                .ok_or_else(|| err(format!("{} has no action {id}", o.kind)))?;
            moves.extend(sym::instantiate(&a.moves, b));
            ends.push(moves.len() as u64);
        }
        Ok((bytes(py, &nx_sim::encode_moves(&moves)), ends))
    }

    /// Baseline plan (action ids) for one orbit's slot contents.
    fn baseline_orbit(&self, orbit_type: &str, content: &[u8]) -> PyResult<Vec<u32>> {
        let k = kind(orbit_type)?;
        let plan = self.solver.plan(k, content).map_err(err)?;
        let kl = self.solver.kind(k);
        Ok(plan.into_iter().map(|a| kl.actions[a].id).collect())
    }

    /// Full baseline solve; returns the `SolveResult` JSON (CONVENTIONS §8).
    fn solve_baseline(&self, n: u32, facelets: &[u8]) -> PyResult<String> {
        let c = cube(n, facelets)?;
        let out = nx_solve::solve_baseline(&c, &self.solver).map_err(err)?;
        serde_json::to_string(&out.result).map_err(err)
    }
}

#[pymodule]
fn nxsim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(sticker_count, m)?)?;
    m.add_function(wrap_pyfunction!(solved, m)?)?;
    m.add_function(wrap_pyfunction!(scramble, m)?)?;
    m.add_function(wrap_pyfunction!(random_state, m)?)?;
    m.add_function(wrap_pyfunction!(apply_moves, m)?)?;
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    m.add_function(wrap_pyfunction!(is_solved, m)?)?;
    m.add_function(wrap_pyfunction!(verify, m)?)?;
    m.add_function(wrap_pyfunction!(cancel, m)?)?;
    m.add_function(wrap_pyfunction!(format_moves, m)?)?;
    m.add_function(wrap_pyfunction!(orbits, m)?)?;
    m.add_function(wrap_pyfunction!(sticker_orbit, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(insert, m)?)?;
    m.add_function(wrap_pyfunction!(phases, m)?)?;
    m.add_class::<Library>()?;
    Ok(())
}
