//! Browser API over `nx-sim` (see `docs/nn/ARCHITECTURE.md` §11–12): exact replay with
//! snapshots, orbit maps and slot maps. The web never re-implements move logic (ADR-009).

use nx_sim::{Cube, Move, OrbitKind, SimError, SlotMap};
use wasm_bindgen::prelude::*;

fn js(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn decode_allowed(n: u32, moves: &[u32]) -> Result<Vec<Move>, SimError> {
    moves
        .iter()
        .map(|&w| {
            let mv = Move::decode(w).ok_or(SimError::BadMoveWord(w))?;
            if mv.is_allowed(n) {
                Ok(mv)
            } else {
                Err(SimError::MoveNotAllowed { n, mv })
            }
        })
        .collect()
}

/// Replays a move list from a start state. Snapshots every `every` moves make seeking cheap.
#[wasm_bindgen]
pub struct Replayer {
    n: u32,
    moves: Vec<Move>,
    snapshots: Vec<Vec<u8>>,
    every: usize,
    cur: Cube,
    pos: usize,
}

#[wasm_bindgen]
impl Replayer {
    /// Validates the moves and replays them once to build snapshots.
    #[wasm_bindgen(constructor)]
    pub fn new(n: u32, facelets: &[u8], moves: &[u32], every: usize) -> Result<Replayer, JsError> {
        let start = Cube::from_facelets(n, facelets.to_vec()).map_err(js)?;
        let moves = decode_allowed(n, moves).map_err(js)?;
        let every = every.max(1);
        let mut snapshots = vec![start.facelets().to_vec()];
        let mut c = start.clone();
        for chunk in moves.chunks(every) {
            c.apply_all(chunk);
            if chunk.len() == every {
                snapshots.push(c.facelets().to_vec());
            }
        }
        Ok(Replayer {
            n,
            moves,
            snapshots,
            every,
            cur: start,
            pos: 0,
        })
    }

    pub fn len(&self) -> usize {
        self.moves.len()
    }

    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn facelets(&self) -> Vec<u8> {
        self.cur.facelets().to_vec()
    }

    /// Move to position `pos` (0 = start, `len()` = after the last move).
    pub fn seek(&mut self, pos: usize) {
        let pos = pos.min(self.moves.len());
        if pos < self.pos || pos - self.pos > self.every {
            let k = pos / self.every;
            self.cur =
                Cube::from_facelets(self.n, self.snapshots[k].clone()).expect("snapshot shape");
            self.pos = k * self.every;
        }
        self.cur.apply_all(&self.moves[self.pos..pos]);
        self.pos = pos;
    }

    /// Is the state after all moves solved?
    #[wasm_bindgen(js_name = endsSolved)]
    pub fn ends_solved(&self) -> bool {
        let last = self.snapshots.len() - 1;
        let mut c =
            Cube::from_facelets(self.n, self.snapshots[last].clone()).expect("snapshot shape");
        c.apply_all(&self.moves[last * self.every..]);
        c.is_solved()
    }
}

/// Orbit id of every sticker (CONVENTIONS §3).
#[wasm_bindgen(js_name = stickerOrbits)]
pub fn sticker_orbits(n: u32) -> Result<Vec<u32>, JsError> {
    if n < 2 {
        return Err(js(SimError::UnsupportedSize(n)));
    }
    let mut ids = vec![0u32; nx_sim::sticker_count(n)];
    for o in nx_sim::orbits(n) {
        for s in o.stickers {
            ids[s as usize] = o.id;
        }
    }
    Ok(ids)
}

/// Kind of every orbit, as an index into
/// `[Corner, MidEdge, FixedCenter, Wing, XCenter, PlusCenter, ObliqueA, ObliqueB]`.
#[wasm_bindgen(js_name = orbitKinds)]
pub fn orbit_kinds(n: u32) -> Result<Vec<u8>, JsError> {
    if n < 2 {
        return Err(js(SimError::UnsupportedSize(n)));
    }
    Ok(nx_sim::orbits(n)
        .iter()
        .map(|o| {
            OrbitKind::ALL
                .iter()
                .position(|&k| k == o.kind)
                .expect("known kind") as u8
        })
        .collect())
}

/// Stickers of an orbit's canonical slots, slot-major (`slots × width` values).
#[wasm_bindgen(js_name = orbitSlots)]
pub fn orbit_slots(n: u32, orbit_id: u32) -> Result<Vec<u32>, JsError> {
    let os = nx_sim::orbits(n);
    let o = os
        .get(orbit_id as usize)
        .ok_or_else(|| js("orbit id out of range"))?;
    Ok(SlotMap::for_orbit(n, o).stickers)
}

/// An orbit's slot contents from colors (CONVENTIONS §4–5).
#[wasm_bindgen(js_name = extractOrbit)]
pub fn extract_orbit(n: u32, facelets: &[u8], orbit_id: u32) -> Result<Vec<u8>, JsError> {
    let c = Cube::from_facelets(n, facelets.to_vec()).map_err(js)?;
    let os = nx_sim::orbits(n);
    let o = os
        .get(orbit_id as usize)
        .ok_or_else(|| js("orbit id out of range"))?;
    SlotMap::for_orbit(n, o).extract(&c).map_err(js)
}

/// Doubled-coordinate sticker centres `[x, y, z, ...]` (CONVENTIONS §1).
#[wasm_bindgen(js_name = stickerPositions)]
pub fn sticker_positions(n: u32) -> Result<Vec<i32>, JsError> {
    if n < 2 {
        return Err(js(SimError::UnsupportedSize(n)));
    }
    Ok((0..nx_sim::sticker_count(n))
        .flat_map(|i| nx_sim::geometry::position_of(n, i).map(|v| v as i32))
        .collect())
}

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
    cube.apply_all(&decode_allowed(n, moves)?);
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

    #[test]
    fn replayer_seeks_exactly() {
        let n = 6;
        let start = nx_sim::random_state(n, 2);
        let moves = nx_sim::scramble_moves(n, 1000, 5);
        let words: Vec<u32> = moves.iter().map(|m| m.encode()).collect();
        let mut r = Replayer::new(n, start.facelets(), &words, 64).unwrap();
        assert_eq!(r.len(), 1000);
        for pos in [0, 1, 63, 64, 65, 500, 999, 1000, 10, 700, 3] {
            r.seek(pos);
            let mut c = start.clone();
            c.apply_all(&moves[..pos]);
            assert_eq!(r.facelets(), c.facelets(), "pos {pos}");
            assert_eq!(r.position(), pos);
        }
        let mut c = start.clone();
        c.apply_all(&moves);
        assert_eq!(r.ends_solved(), c.is_solved());
        let inv: Vec<u32> = nx_sim::invert(&moves).iter().map(|m| m.encode()).collect();
        let r2 = Replayer::new(n, c.facelets(), &inv, 100).unwrap();
        assert_eq!(r2.ends_solved(), start.is_solved());
    }

    #[test]
    fn orbit_maps() {
        let n = 7;
        let ids = sticker_orbits(n).unwrap();
        let kinds = orbit_kinds(n).unwrap();
        assert_eq!(ids.len(), 6 * 49);
        assert_eq!(kinds.len(), 11);
        assert_eq!(kinds[..3], [0, 1, 2]);
        let slots = orbit_slots(n, 9).unwrap();
        assert_eq!(slots.len(), 24);
        assert!(slots.iter().all(|&s| ids[s as usize] == 9));
        let solved = Cube::solved(n).into_facelets();
        let x = extract_orbit(n, &solved, 5).unwrap();
        assert_eq!(x, (0..24).map(|i| i / 4).collect::<Vec<u8>>());
        assert_eq!(sticker_positions(n).unwrap().len(), 3 * 6 * 49);
    }
}
