//! Browser API over `nx-sim` (see `docs/nn/ARCHITECTURE.md` §11). Filled in at M1.6 and M8.2.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
