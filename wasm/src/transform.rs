//! WASM bindings for prosemirror-transform types.
//!
//! Each struct wraps a `B*` inner value from `prosemirror::binding::transform`
//! and forwards every method via `#[wasm_bindgen]`.

use wasm_bindgen::prelude::*;
