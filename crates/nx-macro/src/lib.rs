//! Symbolic moves, macro discovery and verification, action library
//! (see `docs/nn/ARCHITECTURE.md` §6).

pub mod sym;

pub use sym::{Binding, LayerRef, ParseSymError, SymMove};
