mod model;
mod transform;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use prosemirror::dynamic::types::Dyn;
use prosemirror::dynamic::{DynamicNode, DynamicSchema};
use prosemirror::transform::Step;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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

fn get_or_create_schema(schema_json: &str) -> PyResult<Arc<DynamicSchema>> {
    let cache = SCHEMA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    // `unwrap_or_else(|e| e.into_inner())` recovers from a poisoned lock
    // (which would only happen if a previous thread panicked mid-insert).
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(existing) = guard.get(schema_json) {
        return Ok(Arc::clone(existing));
    }

    let schema_val: serde_json::Value = serde_json::from_str(schema_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid schema JSON: {e}")))?;
    let schema = DynamicSchema::from_json(&schema_val)
        .map_err(|e| PyValueError::new_err(format!("Invalid schema: {e}")))?;

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
/// strings cross the Python/Rust boundary, keeping data-transfer overhead
/// to the absolute minimum for each operation:
///
/// * Steps arrive as a JSON string → parsed in Rust → applied in Rust.
/// * The document is serialized in Rust → returned as a plain Python ``str``.
///
/// The parsed schema is automatically cached inside Rust, keyed by the exact
/// schema-JSON string.  Constructing many ``Editor`` objects that share the
/// same schema therefore only pays the parse cost once.
#[pyclass(module = "prosemirror_rs")]
struct Editor {
    schema: Arc<DynamicSchema>,
    doc: DynamicNode,
    version: usize,
}

#[pymethods]
impl Editor {
    /// Create a new Editor.
    ///
    /// The parsed schema is cached inside Rust (keyed by the exact
    /// *schema_json* string), so repeated construction with the same schema
    /// only parses it once.
    ///
    /// :param schema_json: ProseMirror schema specification as a JSON string.
    /// :param doc_json: Initial document state as a JSON string.
    /// :raises ValueError: If either string is not valid JSON, or the schema /
    ///     document does not conform to the ProseMirror spec.
    #[new]
    #[pyo3(signature = (schema_json, doc_json))]
    fn new(schema_json: &str, doc_json: &str) -> PyResult<Self> {
        let schema = get_or_create_schema(schema_json)?;

        let doc_val: serde_json::Value = serde_json::from_str(doc_json)
            .map_err(|e| PyValueError::new_err(format!("Invalid document JSON: {e}")))?;
        let doc = schema
            .node_from_json(&doc_val)
            .map_err(|e| PyValueError::new_err(format!("Invalid document: {e}")))?;

        Ok(Editor {
            schema,
            doc,
            version: 0,
        })
    }

    /// Apply a single step to the document.
    ///
    /// :param step_json: The step as a JSON string.
    /// :returns: ``True`` if applied successfully, ``False`` if the step could
    ///     not be applied (document is left unchanged).
    /// :raises ValueError: If *step_json* is not valid JSON or not a
    ///     recognised step type.
    fn apply_step(&mut self, step_json: &str) -> PyResult<bool> {
        let result = {
            let schema = &self.schema;
            let doc = &self.doc;
            schema.with_types(|| -> PyResult<Option<DynamicNode>> {
                let step: Step<Dyn> = serde_json::from_str(step_json)
                    .map_err(|e| PyValueError::new_err(format!("Invalid step JSON: {e}")))?;
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
    /// Python JSON machinery is involved, and no intermediate Python objects
    /// are created.
    ///
    /// All steps are parsed before any are applied, so a malformed JSON array
    /// raises ``ValueError`` without mutating the document.
    ///
    /// The batch is fully atomic: if any step fails to apply the document is
    /// rolled back to its state before the call, leaving it completely
    /// unchanged.  The version counter is likewise rolled back.
    ///
    /// :param steps_json: A JSON array of step objects, e.g.
    ///     ``'[{"stepType":"replace",...},...]'``.
    /// :returns: ``True`` if every step applied successfully; ``False`` if any
    ///     step failed (document and version are rolled back entirely).
    /// :raises ValueError: If *steps_json* is not a valid JSON array of steps.
    fn apply_steps_json(&mut self, steps_json: &str) -> PyResult<bool> {
        // Phase 1: parse the whole array before touching the document.
        let steps: Vec<Step<Dyn>> = {
            let schema = &self.schema;
            schema.with_types(|| {
                serde_json::from_str(steps_json)
                    .map_err(|e| PyValueError::new_err(format!("Invalid steps JSON: {e}")))
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

    /// Apply a batch of steps from a Python list of JSON strings, atomically.
    ///
    /// Use this when steps are constructed or modified in Python (e.g.
    /// programmatically building a step dict and calling ``json.dumps``).
    /// For steps that arrive directly from a network client prefer
    /// :meth:`apply_steps_json` to avoid unnecessary Python-level parsing.
    ///
    /// All steps are parsed before any are applied, so a bad JSON string
    /// raises ``ValueError`` without mutating the document.
    ///
    /// The batch is fully atomic: if any step fails to apply the document is
    /// rolled back to its state before the call, leaving it completely
    /// unchanged.  The version counter is likewise rolled back.
    ///
    /// :param steps: A list where each element is a JSON string for one step.
    /// :returns: ``True`` if every step applied successfully; ``False`` if any
    ///     step failed (document and version are rolled back entirely).
    /// :raises ValueError: If any element is not valid step JSON.
    fn apply_steps(&mut self, steps: Vec<String>) -> PyResult<bool> {
        // Parse all steps up-front so that a bad step raises ValueError
        // before any mutation takes place.
        let parsed: Vec<Step<Dyn>> = {
            let schema = &self.schema;
            schema.with_types(|| {
                steps
                    .iter()
                    .map(|s| {
                        serde_json::from_str::<Step<Dyn>>(s)
                            .map_err(|e| PyValueError::new_err(format!("Invalid step JSON: {e}")))
                    })
                    .collect::<PyResult<Vec<_>>>()
            })?
        };

        // A snapshot is only needed when there are at least two steps: with a
        // single step the document is either untouched (failure) or cleanly
        // advanced (success), so no previously-committed state can need rolling back.
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
    ///
    /// This is more efficient than constructing a brand-new ``Editor`` when
    /// you need to restore a snapshot (e.g. after an unrecoverable conflict),
    /// because the schema is never re-parsed — only the document JSON is
    /// processed.  The version counter is reset to zero.
    ///
    /// :param doc_json: The replacement document as a JSON string.
    /// :raises ValueError: If *doc_json* is not valid JSON or does not
    ///     conform to the schema.
    fn reset(&mut self, doc_json: &str) -> PyResult<()> {
        let doc_val: serde_json::Value = serde_json::from_str(doc_json)
            .map_err(|e| PyValueError::new_err(format!("Invalid document JSON: {e}")))?;
        let doc = self
            .schema
            .node_from_json(&doc_val)
            .map_err(|e| PyValueError::new_err(format!("Invalid document: {e}")))?;
        self.doc = doc;
        self.version = 0;
        Ok(())
    }

    /// Serialize the current document to a JSON string.
    ///
    /// Serialization happens entirely in Rust; only the resulting string is
    /// passed to Python.  This makes the method suitable for saving the
    /// document directly to a database without creating any intermediate
    /// Python dicts or lists.
    ///
    /// When `skip_defaults` is `True`, attributes whose value matches the
    /// schema-defined default are omitted from the output ("mini" JSON).
    ///
    /// :param skip_defaults: If True, omit attributes that have default values.
    /// :returns: The document as a compact JSON string.
    #[pyo3(signature = (skip_defaults = false))]
    fn doc_json(&self, skip_defaults: bool) -> PyResult<String> {
        let val = self.schema.with_types(|| self.doc.to_json(skip_defaults));
        serde_json::to_string(&val)
            .map_err(|e| PyValueError::new_err(format!("Serialization error: {e}")))
    }

    /// Number of steps successfully applied since construction (or last
    /// :meth:`reset`).
    ///
    /// Use as a document version counter in collaborative-editing protocols.
    #[getter]
    fn version(&self) -> usize {
        self.version
    }
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

/// Python bindings for prosemirror-rs.
///
/// Provides a memory- and CPU-efficient interface to ProseMirror's document
/// model and transform pipeline.  Document state lives entirely in Rust; only
/// JSON strings cross the Python/Rust boundary.
///
/// Schema caching:
/// - The first ``Editor(schema_json, ...)`` call parses the schema and stores
///   it in a global Rust cache keyed by the exact schema-JSON string.
/// - All subsequent ``Editor`` constructions with the same string reuse the
///   cached schema at the cost of a single ``Arc`` clone.
///
/// Free-threaded safety (Python 3.13t+):
/// - PyO3's per-object ``RefCell`` prevents data races by raising
///   ``RuntimeError`` when two threads contend on the same ``Editor``.
/// - ``with_types()`` uses a thread-local, so each OS thread has its own
///   isolated context — no cross-thread sharing occurs.
/// - If your application needs multiple threads to share one ``Editor``
///   without hitting ``RuntimeError``, wrap it in a ``threading.Lock``.
#[pymodule]
fn prosemirror_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Editor>()?;
    m.add_class::<model::PySchema>()?;
    m.add_class::<model::PyNodeType>()?;
    m.add_class::<model::PyMarkType>()?;
    m.add_class::<model::PyNode>()?;
    m.add_class::<model::PyFragment>()?;
    m.add_class::<model::PySlice>()?;
    m.add_class::<model::PyResolvedPos>()?;
    m.add_class::<model::PyMark>()?;
    m.add_class::<model::PyMarkSet>()?;
    m.add_class::<transform::PyStepMap>()?;
    m.add_class::<transform::PyMapResult>()?;
    m.add_class::<transform::PyMapping>()?;
    m.add_class::<transform::PyStepResult>()?;
    m.add_class::<transform::PyStep>()?;
    m.add_class::<transform::PyTransform>()?;
    m.add_class::<model::PyNodeRange>()?;
    m.add_function(wrap_pyfunction!(transform::py_lift_target, m)?)?;
    m.add_function(wrap_pyfunction!(transform::py_can_split, m)?)?;
    m.add_function(wrap_pyfunction!(transform::py_find_wrapping, m)?)?;
    m.add_function(wrap_pyfunction!(transform::py_can_join, m)?)?;
    m.add_function(wrap_pyfunction!(transform::py_join_point, m)?)?;
    m.add_function(wrap_pyfunction!(transform::py_insert_point, m)?)?;
    m.add_function(wrap_pyfunction!(transform::py_drop_point, m)?)?;
    Ok(())
}
