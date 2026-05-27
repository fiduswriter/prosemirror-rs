use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use prosemirror::dynamic::types::Dyn;
use prosemirror::dynamic::{DynamicNode, DynamicSchema};
use prosemirror::transform::Step;

// ---------------------------------------------------------------------------
// Schema cache
// ---------------------------------------------------------------------------

/// Global cache mapping raw schema-JSON strings → parsed schemas.
///
/// Keyed by the exact bytes of the JSON string, so two textually-identical
/// strings always hit the same entry.  Parsing a schema is the expensive
/// part of Editor construction; once cached, every subsequent `Editor::new`
/// for the same schema is just an `Arc` clone + a document parse.
static SCHEMA_CACHE: OnceLock<Mutex<HashMap<String, Arc<DynamicSchema>>>> = OnceLock::new();

fn get_or_create_schema(schema_json: &str) -> napi::Result<Arc<DynamicSchema>> {
    let cache = SCHEMA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    // Recover from a poisoned lock (would only happen if a previous thread panicked mid-insert).
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(existing) = guard.get(schema_json) {
        return Ok(Arc::clone(existing));
    }

    let schema_val: serde_json::Value = serde_json::from_str(schema_json)
        .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid schema JSON: {e}")))?;
    let schema = DynamicSchema::from_json(&schema_val)
        .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid schema: {e}")))?;

    let arc = Arc::new(schema);
    guard.insert(schema_json.to_owned(), Arc::clone(&arc));
    Ok(arc)
}

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
    schema: Arc<DynamicSchema>,
    doc: DynamicNode,
    version: usize,
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
        let schema = get_or_create_schema(&schema_json)?;

        let doc_val: serde_json::Value = serde_json::from_str(&doc_json).map_err(|e| {
            napi::Error::new(Status::InvalidArg, format!("Invalid document JSON: {e}"))
        })?;
        let doc = schema
            .node_from_json(&doc_val)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid document: {e}")))?;

        Ok(Editor {
            schema,
            doc,
            version: 0,
        })
    }

    /// Apply a single step to the document.
    ///
    /// @param stepJson The step as a JSON string.
    /// @returns `true` if applied successfully, `false` if the step could not
    ///   be applied (document is left unchanged).
    /// @throws {Error} If `stepJson` is not valid JSON or not a recognised step type.
    #[napi]
    pub fn apply_step(&mut self, step_json: String) -> napi::Result<bool> {
        let result = {
            let schema = &self.schema;
            let doc = &self.doc;
            schema.with_types(|| -> napi::Result<Option<DynamicNode>> {
                let step: Step<Dyn> = serde_json::from_str(&step_json).map_err(|e| {
                    napi::Error::new(Status::InvalidArg, format!("Invalid step JSON: {e}"))
                })?;
                Ok(step.apply(doc).ok())
            })
        }?;

        if let Some(new_doc) = result {
            self.doc = new_doc;
            self.version += 1;
            Ok(true)
        } else {
            Ok(false)
        }
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
        // Phase 1: parse the whole array before touching the document.
        let steps: Vec<Step<Dyn>> = {
            let schema = &self.schema;
            schema.with_types(|| {
                serde_json::from_str(&steps_json).map_err(|e| {
                    napi::Error::new(Status::InvalidArg, format!("Invalid steps JSON: {e}"))
                })
            })?
        };

        // A snapshot is only needed when there are at least two steps: with a
        // single step the document is either untouched (failure) or cleanly
        // advanced (success), so no previously-committed state can need rolling back.
        let mut snapshot: Option<(DynamicNode, usize)> = if steps.len() > 1 {
            Some((self.doc.clone(), self.version))
        } else {
            None
        };

        // Phase 2: apply each step; roll back and return false on the first failure.
        for step in steps {
            let result = {
                let schema = &self.schema;
                let doc = &self.doc;
                schema.with_types(|| step.apply(doc))
            };
            match result {
                Ok(new_doc) => {
                    self.doc = new_doc;
                    self.version += 1;
                }
                Err(_) => {
                    // Option::take moves the contents out via &mut self rather
                    // than an unconditional move, which the borrow checker would
                    // reject inside a loop body.
                    if let Some((snap_doc, snap_version)) = snapshot.take() {
                        self.doc = snap_doc;
                        self.version = snap_version;
                    }
                    return Ok(false);
                }
            }
        }
        Ok(true)
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
        // Parse all steps up-front so that a bad step throws
        // before any mutation takes place.
        let parsed: Vec<Step<Dyn>> = {
            let schema = &self.schema;
            schema.with_types(|| {
                steps
                    .iter()
                    .map(|s| {
                        serde_json::from_str::<Step<Dyn>>(s).map_err(|e| {
                            napi::Error::new(Status::InvalidArg, format!("Invalid step JSON: {e}"))
                        })
                    })
                    .collect::<napi::Result<Vec<_>>>()
            })?
        };

        // Snapshot the pre-batch state so we can roll back cheaply.
        let snapshot = self.doc.clone();
        let snapshot_version = self.version;

        for step in parsed {
            let result = {
                let schema = &self.schema;
                let doc = &self.doc;
                schema.with_types(|| step.apply(doc))
            };
            match result {
                Ok(new_doc) => {
                    self.doc = new_doc;
                    self.version += 1;
                }
                Err(_) => {
                    self.doc = snapshot;
                    self.version = snapshot_version;
                    return Ok(false);
                }
            }
        }
        Ok(true)
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
        let doc_val: serde_json::Value = serde_json::from_str(&doc_json).map_err(|e| {
            napi::Error::new(Status::InvalidArg, format!("Invalid document JSON: {e}"))
        })?;
        let doc = self
            .schema
            .node_from_json(&doc_val)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid document: {e}")))?;
        self.doc = doc;
        self.version = 0;
        Ok(())
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
        let val = self
            .schema
            .with_types(|| self.doc.to_json(skip_defaults.unwrap_or(false)));
        serde_json::to_string(&val).map_err(|e| {
            napi::Error::new(Status::GenericFailure, format!("Serialization error: {e}"))
        })
    }

    /// Number of steps successfully applied since construction (or last `reset()`).
    ///
    /// Use as a document version counter in collaborative-editing protocols.
    #[napi(getter)]
    pub fn version(&self) -> u32 {
        self.version as u32
    }
}
