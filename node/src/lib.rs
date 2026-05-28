mod model;
mod transform;

use napi::bindgen_prelude::*;
use napi_derive::napi;

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/// A stateful ProseMirror document editor backed by Rust.
///
/// The schema and document state live entirely in Rust memory.  Only JSON
/// strings cross the JavaScript/Rust boundary, keeping data-transfer overhead
/// to the absolute minimum for each operation:
///
/// * Steps arrive as a JSON string → parsed in Rust → applied in Rust.
/// * The document is serialized in Rust → returned as a plain JS `string`.
///
/// The parsed schema is automatically cached inside Rust, keyed by the exact
/// schema-JSON string.  Constructing many `Editor` objects that share the
/// same schema therefore only pays the parse cost once.
#[napi]
pub struct Editor {
    inner: prosemirror::editor::Editor,
}

#[napi]
impl Editor {
    /// Create a new Editor.
    ///
    /// The parsed schema is cached inside Rust (keyed by the exact
    /// `schemaJson` string), so repeated construction with the same schema
    /// only parses it once.
    ///
    /// @param schemaJson ProseMirror schema specification as a JSON string.
    /// @param docJson Initial document state as a JSON string.
    /// @throws {Error} If either string is not valid JSON, or the schema /
    ///   document does not conform to the ProseMirror spec.
    #[napi(constructor)]
    pub fn new(schema_json: String, doc_json: String) -> napi::Result<Self> {
        let inner = prosemirror::editor::Editor::new(&schema_json, &doc_json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))?;
        Ok(Editor { inner })
    }

    /// Apply a single step to the document.
    ///
    /// @param stepJson The step as a JSON string.
    /// @returns `true` if applied successfully, `false` if the step could not
    ///   be applied (document is left unchanged).
    /// @throws {Error} If `stepJson` is not valid JSON or not a recognised step type.
    #[napi]
    pub fn apply_step(&mut self, step_json: String) -> napi::Result<bool> {
        self.inner
            .apply_step(&step_json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    /// Apply a batch of steps supplied as a single JSON array string, atomically.
    ///
    /// **This is the preferred method when steps arrive from a network client.**
    /// The entire string is handed to Rust and parsed there in one pass — no
    /// JS JSON machinery is involved, and no intermediate JS objects are created.
    ///
    /// All steps are parsed before any are applied, so a malformed JSON array
    /// throws without mutating the document.
    ///
    /// The batch is fully atomic: if any step fails to apply the document is
    /// rolled back to its state before the call, leaving it completely
    /// unchanged.  The version counter is likewise rolled back.
    ///
    /// @param stepsJson A JSON array of step objects, e.g.
    ///   `'[{"stepType":"replace",...},...]'`.
    /// @returns `true` if every step applied successfully; `false` if any
    ///   step failed (document and version are rolled back entirely).
    /// @throws {Error} If `stepsJson` is not a valid JSON array of steps.
    #[napi]
    pub fn apply_steps_json(&mut self, steps_json: String) -> napi::Result<bool> {
        self.inner
            .apply_steps_json(&steps_json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    /// Apply a batch of steps from a JS array of JSON strings, atomically.
    ///
    /// Use this when steps are constructed or modified in JS (e.g.
    /// programmatically building a step object and calling `JSON.stringify`).
    /// For steps that arrive directly from a network client prefer
    /// `applyStepsJson` to avoid unnecessary JS-level parsing.
    ///
    /// All steps are parsed before any are applied, so a bad JSON string
    /// throws without mutating the document.
    ///
    /// The batch is fully atomic: if any step fails to apply the document is
    /// rolled back to its state before the call, leaving it completely
    /// unchanged.  The version counter is likewise rolled back.
    ///
    /// @param steps An array where each element is a JSON string for one step.
    /// @returns `true` if every step applied successfully; `false` if any
    ///   step failed (document and version are rolled back entirely).
    /// @throws {Error} If any element is not valid step JSON.
    #[napi]
    pub fn apply_steps(&mut self, steps: Vec<String>) -> napi::Result<bool> {
        self.inner
            .apply_steps(&steps)
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    /// Reset the document to a new state, reusing the already-parsed schema.
    ///
    /// This is more efficient than constructing a brand-new `Editor` when
    /// you need to restore a snapshot (e.g. after an unrecoverable conflict),
    /// because the schema is never re-parsed — only the document JSON is
    /// processed.  The version counter is reset to zero.
    ///
    /// @param docJson The replacement document as a JSON string.
    /// @throws {Error} If `docJson` is not valid JSON or does not conform to
    ///   the schema.
    #[napi]
    pub fn reset(&mut self, doc_json: String) -> napi::Result<()> {
        self.inner
            .reset(&doc_json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    /// Serialize the current document to a JSON string.
    ///
    /// Serialization happens entirely in Rust; only the resulting string is
    /// passed to JavaScript.  This makes the method suitable for saving the
    /// document directly to a database without creating any intermediate
    /// JS objects.
    ///
    /// When `skipDefaults` is `true`, attributes whose value matches the
    /// schema-defined default are omitted from the output ("mini" JSON).
    ///
    /// @param skipDefaults If true, omit attributes that have default values.
    /// @returns The document as a compact JSON string.
    #[napi]
    pub fn doc_json(&self, skip_defaults: Option<bool>) -> napi::Result<String> {
        self.inner
            .doc_json(skip_defaults.unwrap_or(false))
            .map_err(|e| napi::Error::new(Status::GenericFailure, e))
    }

    /// Number of steps successfully applied since construction (or last `reset()`).
    ///
    /// Use as a document version counter in collaborative-editing protocols.
    #[napi(getter)]
    pub fn version(&self) -> u32 {
        self.inner.version() as u32
    }
}
