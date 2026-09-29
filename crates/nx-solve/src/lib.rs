//! Solve phases, baseline solver, move emission, cancellation and verification
//! (see `docs/nn/ARCHITECTURE.md` §7).

pub mod orbit_solver;
pub mod phases;
pub mod solve;

pub use orbit_solver::SolverLib;
pub use solve::{Segment, SolveError, SolveOutput, SolveResult, solve_baseline};
