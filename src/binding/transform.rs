//! Language-agnostic binding layer for ProseMirror transform types.
#![allow(clippy::should_implement_trait)] // We expose JS-style methods, not
                                          // standard trait impls.

use std::sync::Arc;

use serde_json::Value;

use super::model::{BNode, BNodeRange, BSlice};
use crate::dynamic::{
    types::{Dyn, DynamicMark, DynamicNode, DynamicNodeType},
    DynamicSchema,
};
use crate::model::{Fragment, MarkSet, Slice};
use crate::transform::{
    map::{MapResult, Mappable, Mapping, StepMap},
    structure::{
        can_join, can_split, drop_point, find_wrapping, insert_point, join_point, lift_target,
    },
    AddMarkStep, AddNodeMarkStep, AttrStep, DocAttrStep, MarkOrType, RemoveMarkStep,
    RemoveNodeMarkStep, ReplaceAroundStep, ReplaceStep, Span, Step, Transform, Wrapper,
};

// ---------------------------------------------------------------------------
// BStepMap
// ---------------------------------------------------------------------------

/// Binding wrapper for [`StepMap`].
pub struct BStepMap {
    pub inner: StepMap,
}

impl BStepMap {
    /// Create a new `BStepMap` from raw range triples.
    pub fn new(ranges: Vec<usize>) -> Self {
        BStepMap {
            inner: StepMap::new(ranges),
        }
    }

    /// The raw range triples.
    pub fn ranges(&self) -> &[usize] {
        &self.inner.ranges
    }

    /// Map a position, returning the new position.
    pub fn map(&self, pos: usize, assoc: i32) -> usize {
        self.inner.map(pos, assoc)
    }

    /// Map a position, returning a full [`BMapResult`].
    pub fn map_result(&self, pos: usize, assoc: i32) -> BMapResult {
        BMapResult {
            inner: self.inner.map_result(pos, assoc),
        }
    }

    /// Recover a position from a packed recovery value.
    pub fn recover(&self, value: usize) -> Option<usize> {
        self.inner.recover(value)
    }

    /// Return the inverse of this map.
    pub fn invert(&self) -> BStepMap {
        BStepMap {
            inner: self.inner.invert(),
        }
    }

    /// Iterate over the old/new range pairs.
    pub fn for_each<F: FnMut(usize, usize, usize, usize)>(&self, f: F) {
        self.inner.for_each(f)
    }

    /// Create a simple offset map.
    pub fn offset(n: isize) -> BStepMap {
        BStepMap {
            inner: StepMap::offset(n),
        }
    }

