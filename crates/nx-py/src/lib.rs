//! Python module `nxsim` (see `docs/nn/ARCHITECTURE.md` §11). Filled in at M5.1.

use pyo3::prelude::*;

/// Crate version, used by the import smoke test.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn nxsim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
