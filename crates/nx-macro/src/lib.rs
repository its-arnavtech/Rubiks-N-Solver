//! Symbolic moves, macro discovery and verification, action library
//! (see `docs/nn/ARCHITECTURE.md` §6).

pub mod discover;
pub mod effect;
pub mod library;
pub mod sym;
pub mod verify;

pub use effect::{Effect, MacroKind, OrbitProbe, orientation_mod};
pub use library::{LIBRARY_VERSION, Library, TypeLibrary};
pub use sym::{Binding, LayerRef, ParseSymError, SymMove};

/// Orbit types that have macros, in type-id order.
pub const MACRO_KINDS: [nx_sim::OrbitKind; 7] = [
    nx_sim::OrbitKind::Corner,
    nx_sim::OrbitKind::MidEdge,
    nx_sim::OrbitKind::Wing,
    nx_sim::OrbitKind::XCenter,
    nx_sim::OrbitKind::PlusCenter,
    nx_sim::OrbitKind::ObliqueA,
    nx_sim::OrbitKind::ObliqueB,
];

/// The corner orbit's fixed slot (DBL).
pub(crate) fn slots_fixed_corner() -> usize {
    nx_sim::slots::FIXED_CORNER_SLOT
}

/// Default location of the committed library, relative to the repo root.
pub const LIBRARY_PATH: &str = "artifacts/macros/library.json";