    /// Create an empty step map.
    pub fn empty_map() -> BStepMap {
        BStepMap {
            inner: StepMap::empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// BMapResult
// ---------------------------------------------------------------------------

/// Binding wrapper for [`MapResult`].
pub struct BMapResult {
    pub inner: MapResult,
}

impl BMapResult {
    /// The mapped position.
    pub fn pos(&self) -> usize {
        self.inner.pos
    }

    /// Whether the position was deleted (from the side indicated by assoc).
    pub fn deleted(&self) -> bool {
        self.inner.deleted()
    }

    /// Whether the position was deleted before.
    pub fn deleted_before(&self) -> bool {
        self.inner.deleted_before()
    }

    /// Whether the position was deleted after.
    pub fn deleted_after(&self) -> bool {
        self.inner.deleted_after()
    }

    /// Whether the position was deleted across.
    pub fn deleted_across(&self) -> bool {
        self.inner.deleted_across()
    }

    /// Recovery value for mirror-based position recovery.
    pub fn recover(&self) -> Option<usize> {
        self.inner.recover
    }
}

// ---------------------------------------------------------------------------
// BMapping
// ---------------------------------------------------------------------------

/// Binding wrapper for [`Mapping`].
pub struct BMapping {
    pub inner: Mapping,
}

impl BMapping {
    /// Create a new empty mapping.
    pub fn new() -> Self {
        BMapping {
            inner: Mapping::new(),
        }
    }

    /// The individual step maps (cloned).
    pub fn maps(&self) -> Vec<BStepMap> {
        self.inner
            .maps
            .iter()
            .map(|m| BStepMap { inner: m.clone() })
            .collect()
    }

    /// The end index (exclusive) of the active range of maps.
    pub fn to_end(&self) -> usize {
        self.inner.to
    }

    /// Map a position, returning just the new position.
    pub fn map(&self, pos: usize, assoc: i32) -> usize {
        self.inner.map(pos, assoc)
    }

    /// Map a position, returning a full [`BMapResult`].
    pub fn map_result(&self, pos: usize, assoc: i32) -> BMapResult {
        BMapResult {
            inner: self.inner.map_result(pos, assoc),
        }
    }

    /// Push a new step map, optionally recording a mirror pair.
    pub fn append_map(&mut self, map: BStepMap, mirrors: Option<usize>) {
        self.inner.append_map(map.inner, mirrors);
    }

    /// Append another mapping.
    pub fn append_mapping(&mut self, other: &BMapping) {
        self.inner.append_mapping(&other.inner);
    }

    /// Append the inverse of another mapping in reverse order.
    pub fn append_mapping_inverted(&mut self, other: &BMapping) {
        self.inner.append_mapping_inverted(&other.inner);
    }

    /// Find the mirror of step `n`.
    pub fn get_mirror(&self, n: usize) -> Option<usize> {
        self.inner.get_mirror(n)
    }

    /// Record a mirror pair.
    pub fn set_mirror(&mut self, n: usize, m: usize) {
        self.inner.set_mirror(n, m);
    }

    /// Return the inverse of this mapping.
    pub fn invert(&self) -> BMapping {
        BMapping {
            inner: self.inner.invert(),
        }
    }

    /// Return a sub-mapping view.
    pub fn slice(&self, from: usize, to: Option<usize>) -> BMapping {
        BMapping {
            inner: self.inner.slice(from, to),
        }
    }
}

impl Default for BMapping {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BStep
// ---------------------------------------------------------------------------

/// Binding wrapper for [`Step<Dyn>`].
pub struct BStep {
    pub inner: Step<Dyn>,
}

impl BStep {
    /// Deserialize a step from JSON, using the given schema for type resolution.
    pub fn from_json(schema: &Arc<DynamicSchema>, val: Value) -> Result<BStep, String> {
        schema
            .with_types(|| serde_json::from_value(val))
            .map(|inner| BStep { inner })
            .map_err(|e| format!("{e:?}"))
    }

    /// Serialize this step to JSON.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(&self.inner).unwrap_or(Value::Null)
    }

    /// Apply this step to a document, returning the new document.
    pub fn apply(&self, doc: &BNode) -> Result<BNode, String> {
        doc.schema
            .with_types(|| self.inner.apply(&doc.inner))
            .map(|n| BNode {
                schema: doc.schema.clone(),
                inner: n,
            })
            .map_err(|e| format!("{e:?}"))
    }

    /// Get the [`BStepMap`] describing the position offset caused by this step.
    pub fn get_map(&self) -> BStepMap {
        BStepMap {
            inner: self.inner.get_map(),
        }
    }

    /// Return the inverse of this step.
    pub fn invert(&self, doc: &BNode) -> BStep {
        BStep {
            inner: doc.schema.with_types(|| self.inner.invert(&doc.inner)),
        }
    }

    /// Map this step through a mapping. Returns `None` if the step was invalidated.
    pub fn map_through(&self, mapping: &BMapping) -> Option<BStep> {
        self.inner.map(&mapping.inner).map(|s| BStep { inner: s })
    }

    /// Attempt to merge this step with another step.
    pub fn merge_with(&self, other: &BStep) -> Option<BStep> {
        self.inner.merge(&other.inner).map(|s| BStep { inner: s })
    }

    // -----------------------------------------------------------------------
    // Factory methods
    // -----------------------------------------------------------------------

    /// Create a `ReplaceStep`.
    pub fn make_replace(from: usize, to: usize, slice: Slice<Dyn>, structure: bool) -> BStep {
        BStep {
            inner: Step::Replace(ReplaceStep {
                span: Span { from, to },
                slice,
                structure,
            }),
        }
    }

    /// Create a `ReplaceAroundStep`.
    pub fn make_replace_around(
        from: usize,
        to: usize,
        gap_from: usize,
        gap_to: usize,
        slice: Slice<Dyn>,
        insert: usize,
        structure: bool,
    ) -> BStep {
        BStep {
            inner: Step::ReplaceAround(ReplaceAroundStep {
                span: Span { from, to },
                gap_from,
                gap_to,
                slice,
                insert,
                structure,
            }),
        }
    }

    /// Create an `AddMarkStep`.
    pub fn make_add_mark(from: usize, to: usize, mark: DynamicMark) -> BStep {
        BStep {
            inner: Step::AddMark(AddMarkStep {
                span: Span { from, to },
                mark,
            }),
        }
    }

    /// Create a `RemoveMarkStep`.
    pub fn make_remove_mark(from: usize, to: usize, mark: DynamicMark) -> BStep {
        BStep {
            inner: Step::RemoveMark(RemoveMarkStep {
                span: Span { from, to },
                mark,
            }),
        }
    }

    /// Create an `AddNodeMarkStep`.
    pub fn make_add_node_mark(pos: usize, mark: DynamicMark) -> BStep {
        BStep {
            inner: Step::AddNodeMark(AddNodeMarkStep { pos, mark }),
        }
    }

    /// Create a `RemoveNodeMarkStep`.
    pub fn make_remove_node_mark(pos: usize, mark: DynamicMark) -> BStep {
        BStep {
            inner: Step::RemoveNodeMark(RemoveNodeMarkStep { pos, mark }),
        }
    }

    /// Create an `AttrStep`.
    pub fn make_attr(pos: usize, attr: String, value: Value) -> BStep {
        BStep {
            inner: Step::Attr(AttrStep { pos, attr, value }),
        }
    }

    /// Create a `DocAttrStep`.
    pub fn make_doc_attr(attr: String, value: Value) -> BStep {
        BStep {
            inner: Step::DocAttr(DocAttrStep { attr, value }),
        }
    }
}

// ---------------------------------------------------------------------------
// BStepResult
// ---------------------------------------------------------------------------

/// The result of applying a step in the binding layer.
pub struct BStepResult {
    pub schema: Arc<DynamicSchema>,
    pub doc: Option<DynamicNode>,
    pub failed: Option<String>,
}

/// Wrap a `Result<DynamicNode, E>` into a [`BStepResult`].
pub fn wrap_step_result(
    schema: Arc<DynamicSchema>,
    result: Result<DynamicNode, impl std::fmt::Debug>,
) -> BStepResult {
    match result {
        Ok(doc) => BStepResult {
            schema,
            doc: Some(doc),
            failed: None,
        },
        Err(e) => BStepResult {
            schema,
            doc: None,
            failed: Some(format!("{e:?}")),
        },
    }
}

// ---------------------------------------------------------------------------
// BTransform
// ---------------------------------------------------------------------------

/// Binding wrapper for [`Transform<Dyn>`].
pub struct BTransform {
    pub schema: Arc<DynamicSchema>,
    pub inner: Transform<Dyn>,
}

impl BTransform {
    /// Create a new transform starting from the given document.
    pub fn new(doc: &BNode) -> Self {
        let schema = doc.schema.clone();
        let inner = schema.with_types(|| Transform::new(doc.inner.clone()));
        BTransform { schema, inner }
    }

    /// The current document.
    pub fn doc(&self) -> BNode {
        BNode {
            schema: self.schema.clone(),
            inner: self.inner.doc.clone(),
        }
    }

    /// The document before all steps were applied.
    pub fn before(&self) -> BNode {
        BNode {
            schema: self.schema.clone(),
            inner: self.inner.before().clone(),
        }
    }

    /// The steps applied so far (cloned).
    pub fn steps(&self) -> Vec<BStep> {
        self.inner
            .steps
            .iter()
            .map(|s| BStep { inner: s.clone() })
            .collect()
    }

    /// The intermediate documents (cloned).
    pub fn docs(&self) -> Vec<BNode> {
        self.inner
            .docs
            .iter()
            .map(|d| BNode {
                schema: self.schema.clone(),
                inner: d.clone(),
            })
            .collect()
    }

    /// The accumulated mapping.
    pub fn mapping(&self) -> BMapping {
        BMapping {
            inner: self.inner.mapping.clone(),
        }
    }

    /// Whether any steps have been applied.
    pub fn doc_changed(&self) -> bool {
        self.inner.doc_changed()
    }

    /// Apply a step, raising on failure.
    pub fn step(&mut self, step: &BStep) -> Result<(), String> {
        let schema = Arc::clone(&self.schema);
        schema
            .with_types(|| self.inner.step(step.inner.clone()))
            .map_err(|e| format!("{e:?}"))
            .map(|_| ())
    }

    /// Apply a step, returning an error message on failure and `None` on success.
    pub fn maybe_step(&mut self, step: &BStep) -> Option<String> {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.maybe_step(step.inner.clone()))
    }

    /// Return the total changed range, or `None` if no changes.
    pub fn changed_range(&self) -> Option<(usize, usize)> {
        self.inner.changed_range()
    }

    /// Low-level replace.
    pub fn replace(&mut self, from: usize, to: Option<usize>, slice: Option<Slice<Dyn>>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.replace(from, to, slice));
    }

    /// Replace a range with specific content.
    pub fn replace_with(&mut self, from: usize, to: usize, content: Fragment<Dyn>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.replace_with(from, to, content));
    }

