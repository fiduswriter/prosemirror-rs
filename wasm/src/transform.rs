//! WASM bindings for prosemirror-transform types.
//!
//! Each struct wraps the underlying Rust transform type directly and
//! forwards every method via `#[wasm_bindgen]`.  Model types (nodes, marks,
//! slices, etc.) are re-used from `crate::model`.

use std::sync::Arc;

use js_sys::{Array, Function, Object, Reflect};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use prosemirror::binding::model::BNode;
use prosemirror::dynamic::{
    types::{Dyn, DynamicNodeType},
    DynamicSchema,
};
use prosemirror::model::{Fragment, MarkSet, NodeType as ModelNodeTypeTrait, Slice as ModelSlice};
use prosemirror::transform::{
    map::{MapResult, Mappable, Mapping, StepMap},
    structure::{
        can_join as rs_can_join, can_split as rs_can_split, drop_point as rs_drop_point,
        find_wrapping as rs_find_wrapping, insert_point as rs_insert_point,
        join_point as rs_join_point, lift_target as rs_lift_target,
    },
    AddMarkStep, AddNodeMarkStep, AttrStep, DocAttrStep, MarkOrType, RemoveMarkStep,
    RemoveNodeMarkStep, ReplaceAroundStep, ReplaceStep, Span, Step, Transform, Wrapper,
};

// ---------------------------------------------------------------------------
// StepMap
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct StepMap_ {
    inner: StepMap,
}

#[wasm_bindgen]
impl StepMap_ {
    /// Create a new StepMap from flat range data.
    /// Each group of three numbers represents (oldStart, oldEnd, newStart).
    #[wasm_bindgen(constructor)]
    pub fn new(ranges: Vec<u32>) -> StepMap_ {
        StepMap_ {
            inner: StepMap::new(ranges.into_iter().map(|r| r as usize).collect()),
        }
    }

    /// The raw ranges backing this map.
    #[wasm_bindgen(getter)]
    pub fn ranges(&self) -> Vec<u32> {
        self.inner.ranges.iter().map(|r| *r as u32).collect()
    }

    /// Map a position through this step map.
    #[wasm_bindgen]
    pub fn map(&self, pos: u32, bias: Option<i32>) -> u32 {
        self.inner.map(pos as usize, bias.unwrap_or(1)) as u32
    }

    /// Map a position, returning a full MapResult.
    #[wasm_bindgen(js_name = "mapResult")]
    pub fn map_result(&self, pos: u32, bias: Option<i32>) -> MapResult_ {
        MapResult_ {
            inner: self.inner.map_result(pos as usize, bias.unwrap_or(1)),
        }
    }

    /// Recover a position from a packed recovery value.
    #[wasm_bindgen]
    pub fn recover(&self, value: u32) -> Option<u32> {
        self.inner.recover(value as usize).map(|v| v as u32)
    }

    /// Return the inverse of this step map.
    #[wasm_bindgen]
    pub fn invert(&self) -> StepMap_ {
        StepMap_ {
            inner: self.inner.invert(),
        }
    }

