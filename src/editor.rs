//! Shared stateful editor implementation for language bindings.
//!
//! This module provides a language-agnostic [`Editor`] that handles schema
//! caching, atomic step application, and document serialization.  Both the
//! Python and Node.js bindings are thin wrappers around this type.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::dynamic::{DynamicNode, DynamicSchema};
use crate::transform::Step;

// ---------------------------------------------------------------------------
// Schema cache
// ---------------------------------------------------------------------------

static SCHEMA_CACHE: OnceLock<Mutex<HashMap<String, Arc<DynamicSchema>>>> = OnceLock::new();

/// Parse a schema from JSON, re-using a globally-cached value when the same
/// JSON string has been seen before.
pub fn get_or_create_schema(schema_json: &str) -> Result<Arc<DynamicSchema>, String> {
    let cache = SCHEMA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(existing) = guard.get(schema_json) {
        return Ok(Arc::clone(existing));
    }

    let schema_val: serde_json::Value =
        serde_json::from_str(schema_json).map_err(|e| format!("Invalid schema JSON: {e}"))?;
    let schema =
        DynamicSchema::from_json(&schema_val).map_err(|e| format!("Invalid schema: {e}"))?;

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
/// strings cross the language boundary, keeping data-transfer overhead to the
/// absolute minimum for each operation.
pub struct Editor {
    schema: Arc<DynamicSchema>,
    doc: DynamicNode,
    version: usize,
}

impl Editor {
    /// Create a new Editor.
    ///
    /// The parsed schema is cached inside Rust (keyed by the exact
    /// *schema_json* string), so repeated construction with the same schema
    /// only parses it once.
    pub fn new(schema_json: &str, doc_json: &str) -> Result<Self, String> {
        let schema = get_or_create_schema(schema_json)?;

        let doc_val: serde_json::Value =
            serde_json::from_str(doc_json).map_err(|e| format!("Invalid document JSON: {e}"))?;
        let doc = schema
            .node_from_json(&doc_val)
            .map_err(|e| format!("Invalid document: {e}"))?;

        Ok(Editor {
            schema,
            doc,
            version: 0,
        })
    }

    /// Apply a single step to the document.
    ///
    /// Returns `true` if applied successfully, `false` if the step could not
    /// be applied (document is left unchanged).
    pub fn apply_step(&mut self, step_json: &str) -> Result<bool, String> {
        let result = {
            let schema = &self.schema;
            let doc = &self.doc;
            schema.with_types(|| -> Result<Option<DynamicNode>, String> {
                let step: Step<crate::dynamic::types::Dyn> = serde_json::from_str(step_json)
                    .map_err(|e| format!("Invalid step JSON: {e}"))?;
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
    /// All steps are parsed before any are applied, so a malformed JSON array
    /// returns an error without mutating the document.
    ///
    /// The batch is fully atomic: if any step fails to apply the document is
    /// rolled back to its state before the call.
    pub fn apply_steps_json(&mut self, steps_json: &str) -> Result<bool, String> {
        let steps: Vec<Step<crate::dynamic::types::Dyn>> = {
            let schema = &self.schema;
            schema.with_types(|| {
                serde_json::from_str(steps_json).map_err(|e| format!("Invalid steps JSON: {e}"))
            })?
        };

        let mut snapshot: Option<(DynamicNode, usize)> = if steps.len() > 1 {
            Some((self.doc.clone(), self.version))
        } else {
            None
        };

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

    /// Apply a batch of steps from a Vec of JSON strings, atomically.
    ///
    /// All steps are parsed before any are applied, so a bad JSON string
    /// returns an error without mutating the document.
    pub fn apply_steps(&mut self, steps: &[String]) -> Result<bool, String> {
        let parsed: Vec<Step<crate::dynamic::types::Dyn>> = {
            let schema = &self.schema;
            schema.with_types(|| {
                steps
                    .iter()
                    .map(|s| {
                        serde_json::from_str::<Step<crate::dynamic::types::Dyn>>(s)
                            .map_err(|e| format!("Invalid step JSON: {e}"))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })?
        };

        let mut snapshot: Option<(DynamicNode, usize)> = if parsed.len() > 1 {
            Some((self.doc.clone(), self.version))
        } else {
            None
        };

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

    /// Reset the document to a new state, reusing the already-parsed schema.
    pub fn reset(&mut self, doc_json: &str) -> Result<(), String> {
        let doc_val: serde_json::Value =
            serde_json::from_str(doc_json).map_err(|e| format!("Invalid document JSON: {e}"))?;
        let doc = self
            .schema
            .node_from_json(&doc_val)
            .map_err(|e| format!("Invalid document: {e}"))?;
        self.doc = doc;
        self.version = 0;
        Ok(())
    }

    /// Serialize the current document to a JSON string.
    ///
    /// When `skip_defaults` is `true`, attributes whose value matches the
    /// schema-defined default are omitted from the output ("mini" JSON).
    pub fn doc_json(&self, skip_defaults: bool) -> Result<String, String> {
        let val = self.schema.with_types(|| self.doc.to_json(skip_defaults));
        serde_json::to_string(&val).map_err(|e| format!("Serialization error: {e}"))
    }

    /// Number of steps successfully applied since construction (or last reset).
    pub fn version(&self) -> usize {
        self.version
    }

    /// Immutable access to the underlying schema.
    pub fn schema(&self) -> &Arc<DynamicSchema> {
        &self.schema
    }

    /// Immutable access to the underlying document.
    pub fn doc(&self) -> &DynamicNode {
        &self.doc
    }

    /// Mutable access to the underlying document.
    pub fn doc_mut(&mut self) -> &mut DynamicNode {
        &mut self.doc
    }
}
