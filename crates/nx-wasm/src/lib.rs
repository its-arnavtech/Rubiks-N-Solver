//! Browser API over `nx-sim` (see `docs/nn/ARCHITECTURE.md` §11). Orbit maps and snapshots
//! are added at M8.2.

use nx_sim::{Cube, Move, SimError};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Apply wire-encoded moves (CONVENTIONS §2) to a facelet array and return the new facelets.
/// Every move must be allowed on N (layers `0..=N−2`).
pub fn apply_moves_native(n: u32, facelets: &[u8], moves: &[u32]) -> Result<Vec<u8>, SimError> {
    let mut cube = Cube::from_facelets(n, facelets.to_vec())?;
    for &w in moves {
        let mv = Move::decode(w).ok_or(SimError::BadMoveWord(w))?;
        if !mv.is_allowed(n) {
            return Err(SimError::MoveNotAllowed { n, mv });
        }
        cube.apply(mv);
    }
    Ok(cube.into_facelets())
}

#[wasm_bindgen(js_name = applyMoves)]
pub fn apply_moves(n: u32, facelets: &[u8], moves: &[u32]) -> Result<Vec<u8>, JsError> {
    apply_moves_native(n, facelets, moves).map_err(|e| JsError::new(&e.to_string()))
}

/// Solved facelets for N.
#[wasm_bindgen(js_name = solvedFacelets)]
pub fn solved_facelets(n: u32) -> Result<Vec<u8>, JsError> {
    if n < 2 {
        return Err(JsError::new(&SimError::UnsupportedSize(n).to_string()));
    }
    Ok(Cube::solved(n).into_facelets())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_moves_round_trip_and_errors() {
        let n = 5;
        let (cube, moves) = Cube::scramble(n, 40, 3);
        let words: Vec<u32> = moves.iter().map(|m| m.encode()).collect();
        let solved = Cube::solved(n).into_facelets();
        assert_eq!(
            apply_moves_native(n, &solved, &words).unwrap(),
            cube.facelets()
        );
        let back: Vec<u32> = nx_sim::invert(&moves).iter().map(|m| m.encode()).collect();
        assert_eq!(
            apply_moves_native(n, cube.facelets(), &back).unwrap(),
            solved
        );

        let last_layer = nx_sim::Move::new(nx_sim::Axis::X, n - 1, 1).encode();
        assert!(matches!(
            apply_moves_native(n, &solved, &[last_layer]),
            Err(SimError::MoveNotAllowed { .. })
        ));
        assert_eq!(
            apply_moves_native(n, &solved, &[0]),
            Err(SimError::BadMoveWord(0))
        );
        assert!(apply_moves_native(n, &solved[1..], &[]).is_err());
    }
}
