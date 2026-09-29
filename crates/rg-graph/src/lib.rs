//! Puzzle-agnostic graph engine (see `docs/06-graph-engine.md`).
//!
//! This crate deliberately does not depend on `rg-cube`: it knows about integer
//! coordinate spaces, distances and search, and nothing about cubes.

pub mod bfs;
pub mod combinatorics;
pub mod ida;