    /// Call the given callback for each entry in the map.
    /// The callback receives (oldStart, oldEnd, newStart, newEnd).
    #[wasm_bindgen(js_name = "forEach")]
    pub fn for_each(&self, f: &Function) -> Result<(), JsValue> {
        let mut entries: Vec<(usize, usize, usize, usize)> = Vec::new();
        self.inner
            .for_each(|old_start, old_end, new_start, new_end| {
                entries.push((old_start, old_end, new_start, new_end));
            });

        let this = JsValue::NULL;
        for (old_start, old_end, new_start, new_end) in entries {
            f.call4(
                &this,
                &JsValue::from(old_start as u32),
                &JsValue::from(old_end as u32),
                &JsValue::from(new_start as u32),
                &JsValue::from(new_end as u32),
            )
            .map_err(|e| JsValue::from_str(&format!("forEach callback error: {:?}", e)))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MapResult
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct MapResult_ {
    inner: MapResult,
}

#[wasm_bindgen]
impl MapResult_ {
    /// The mapped position.
    #[wasm_bindgen(getter)]
    pub fn pos(&self) -> u32 {
        self.inner.pos as u32
    }

    /// Whether the position was deleted (from the side indicated by bias).
    #[wasm_bindgen(getter)]
    pub fn deleted(&self) -> bool {
        self.inner.deleted()
    }

    /// Whether the position was deleted before.
    #[wasm_bindgen(getter, js_name = "deletedBefore")]
    pub fn deleted_before(&self) -> bool {
        self.inner.deleted_before()
    }

    /// Whether the position was deleted after.
    #[wasm_bindgen(getter, js_name = "deletedAfter")]
    pub fn deleted_after(&self) -> bool {
        self.inner.deleted_after()
    }

    /// Whether the position was deleted across.
    #[wasm_bindgen(getter, js_name = "deletedAcross")]
    pub fn deleted_across(&self) -> bool {
        self.inner.deleted_across()
    }
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct Mapping_ {
    inner: Mapping,
}

#[wasm_bindgen]
impl Mapping_ {
    /// Create a new mapping, optionally initialised with a set of step maps.
    #[wasm_bindgen(constructor)]
    pub fn new(maps: Option<Vec<StepMap_>>) -> Mapping_ {
        let mut mapping = Mapping::new();
        if let Some(maps) = maps {
            for m in maps {
                mapping.append_map(m.inner, None);
            }
        }
        Mapping_ { inner: mapping }
    }

    /// The individual step maps that make up this mapping.
    #[wasm_bindgen(getter)]
    pub fn maps(&self) -> Vec<StepMap_> {
        self.inner
            .maps
            .iter()
            .map(|m| StepMap_ { inner: m.clone() })
            .collect()
    }

    /// Map a position through this mapping.
    #[wasm_bindgen]
    pub fn map(&self, pos: u32, bias: Option<i32>) -> u32 {
        self.inner.map(pos as usize, bias.unwrap_or(1)) as u32
    }

    /// Map a position, returning a full MapResult.
    #[wasm_bindgen(js_name = "mapResult")]
    pub fn map_result(&self, pos: u32, bias: Option<i32>) -> MapResult_ {
        MapResult_ {
            inner: self.inner.map_result(pos as usize, bias.unwrap_or(1)),
        }
    }

    /// Append a step map, optionally recording a mirror pair.
    #[wasm_bindgen(js_name = "appendMap")]
    pub fn append_map(&mut self, map: &StepMap_, mirrors: Option<u32>) {
        self.inner
            .append_map(map.inner.clone(), mirrors.map(|m| m as usize));
    }

    /// Find the mirror of step `n`.
    #[wasm_bindgen(js_name = "getMirror")]
    pub fn get_mirror(&self, n: u32) -> Option<u32> {
        self.inner.get_mirror(n as usize).map(|m| m as u32)
    }

    /// Record a mirror pair.
    #[wasm_bindgen(js_name = "setMirror")]
    pub fn set_mirror(&mut self, n: u32, m: u32) {
        self.inner.set_mirror(n as usize, m as usize);
    }

    /// Return the inverse of this mapping.
    #[wasm_bindgen]
    pub fn invert(&self) -> Mapping_ {
        Mapping_ {
            inner: self.inner.invert(),
        }
    }

    /// Return a sub-mapping for the given range of maps.
    #[wasm_bindgen]
    pub fn slice(&self, from: u32, to: Option<u32>) -> Mapping_ {
        Mapping_ {
            inner: self.inner.slice(from as usize, to.map(|t| t as usize)),
        }
    }

    /// Return a deep copy of this mapping.
    #[wasm_bindgen]
    pub fn copy(&self) -> Mapping_ {
        Mapping_ {
            inner: self.inner.clone(),
        }
    }

    /// Append another mapping's maps to this one.
    #[wasm_bindgen(js_name = "appendMapping")]
    pub fn append_mapping(&mut self, mapping: &Mapping_) {
        self.inner.append_mapping(&mapping.inner);
    }

    /// Append the inverse of another mapping's maps in reverse order.
    #[wasm_bindgen(js_name = "appendMappingInverted")]
    pub fn append_mapping_inverted(&mut self, mapping: &Mapping_) {
        self.inner.append_mapping_inverted(&mapping.inner);
    }
}

// ---------------------------------------------------------------------------
// Step
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct Step_ {
    inner: Step<Dyn>,
}

#[wasm_bindgen]
impl Step_ {
    /// Deserialize a step from its JSON representation.
    #[wasm_bindgen(js_name = "fromJSON")]
    pub fn from_json(schema: &crate::model::Schema, json_str: &str) -> Result<Step_, JsValue> {
        let val: Value = serde_json::from_str(json_str)
            .map_err(|e| JsValue::from_str(&format!("Invalid JSON: {}", e)))?;
        let step = schema
            .inner
            .with_types(|| serde_json::from_value::<Step<Dyn>>(val))
            .map_err(|e| JsValue::from_str(&format!("Invalid step JSON: {:?}", e)))?;
        Ok(Step_ { inner: step })
    }

    /// Serialize this step to its JSON representation.
    #[wasm_bindgen(js_name = "toJSON")]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.inner).unwrap_or_else(|_| "null".to_string())
    }

    /// Apply this step to a document.
    /// Returns a JS object `{ doc, failed }`. On success, `failed` is null.
    #[wasm_bindgen]
    pub fn apply(&self, doc: &crate::model::Node) -> Result<JsValue, JsValue> {
        let result = doc
            .inner
            .schema
            .with_types(|| self.inner.apply(&doc.inner.inner));

        let result_obj = Object::new();
        match result {
            Ok(node) => {
                let new_doc = crate::model::Node {
                    inner: BNode {
                        schema: doc.inner.schema.clone(),
                        inner: node,
                    },
                };
                Reflect::set(
                    &result_obj,
                    &JsValue::from_str("doc"),
                    &JsValue::from(new_doc),
                )
                .map_err(|e| JsValue::from_str(&format!("Reflect::set failed: {:?}", e)))?;
                Reflect::set(&result_obj, &JsValue::from_str("failed"), &JsValue::NULL)
                    .map_err(|e| JsValue::from_str(&format!("Reflect::set failed: {:?}", e)))?;
            }
            Err(e) => {
                Reflect::set(&result_obj, &JsValue::from_str("doc"), &JsValue::NULL)
                    .map_err(|e| JsValue::from_str(&format!("Reflect::set failed: {:?}", e)))?;
                Reflect::set(
                    &result_obj,
                    &JsValue::from_str("failed"),
                    &JsValue::from_str(&format!("{:?}", e)),
                )
                .map_err(|e| JsValue::from_str(&format!("Reflect::set failed: {:?}", e)))?;
            }
        }
        Ok(result_obj.into())
    }

    /// Get the StepMap that describes the position changes caused by this step.
    #[wasm_bindgen(js_name = "getMap")]
    pub fn get_map(&self) -> StepMap_ {
        StepMap_ {
            inner: self.inner.get_map(),
        }
    }

    /// Create an inverted version of this step.
    #[wasm_bindgen]
    pub fn invert(&self, doc: &crate::model::Node) -> Step_ {
        let step = doc
            .inner
            .schema
            .with_types(|| self.inner.invert(&doc.inner.inner));
        Step_ { inner: step }
    }

    /// Map this step through a mapping. Returns null if the step was
    /// invalidated.
    #[wasm_bindgen]
    pub fn map(&self, mapping: &Mapping_) -> Option<Step_> {
        self.inner.map(&mapping.inner).map(|s| Step_ { inner: s })
    }

    /// Try to merge this step with another step.
    #[wasm_bindgen]
    pub fn merge(&self, other: &Step_) -> Option<Step_> {
        self.inner.merge(&other.inner).map(|s| Step_ { inner: s })
    }

    // -----------------------------------------------------------------------
    // Static factory methods
    // -----------------------------------------------------------------------

    /// Create a ReplaceStep.
    #[wasm_bindgen(js_name = "replace")]
    pub fn static_replace(
        from: u32,
        to: u32,
        slice: Option<crate::model::Slice>,
        structure: Option<bool>,
    ) -> Step_ {
        let slice = slice
            .map(|s| s.inner.inner.clone())
            .unwrap_or_else(|| ModelSlice::new(Fragment::new(), 0, 0));
        Step_ {
            inner: Step::Replace(ReplaceStep {
                span: Span {
                    from: from as usize,
                    to: to as usize,
                },
                slice,
                structure: structure.unwrap_or(false),
            }),
        }
    }

    /// Create a ReplaceAroundStep.
    #[wasm_bindgen(js_name = "replaceAround")]
    pub fn replace_around(
        from: u32,
        to: u32,
        gap_from: u32,
        gap_to: u32,
        slice: Option<crate::model::Slice>,
        insert: u32,
        structure: Option<bool>,
    ) -> Step_ {
        let slice = slice
            .map(|s| s.inner.inner.clone())
            .unwrap_or_else(|| ModelSlice::new(Fragment::new(), 0, 0));
        Step_ {
            inner: Step::ReplaceAround(ReplaceAroundStep {
                span: Span {
                    from: from as usize,
                    to: to as usize,
                },
                gap_from: gap_from as usize,
                gap_to: gap_to as usize,
                slice,
                insert: insert as usize,
                structure: structure.unwrap_or(false),
            }),
        }
    }

    /// Create an AddMarkStep.
    #[wasm_bindgen(js_name = "addMark")]
    pub fn add_mark(from: u32, to: u32, mark: &crate::model::Mark) -> Step_ {
        Step_ {
            inner: Step::AddMark(AddMarkStep {
                span: Span {
                    from: from as usize,
                    to: to as usize,
                },
                mark: mark.inner.inner.clone(),
            }),
        }
    }

    /// Create a RemoveMarkStep.
    #[wasm_bindgen(js_name = "removeMark")]
    pub fn remove_mark(from: u32, to: u32, mark: &crate::model::Mark) -> Step_ {
        Step_ {
            inner: Step::RemoveMark(RemoveMarkStep {
                span: Span {
                    from: from as usize,
                    to: to as usize,
                },
                mark: mark.inner.inner.clone(),
            }),
        }
    }

    /// Create an AddNodeMarkStep.
    #[wasm_bindgen(js_name = "addNodeMark")]
    pub fn add_node_mark(pos: u32, mark: &crate::model::Mark) -> Step_ {
        Step_ {
            inner: Step::AddNodeMark(AddNodeMarkStep {
                pos: pos as usize,
                mark: mark.inner.inner.clone(),
            }),
        }
    }

    /// Create a RemoveNodeMarkStep.
    #[wasm_bindgen(js_name = "removeNodeMark")]
    pub fn remove_node_mark(pos: u32, mark: &crate::model::Mark) -> Step_ {
        Step_ {
            inner: Step::RemoveNodeMark(RemoveNodeMarkStep {
                pos: pos as usize,
                mark: mark.inner.inner.clone(),
            }),
        }
    }

    /// Create an AttrStep.
    #[wasm_bindgen(js_name = "attr")]
    pub fn attr(pos: u32, attr: String, value: JsValue) -> Result<Step_, JsValue> {
        let value: Value = serde_wasm_bindgen::from_value(value)
            .map_err(|e| JsValue::from_str(&format!("Invalid attr value: {}", e)))?;
        Ok(Step_ {
            inner: Step::Attr(AttrStep {
                pos: pos as usize,
                attr,
                value,
            }),
        })
    }

    /// Create a DocAttrStep.
    #[wasm_bindgen(js_name = "docAttr")]
    pub fn doc_attr(attr: String, value: JsValue) -> Result<Step_, JsValue> {
        let value: Value = serde_wasm_bindgen::from_value(value)
            .map_err(|e| JsValue::from_str(&format!("Invalid attr value: {}", e)))?;
        Ok(Step_ {
            inner: Step::DocAttr(DocAttrStep { attr, value }),
        })
    }
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct Transform_ {
    schema: Arc<DynamicSchema>,
    inner: Transform<Dyn>,
}

#[wasm_bindgen]
impl Transform_ {
    /// Create a new transform starting from the given document.
    #[wasm_bindgen(constructor)]
    pub fn new(doc: &crate::model::Node) -> Transform_ {
        Transform_ {
            schema: doc.inner.schema.clone(),
            inner: Transform::new(doc.inner.inner.clone()),
        }
    }

    /// The current document.
    #[wasm_bindgen(getter)]
    pub fn doc(&self) -> crate::model::Node {
        crate::model::Node {
            inner: BNode {
                schema: self.schema.clone(),
                inner: self.inner.doc.clone(),
            },
        }
    }

    /// The steps applied so far.
    #[wasm_bindgen(getter)]
    pub fn steps(&self) -> Vec<Step_> {
        self.inner
            .steps
            .iter()
            .map(|s| Step_ { inner: s.clone() })
            .collect()
    }

    /// The intermediate documents after each step.
    #[wasm_bindgen(getter)]
    pub fn docs(&self) -> Vec<crate::model::Node> {
        self.inner
            .docs
            .iter()
            .map(|d| crate::model::Node {
                inner: BNode {
                    schema: self.schema.clone(),
                    inner: d.clone(),
                },
            })
            .collect()
    }

    /// The accumulated mapping from applied steps.
    #[wasm_bindgen(getter)]
    pub fn mapping(&self) -> Mapping_ {
        Mapping_ {
            inner: self.inner.mapping.clone(),
        }
    }

    /// The document before any steps were applied.
    #[wasm_bindgen(getter)]
    pub fn before(&self) -> crate::model::Node {
        crate::model::Node {
            inner: BNode {
                schema: self.schema.clone(),
                inner: self.inner.before().clone(),
            },
        }
    }

    /// Whether any steps have been applied to change the document.
    #[wasm_bindgen(getter, js_name = "docChanged")]
    pub fn doc_changed(&self) -> bool {
        self.inner.doc_changed()
    }

    /// Remove content at the given position that is incompatible with the
    /// given node type.
    #[wasm_bindgen(js_name = "clearIncompatible")]
    pub fn clear_incompatible(
        &mut self,
        pos: u32,
        node_type: &crate::model::NodeType,
        clear_newlines: bool,
    ) {
        let nt = node_type.inner.inner;
        let schema = self.schema.clone();
        schema.with_types(|| {
            self.inner
                .clear_incompatible(pos as usize, nt, None, clear_newlines);
        });
    }

    /// Apply a step.  Throws if the step is invalid.
    #[wasm_bindgen]
    pub fn step(&mut self, step: &Step_) -> Result<(), JsValue> {
        self.schema
            .with_types(|| self.inner.step(step.inner.clone()))
            .map_err(|e| JsValue::from_str(&format!("Step failed: {:?}", e)))?;
        Ok(())
    }

    /// Apply a step, returning an error message on failure or null on success.
    #[wasm_bindgen(js_name = "maybeStep")]
    pub fn maybe_step(&mut self, step: &Step_) -> Option<String> {
        self.schema
            .with_types(|| self.inner.maybe_step(step.inner.clone()))
    }

    /// Replace the content between `from` and `to` (if given) with the given
    /// slice (if given).
    #[wasm_bindgen]
    pub fn replace(&mut self, from: u32, to: Option<u32>, slice: Option<crate::model::Slice>) {
        let slice = slice.map(|s| s.inner.inner.clone());
        self.schema.with_types(|| {
            self.inner
                .replace(from as usize, to.map(|t| t as usize), slice);
        });
    }

    /// Replace a range with the content of the given node.
    #[wasm_bindgen(js_name = "replaceWith")]
    pub fn replace_with(&mut self, from: u32, to: u32, content: &crate::model::Node) {
        self.schema.with_types(|| {
            self.inner.replace_with(
                from as usize,
                to as usize,
                Fragment::from(vec![content.inner.inner.clone()]),
            );
        });
    }

    /// Delete content between two positions.
    #[wasm_bindgen]
    pub fn delete(&mut self, from: u32, to: u32) {
        self.schema.with_types(|| {
            self.inner.delete(from as usize, to as usize);
        });
    }

    /// Insert content at a position.
    #[wasm_bindgen]
    pub fn insert(&mut self, pos: u32, content: &crate::model::Node) {
        self.schema.with_types(|| {
            self.inner.insert(
                pos as usize,
                Fragment::from(vec![content.inner.inner.clone()]),
            );
        });
    }

    /// Add a mark to inline content in the given range.
    #[wasm_bindgen(js_name = "addMark")]
    pub fn add_mark(&mut self, from: u32, to: u32, mark: &crate::model::Mark) {
        self.schema.with_types(|| {
            self.inner
                .add_mark(from as usize, to as usize, mark.inner.inner.clone());
        });
    }

    /// Remove mark(s) from inline content in the given range.
    #[wasm_bindgen(js_name = "removeMark")]
    pub fn remove_mark(&mut self, from: u32, to: u32, mark: Option<crate::model::Mark>) {
        let mark = mark.map(|m| MarkOrType::Mark(m.inner.inner.clone()));
        self.schema.with_types(|| {
            self.inner.remove_mark(from as usize, to as usize, mark);
        });
    }

    /// Remove all marks of the given type from inline content in the given range.
    #[wasm_bindgen(js_name = "removeMarkType")]
    pub fn remove_mark_type(&mut self, from: u32, to: u32, mark_type: &crate::model::MarkType) {
        self.schema.with_types(|| {
            self.inner.remove_mark(
                from as usize,
                to as usize,
                Some(MarkOrType::MarkType(mark_type.inner.inner)),
            );
        });
    }

    /// Add a mark to a specific node.
    #[wasm_bindgen(js_name = "addNodeMark")]
    pub fn add_node_mark(&mut self, pos: u32, mark: &crate::model::Mark) {
        self.schema.with_types(|| {
            self.inner
                .add_node_mark(pos as usize, mark.inner.inner.clone());
        });
    }

    /// Remove a mark from a specific node.
    #[wasm_bindgen(js_name = "removeNodeMark")]
    pub fn remove_node_mark(&mut self, pos: u32, mark: &crate::model::Mark) {
        self.schema.with_types(|| {
            self.inner
                .remove_node_mark(pos as usize, MarkOrType::Mark(mark.inner.inner.clone()));
        });
    }

    /// Remove all marks of the given type from a specific node.
    #[wasm_bindgen(js_name = "removeNodeMarkType")]
    pub fn remove_node_mark_type(&mut self, pos: u32, mark_type: &crate::model::MarkType) {
        self.schema.with_types(|| {
            self.inner
                .remove_node_mark(pos as usize, MarkOrType::MarkType(mark_type.inner.inner));
        });
    }

    /// Replace a range with a slice, fitting the start and end.
    #[wasm_bindgen(js_name = "replaceRange")]
    pub fn replace_range(&mut self, from: u32, to: u32, slice: &crate::model::Slice) {
        self.schema.with_types(|| {
            self.inner
                .replace_range(from as usize, to as usize, slice.inner.inner.clone());
        });
    }

    /// Replace a range with a single node, fitting the start and end.
    #[wasm_bindgen(js_name = "replaceRangeWith")]
    pub fn replace_range_with(&mut self, from: u32, to: u32, node: &crate::model::Node) {
        self.schema.with_types(|| {
            self.inner
                .replace_range_with(from as usize, to as usize, node.inner.inner.clone());
        });
    }

    /// Delete a range, adjusting to avoid leaving invalid structure.
    #[wasm_bindgen(js_name = "deleteRange")]
    pub fn delete_range(&mut self, from: u32, to: u32) {
        self.schema.with_types(|| {
            self.inner.delete_range(from as usize, to as usize);
        });
    }

    /// Lift content out of its parent node to the given target depth.
    #[wasm_bindgen]
    pub fn lift(&mut self, range: &crate::model::NodeRange, target: u32) -> Result<(), JsValue> {
        let node_range = range
            .inner
            .to_node_range()
            .ok_or_else(|| JsValue::from_str("Invalid range"))?;
        self.schema.with_types(|| {
            self.inner.lift(&node_range, target as usize);
        });
        Ok(())
    }

    /// Wrap the content of a range in the given wrapper node specs.
    #[wasm_bindgen]
    pub fn wrap(
        &mut self,
        range: &crate::model::NodeRange,
        wrappers: JsValue,
    ) -> Result<(), JsValue> {
        let node_range = range
            .inner
            .to_node_range()
            .ok_or_else(|| JsValue::from_str("Invalid range"))?;
        let raw_wrappers: Vec<Value> = serde_wasm_bindgen::from_value(wrappers)
            .map_err(|e| JsValue::from_str(&format!("Invalid wrappers: {}", e)))?;
        let wrappers: Vec<Wrapper<Dyn>> = raw_wrappers
            .into_iter()
            .map(|w| {
                let type_name = w
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let attrs = w.get("attrs").cloned().unwrap_or(Value::Null);
                let node_type = self
                    .schema
                    .node_type_map
                    .get(&type_name)
                    .copied()
                    .map(|idx| DynamicNodeType { idx })
                    .ok_or_else(|| {
                        JsValue::from_str(&format!("Unknown node type: {}", type_name))
                    })?;
                Ok(Wrapper { node_type, attrs })
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        self.schema.with_types(|| {
            self.inner.wrap(&node_range, &wrappers);
        });
        Ok(())
    }

    /// Split the node at the given position.
    #[wasm_bindgen]
    pub fn split(
        &mut self,
        pos: u32,
        depth: Option<u32>,
        types_after: JsValue,
    ) -> Result<(), JsValue> {
        let depth = depth.unwrap_or(1);
        let types: Option<Vec<DynamicNodeType>> =
            if types_after.is_null() || types_after.is_undefined() {
                None
            } else {
                let raw_types: Vec<Value> = serde_wasm_bindgen::from_value(types_after)
                    .map_err(|e| JsValue::from_str(&format!("Invalid types_after: {}", e)))?;
                let types: Vec<DynamicNodeType> = raw_types
                    .into_iter()
                    .map(|t| {
                        let type_name = t
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        let node_type = self
                            .schema
                            .node_type_map
                            .get(&type_name)
                            .copied()
                            .map(|idx| DynamicNodeType { idx })
                            .ok_or_else(|| {
                                JsValue::from_str(&format!("Unknown node type: {}", type_name))
                            })?;
                        Ok(node_type)
                    })
                    .collect::<Result<Vec<_>, JsValue>>()?;
                Some(types)
            };
        self.schema
            .with_types(|| {
                self.inner.split(
                    pos as usize,
                    Some(depth as usize),
                    types.as_ref().map(|v| v.as_slice()),
                )
            })
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;
        Ok(())
    }

    /// Join nodes at the given position.
    #[wasm_bindgen]
    pub fn join(&mut self, pos: u32, depth: Option<u32>) {
        self.schema.with_types(|| {
            self.inner.join(pos as usize, depth.map(|d| d as usize));
        });
    }

    /// Change the type and attributes of all text blocks in the given range.
    #[wasm_bindgen(js_name = "setBlockType")]
    pub fn set_block_type(
        &mut self,
        from: u32,
        to: Option<u32>,
        type_: &crate::model::NodeType,
        attrs: JsValue,
    ) -> Result<(), JsValue> {
        let to = to.map(|t| t as usize).unwrap_or(from as usize);
        let attrs: Option<Value> = if attrs.is_null() || attrs.is_undefined() {
            None
        } else {
            Some(
                serde_wasm_bindgen::from_value(attrs)
                    .map_err(|e| JsValue::from_str(&format!("Invalid attrs: {}", e)))?,
            )
        };
        self.schema.with_types(|| {
            self.inner
                .set_block_type(from as usize, to, type_.inner.inner, attrs);
        });
        Ok(())
    }

    /// Change the type, attributes, and/or marks of a node at the given
    /// position.
    #[wasm_bindgen(js_name = "setNodeMarkup")]
    pub fn set_node_markup(
        &mut self,
        pos: u32,
        type_: Option<crate::model::NodeType>,
        attrs: JsValue,
        marks: Option<Vec<crate::model::Mark>>,
    ) -> Result<(), JsValue> {
        let attrs: Option<Value> = if attrs.is_null() || attrs.is_undefined() {
            None
        } else {
            Some(
                serde_wasm_bindgen::from_value(attrs)
                    .map_err(|e| JsValue::from_str(&format!("Invalid attrs: {}", e)))?,
            )
        };
        let type_opt: Option<DynamicNodeType> = type_.map(|t| t.inner.inner);
        let marks = marks
            .map(|m| MarkSet::from_vec(m.iter().map(|mark| mark.inner.inner.clone()).collect()));
        self.schema.with_types(|| {
            self.inner
                .set_node_markup(pos as usize, type_opt, attrs, marks);
        });
        Ok(())
    }

    /// Set an attribute on the node at the given position.
    #[wasm_bindgen(js_name = "setNodeAttribute")]
    pub fn set_node_attribute(
        &mut self,
        pos: u32,
        attr: String,
        value: JsValue,
    ) -> Result<(), JsValue> {
        let value: Value = serde_wasm_bindgen::from_value(value)
            .map_err(|e| JsValue::from_str(&format!("Invalid value: {}", e)))?;
        let step = Step::Attr(AttrStep {
            pos: pos as usize,
            attr,
            value,
        });
        let _ = self.schema.with_types(|| self.inner.step(step));
        Ok(())
    }

    /// Set an attribute on the document root.
    #[wasm_bindgen(js_name = "setDocAttribute")]
    pub fn set_doc_attribute(&mut self, attr: String, value: JsValue) -> Result<(), JsValue> {
        let value: Value = serde_wasm_bindgen::from_value(value)
            .map_err(|e| JsValue::from_str(&format!("Invalid value: {}", e)))?;
        let step = Step::DocAttr(DocAttrStep { attr, value });
        let _ = self.schema.with_types(|| self.inner.step(step));
        Ok(())
    }

    /// Return the total changed range as `{from, to}`, or null if nothing
    /// changed.
    #[wasm_bindgen(js_name = "changedRange")]
    pub fn changed_range(&self) -> Option<JsValue> {
        let mut from = usize::MAX;
        let mut to = 0usize;
        for (i, map) in self.inner.mapping.maps.iter().enumerate() {
            if i > 0 {
                from = map.map(from, 1);
                to = map.map(to, -1);
            }
            map.for_each(|_f, _t, from_b, to_b| {
                from = from.min(from_b);
                to = to.max(to_b);
            });
        }
        if from == usize::MAX {
            None
        } else {
            let obj = Object::new();
            let _ = Reflect::set(
                &obj,
                &JsValue::from_str("from"),
                &JsValue::from(from as u32),
            );
            let _ = Reflect::set(&obj, &JsValue::from_str("to"), &JsValue::from(to as u32));
            Some(obj.into())
        }
    }
}

// ---------------------------------------------------------------------------
// Free functions — structure helpers
// ---------------------------------------------------------------------------

/// Find the depth to which the given range can be lifted, if any.
#[wasm_bindgen(js_name = "liftTarget")]
pub fn lift_target(range: &crate::model::NodeRange) -> Option<u32> {
    let node_range = range.inner.to_node_range()?;
    range
        .inner
        .schema
        .with_types(|| rs_lift_target(&node_range))
        .map(|t| t as u32)
}

/// Find the wrapping node types needed to make `node_type` valid at the
/// given range.
#[wasm_bindgen(js_name = "findWrapping")]
pub fn find_wrapping(
    range: &crate::model::NodeRange,
    node_type: &crate::model::NodeType,
    _attrs: Option<JsValue>,
) -> Result<Option<JsValue>, JsValue> {
    let node_range = match range.inner.to_node_range() {
        Some(r) => r,
        None => return Ok(None),
    };
    let schema = range.inner.schema.clone();
    let result = schema.with_types(|| {
        rs_find_wrapping(&node_range, node_type.inner.inner, |_nt| true).map(|wrappers| {
            let arr = Array::new();
            for w in wrappers {
                let obj = Object::new();
                let type_name = schema.with_types(|| w.node_type.name().to_string());
                let _ = Reflect::set(
                    &obj,
                    &JsValue::from_str("type"),
                    &JsValue::from_str(&type_name),
                );
                let attrs_val: JsValue =
                    serde_wasm_bindgen::to_value(&w.attrs).unwrap_or(JsValue::NULL);
                let _ = Reflect::set(&obj, &JsValue::from_str("attrs"), &attrs_val);
                arr.push(&obj);
            }
            arr
        })
    });
    Ok(result.map(|arr| arr.into()))
}

/// Check whether the document can be split at the given position.
#[wasm_bindgen(js_name = "canSplit")]
pub fn can_split(
    doc: &crate::model::Node,
    pos: u32,
    depth: Option<u32>,
    types_after: JsValue,
) -> Result<bool, JsValue> {
    let types: Option<Vec<DynamicNodeType>> = if types_after.is_null() || types_after.is_undefined()
    {
        None
    } else {
        let raw_types: Vec<Value> = serde_wasm_bindgen::from_value(types_after)
            .map_err(|e| JsValue::from_str(&format!("Invalid types_after: {}", e)))?;
        let types: Vec<DynamicNodeType> = raw_types
            .into_iter()
            .map(|t| {
                let type_name = t
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let schema = &doc.inner.schema;
                let node_type = schema
                    .node_type_map
                    .get(&type_name)
                    .copied()
                    .map(|idx| DynamicNodeType { idx })
                    .unwrap_or(DynamicNodeType { idx: 0 });
                node_type
            })
            .collect();
        Some(types)
    };
    Ok(doc.inner.schema.with_types(|| {
        rs_can_split::<Dyn>(
            &doc.inner.inner,
            pos as usize,
            depth.map(|d| d as usize),
            types.as_ref().map(|v| v.as_slice()),
        )
    }))
}

/// Check whether the document can be joined at the given position.
#[wasm_bindgen(js_name = "canJoin")]
pub fn can_join(doc: &crate::model::Node, pos: u32) -> bool {
    doc.inner
        .schema
        .with_types(|| rs_can_join::<Dyn>(&doc.inner.inner, pos as usize))
        .unwrap_or(false)
}

/// Find a join point near the given position.
#[wasm_bindgen(js_name = "joinPoint")]
pub fn join_point(doc: &crate::model::Node, pos: u32, dir: Option<i32>) -> Option<u32> {
    doc.inner
        .schema
        .with_types(|| rs_join_point::<Dyn>(&doc.inner.inner, pos as usize, dir))
        .map(|p| p as u32)
}

/// Find a valid insertion point for the given node type near `pos`.
#[wasm_bindgen(js_name = "insertPoint")]
pub fn insert_point(
    doc: &crate::model::Node,
    pos: u32,
    node_type: &crate::model::NodeType,
) -> Option<u32> {
    doc.inner
        .schema
        .with_types(|| {
            rs_insert_point::<Dyn>(&doc.inner.inner, pos as usize, node_type.inner.inner)
        })
        .map(|p| p as u32)
}

/// Find a valid drop point for the given slice near `pos`.
#[wasm_bindgen(js_name = "dropPoint")]
pub fn drop_point(doc: &crate::model::Node, pos: u32, slice: &crate::model::Slice) -> Option<u32> {
    doc.inner
        .schema
        .with_types(|| rs_drop_point::<Dyn>(&doc.inner.inner, pos as usize, &slice.inner.inner))
        .map(|p| p as u32)
}
