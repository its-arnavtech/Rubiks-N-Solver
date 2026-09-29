//! NxN cube simulator, orbits, parity and validation (see `docs/nn/ARCHITECTURE.md` §4–5).
//!
//! Layout, axes and turn direction follow `docs/nn/CONVENTIONS.md` §1–2 exactly.

mod cube;
pub mod geometry;
pub mod identity;
mod moves;
pub mod orbits;
pub mod rng;
mod scramble;
pub mod slots;
mod validate;

pub use cube::{Cube, CubeState, LabeledCube, Layout, SimError};
pub use geometry::{FACE_NAMES, P3, sticker_count};
pub use moves::{
    Axis, MAX_LAYER, Move, ParseMoveError, Turn, allowed_moves, cancel, decode_moves, encode_moves,
    format_moves, invert, parse_moves,
};
pub use orbits::{Orbit, OrbitKind, orbits};
pub use scramble::scramble_moves;
pub use slots::{SlotMap, is_orbit_solved};
pub use validate::{ValidationError, random_state, validate};
