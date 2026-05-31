//! WebAssembly bindings for prosemirror-rs.
//!
//! This crate exposes the prosemirror model and transform APIs via
//! wasm-bindgen, targeting the `web` target for use in browsers and
//! bundlers (webpack, vite, etc.).
//!
//! The binding-neutral `B*` wrapper layer (`prosemirror::binding`) is
//! reused directly.  Each WASM struct holds a `B*` inner value and
//! forwards every method.

mod model;
mod transform;

// Re-export everything at the crate root so wasm-bindgen picks it up.
pub use model::*;
pub use transform::*;
