#![warn(missing_docs)]
//! # The ProseMirror API
//!
//! This crate is a re-implementation of the [ProseMirror](https://prosemirror.net) API in Rust.
//! It can be used to create a collaborative editing authority that is able to apply steps to
//! a document.
//!
//! Schemas are defined at runtime via JSON `SchemaSpec` using the [`dynamic`] module.

#[macro_use]
extern crate derive_new;

pub mod binding;
pub mod dynamic;
pub mod editor;
pub mod model;
pub mod transform;
pub mod util;