    /// Delete the content between two positions.
    pub fn delete(&mut self, from: usize, to: usize) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.delete(from, to));
    }

    /// Insert content at a position.
    pub fn insert(&mut self, pos: usize, content: Fragment<Dyn>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.insert(pos, content));
    }

    /// Add a mark to the inline content in the given range.
    pub fn add_mark(&mut self, from: usize, to: usize, mark: DynamicMark) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.add_mark(from, to, mark));
    }

    /// Remove mark(s) from the inline content in the given range.
    pub fn remove_mark(&mut self, from: usize, to: usize, mark: Option<MarkOrType<Dyn>>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.remove_mark(from, to, mark));
    }

    /// Add a mark to a specific node.
    pub fn add_node_mark(&mut self, pos: usize, mark: DynamicMark) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.add_node_mark(pos, mark));
    }

    /// Remove a mark (or all marks of a type) from a specific node.
    pub fn remove_node_mark(&mut self, pos: usize, mark: MarkOrType<Dyn>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.remove_node_mark(pos, mark));
    }

    /// Replace a range with a slice, attempting to fit the slice into the range.
    pub fn replace_range(&mut self, from: usize, to: usize, slice: Slice<Dyn>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.replace_range(from, to, slice));
    }

    /// Replace a range with a single node.
    pub fn replace_range_with(&mut self, from: usize, to: usize, node: DynamicNode) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.replace_range_with(from, to, node));
    }

    /// Delete a range, adjusting to avoid leaving invalid structure.
    pub fn delete_range(&mut self, from: usize, to: usize) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.delete_range(from, to));
    }

    /// Lift content out of its parent node to the given target depth.
    pub fn lift(&mut self, range: &BNodeRange, target: usize) -> Result<(), String> {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| {
            let node_range = range
                .to_node_range()
                .ok_or_else(|| "invalid range".to_string())?;
            self.inner.lift(&node_range, target);
            Ok(())
        })
    }

    /// Wrap the content of a range in the given wrapper nodes.
    pub fn wrap(&mut self, range: &BNodeRange, wrappers: Vec<Wrapper<Dyn>>) -> Result<(), String> {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| {
            let node_range = range
                .to_node_range()
                .ok_or_else(|| "invalid range".to_string())?;
            self.inner.wrap(&node_range, &wrappers);
            Ok(())
        })
    }

    /// Split the node at the given position.
    pub fn split(
        &mut self,
        pos: usize,
        depth: Option<usize>,
        types_after: Option<&[DynamicNodeType]>,
    ) -> Result<(), String> {
        let schema = Arc::clone(&self.schema);
        schema
            .with_types(|| self.inner.split(pos, depth, types_after))
            .map_err(|e| format!("{e:?}"))
            .map(|_| ())
    }

    /// Join nodes at the given position.
    pub fn join(&mut self, pos: usize, depth: Option<usize>) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.join(pos, depth));
    }

    /// Change the type and attributes of all text blocks in the given range.
    pub fn set_block_type(
        &mut self,
        from: usize,
        to: usize,
        type_: DynamicNodeType,
        attrs: Option<Value>,
    ) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.set_block_type(from, to, type_, attrs));
    }

    /// Change the markup of a node at the given position.
    pub fn set_node_markup(
        &mut self,
        pos: usize,
        type_: Option<DynamicNodeType>,
        attrs: Option<Value>,
        marks: Option<MarkSet<Dyn>>,
    ) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.set_node_markup(pos, type_, attrs, marks));
    }

    /// Set a single attribute on the node at the given position.
    pub fn set_node_attribute(&mut self, pos: usize, attr: String, value: Value) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.set_node_attribute(pos, &attr, value));
    }

    /// Set a single attribute on the document root.
    pub fn set_doc_attribute(&mut self, attr: String, value: Value) {
        let schema = Arc::clone(&self.schema);
        schema.with_types(|| self.inner.set_doc_attribute(&attr, value));
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Find the depth to which the given range can be lifted, if any.
pub fn b_lift_target(range: &BNodeRange) -> Option<usize> {
    range.schema.with_types(|| {
        let node_range = range.to_node_range()?;
        lift_target(&node_range)
    })
}

/// Find the wrapping node types needed to make `node_type` valid at the given range.
pub fn b_find_wrapping(
    range: &BNodeRange,
    node_type: DynamicNodeType,
    pred: impl Fn(DynamicNodeType) -> bool,
) -> Option<Vec<Wrapper<Dyn>>> {
    range.schema.with_types(|| {
        let node_range = range.to_node_range()?;
        find_wrapping(&node_range, node_type, |t: &DynamicNodeType| pred(*t))
    })
}

/// Check whether the document can be split at the given position.
pub fn b_can_split(
    doc: &BNode,
    pos: usize,
    depth: Option<usize>,
    types_after: Option<&[DynamicNodeType]>,
) -> bool {
    doc.schema
        .with_types(|| can_split::<Dyn>(&doc.inner, pos, depth, types_after))
}

/// Check whether the document can be joined at the given position.
pub fn b_can_join(doc: &BNode, pos: usize) -> bool {
    doc.schema
        .with_types(|| can_join::<Dyn>(&doc.inner, pos).unwrap_or(false))
}

/// Find a join point near the given position.
pub fn b_join_point(doc: &BNode, pos: usize, dir: Option<i32>) -> Option<usize> {
    doc.schema
        .with_types(|| join_point::<Dyn>(&doc.inner, pos, dir))
}

/// Find a valid insertion point for the given node type.
pub fn b_insert_point(doc: &BNode, pos: usize, node_type: DynamicNodeType) -> Option<usize> {
    doc.schema
        .with_types(|| insert_point::<Dyn>(&doc.inner, pos, node_type))
}

/// Find a valid drop point for the given slice.
pub fn b_drop_point(doc: &BNode, pos: usize, slice: &BSlice) -> Option<usize> {
    doc.schema
        .with_types(|| drop_point::<Dyn>(&doc.inner, pos, &slice.inner))
}
