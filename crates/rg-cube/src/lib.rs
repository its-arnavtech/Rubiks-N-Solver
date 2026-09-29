//! Geometry-derived NxN cube model (see `docs/05-cube-model.md`).
//!
//! Every move permutation is computed from integer sticker coordinates and rotation
//! maps; nothing is typed in by hand.

mod cube;
mod frame;
mod geometry;
mod moves;
mod notation;
mod scramble;

pub use cube::{Cube, CubeError};
pub use frame::{conjugate, sequence_permutation, whole_cube_rotations};
pub use geometry::{Geometry, P3, cubie_count, rotate_cw};
pub use moves::{Axis, Move, generator_moves, invert, simplify};
pub use notation::{ParseError, format_alg, format_move, parse_alg};
pub use scramble::{SplitMix64, random_move_scramble, scramble_length};

/// Face order used everywhere: U, R, F, D, L, B (Kociemba's convention).
pub const FACE_NAMES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

/// Smallest and largest supported cube sizes. Layer masks are `u8`, so N ≤ 8.
pub const MIN_N: u8 = 2;
pub const MAX_N: u8 = 7;
