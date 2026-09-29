//! Browser API (see `docs/04-system-architecture.md` §9.2).
//!
//! Moves cross the boundary as flat byte arrays of `[axis, layers, turns]` triples.

use rg_cube::{Geometry, Move};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn encode(moves: &[Move]) -> Vec<u8> {
    moves.iter().flat_map(|m| m.encode()).collect()
}

fn decode(bytes: &[u8]) -> Result<Vec<Move>, JsError> {
    if bytes.len() % 3 != 0 {
        return Err(JsError::new(
            "encoded moves must be [axis, layers, turns] triples",
        ));
    }
    bytes
        .chunks(3)
        .map(|c| Move::decode(c).ok_or_else(|| JsError::new("invalid encoded move")))
        .collect()
}

fn check_size(n: u8) -> Result<(), JsError> {
    if (rg_cube::MIN_N..=rg_cube::MAX_N).contains(&n) {
        Ok(())
    } else {
        Err(JsError::new(&format!("unsupported cube size {n}")))
    }
}

/// Facelet cube: the single source of truth for cube state in the UI.
#[wasm_bindgen]
pub struct Cube {
    inner: rg_cube::Cube,
}

#[wasm_bindgen]
impl Cube {
    #[wasm_bindgen(constructor)]
    pub fn new(n: u8) -> Result<Cube, JsError> {
        Ok(Cube {
            inner: rg_cube::Cube::new(n).map_err(js_err)?,
        })
    }

    #[wasm_bindgen(getter)]
    pub fn n(&self) -> u8 {
        self.inner.n()
    }

    /// 6·n² colour indices in U R F D L B order.
    pub fn facelets(&self) -> Vec<u8> {
        self.inner.facelets().to_vec()
    }

    #[wasm_bindgen(js_name = setFacelets)]
    pub fn set_facelets(&mut self, facelets: &[u8]) -> Result<(), JsError> {
        self.inner.set_facelets(facelets).map_err(js_err)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Parse and apply an algorithm; returns the encoded moves that were applied.
    pub fn apply(&mut self, alg: &str) -> Result<Vec<u8>, JsError> {
        let moves = self.inner.apply_alg(alg).map_err(js_err)?;
        Ok(encode(&moves))
    }

    #[wasm_bindgen(js_name = applyEncoded)]
    pub fn apply_encoded(&mut self, moves: &[u8]) -> Result<(), JsError> {
        self.inner.apply_moves(&decode(moves)?);
        Ok(())
    }

    #[wasm_bindgen(js_name = isSolved)]
    pub fn is_solved(&self) -> bool {
        self.inner.is_solved()
    }
}

#[wasm_bindgen(js_name = parseAlg)]
pub fn parse_alg(n: u8, alg: &str) -> Result<Vec<u8>, JsError> {
    check_size(n)?;
    Ok(encode(&rg_cube::parse_alg(n, alg).map_err(js_err)?))
}

#[wasm_bindgen(js_name = formatAlg)]
pub fn format_alg(n: u8, moves: &[u8]) -> Result<String, JsError> {
    check_size(n)?;
    Ok(rg_cube::format_alg(n, &decode(moves)?))
}

#[wasm_bindgen(js_name = invertAlg)]
pub fn invert_alg(moves: &[u8]) -> Result<Vec<u8>, JsError> {
    Ok(encode(&rg_cube::invert(&decode(moves)?)))
}

#[wasm_bindgen(js_name = simplifyAlg)]
pub fn simplify_alg(moves: &[u8]) -> Result<Vec<u8>, JsError> {
    Ok(encode(&rg_cube::simplify(&decode(moves)?)))
}

/// Random canonical move sequence from the puzzle's move set.
#[wasm_bindgen(js_name = randomScramble)]
pub fn random_scramble(n: u8, seed: u64) -> Result<Vec<u8>, JsError> {
    check_size(n)?;
    Ok(encode(&rg_cube::random_move_scramble(
        n,
        rg_cube::scramble_length(n),
        seed,
    )))
}

/// Base generators of the puzzle's move set, encoded (quarter turns).
#[wasm_bindgen(js_name = generatorMoves)]
pub fn generator_moves(n: u8) -> Result<Vec<u8>, JsError> {
    check_size(n)?;
    Ok(encode(&rg_cube::generator_moves(n)))
}

/// Doubled-coordinate sticker centres (x, y, z per sticker), in facelet order.
#[wasm_bindgen(js_name = stickerPositions)]
pub fn sticker_positions(n: u8) -> Result<Vec<i32>, JsError> {
    check_size(n)?;
    Ok(Geometry::new(n)
        .positions()
        .iter()
        .flatten()
        .copied()
        .collect())
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Graph-search solvers with their lookup tables. Lives in the solver worker.
#[wasm_bindgen]
pub struct Solver {
    inner: rg_solve::Solver,
}

#[wasm_bindgen]
impl Solver {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Solver {
        Solver {
            inner: rg_solve::Solver::new(),
        }
    }

    pub fn supports(n: u8) -> bool {
        rg_solve::Solver::supports(n)
    }

    /// Build lookup tables for size `n`; returns milliseconds spent (0 if already built).
    pub fn prepare(&mut self, n: u8) -> Result<f64, JsError> {
        let t0 = now_ms();
        let built = self.inner.prepare(n).map_err(js_err)?;
        Ok(if built { now_ms() - t0 } else { 0.0 })
    }

    pub fn solve(
        &mut self,
        n: u8,
        facelets: &[u8],
        target_length: u32,
        max_time_ms: f64,
    ) -> Result<SolveResult, JsError> {
        let opts = rg_solve::SolveOptions {
            target_length: target_length as usize,
            max_time_ms,
        };
        let s = self
            .inner
            .solve(n, facelets, &opts, &now_ms)
            .map_err(js_err)?;
        Ok(SolveResult {
            moves: encode(&s.moves),
            algorithm: s.algorithm.to_string(),
            optimal: s.optimal,
            phase_lengths: s.phase_lengths.iter().map(|&l| l as u32).collect(),
            nodes: s.nodes as f64,
            search_ms: s.search_ms,
        })
    }
}

#[wasm_bindgen]
pub struct SolveResult {
    moves: Vec<u8>,
    algorithm: String,
    optimal: bool,
    phase_lengths: Vec<u32>,
    nodes: f64,
    search_ms: f64,
}

#[wasm_bindgen]
impl SolveResult {
    /// Encoded moves in the caller's frame.
    #[wasm_bindgen(getter)]
    pub fn moves(&self) -> Vec<u8> {
        self.moves.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn algorithm(&self) -> String {
        self.algorithm.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn optimal(&self) -> bool {
        self.optimal
    }

    #[wasm_bindgen(getter, js_name = phaseLengths)]
    pub fn phase_lengths(&self) -> Vec<u32> {
        self.phase_lengths.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn nodes(&self) -> f64 {
        self.nodes
    }

    #[wasm_bindgen(getter, js_name = searchMs)]
    pub fn search_ms(&self) -> f64 {
        self.search_ms
    }
}

/// Doubled-coordinate cubie centre of each sticker (x, y, z per sticker).
#[wasm_bindgen(js_name = cubieCentres)]
pub fn cubie_centres(n: u8) -> Result<Vec<i32>, JsError> {
    check_size(n)?;
    Ok(Geometry::new(n)
        .cubies()
        .iter()
        .flatten()
        .copied()
        .collect())
}
