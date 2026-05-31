//! WASM bindings for prosemirror-model types.
//!
//! Each struct wraps a `B*` inner value from `prosemirror::binding::model`
//! and forwards every method via `#[wasm_bindgen]`.

use wasm_bindgen::prelude::*;
