//! Language-agnostic binding layer.
//!
#![allow(missing_docs)]
//!
//! This module provides binding-neutral wrappers (prefixed `B`) around the
//! prosemirror model and transform types.  Each language binding (Node.js,
//! Python, future WASM, etc.) holds these wrappers internally and provides
//! only the FFI-specific glue (attribute macros, error type translation,
//! callback adaptation).
//!
//! Adding a new method here makes it available to every binding with a
//! single one-line forwarding wrapper.

pub mod model;
pub mod transform;
