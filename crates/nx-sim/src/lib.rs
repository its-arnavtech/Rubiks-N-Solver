//! NxN cube simulator, orbits, parity and validation (see `docs/nn/ARCHITECTURE.md` §4–5).
//!
//! Layout, axes and turn direction follow `docs/nn/CONVENTIONS.md` §1–2 exactly.

mod cube;
pub mod geometry;
mod moves;
pub mod rng;

pub use cube::{Cube, CubeState, LabeledCube, Layout, SimError};
pub use geometry::{FACE_NAMES, P3, sticker_count};
pub use moves::{
    Axis, MAX_LAYER, Move, ParseMoveError, allowed_moves, decode_moves, encode_moves, format_moves,
    invert, parse_moves,
};
