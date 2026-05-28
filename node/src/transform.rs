use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;

use crate::model::{Fragment_, Mark_, Node_, ResolvedPos_, Slice_};
use prosemirror::dynamic::types::{Dyn, DynamicNodeType};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{Fragment, MarkSet, Node, NodeType, ResolvedPos, Slice};
use prosemirror::transform::{
    map::{MapResult, Mappable, Mapping, StepMap},
    structure::{
        can_join as rs_can_join, can_split as rs_can_split, drop_point as rs_drop_point,
        find_wrapping as rs_find_wrapping, insert_point as rs_insert_point,
        join_point as rs_join_point, lift_target as rs_lift_target, NodeRange,
    },
    AddMarkStep, AddNodeMarkStep, AttrStep, DocAttrStep, RemoveMarkStep, RemoveNodeMarkStep,
    ReplaceAroundStep, ReplaceStep, Step, Transform,
};

// ---------------------------------------------------------------------------
// StepMap
// ---------------------------------------------------------------------------

#[napi]
pub struct StepMap_ {
    inner: StepMap,
}

#[napi]
impl StepMap_ {
    #[napi(constructor)]
    pub fn new(ranges: Vec<u32>) -> Self {
        StepMap_ {
            inner: StepMap::new(ranges.into_iter().map(|r| r as usize).collect()),
        }
    }

    #[napi(getter)]
    pub fn ranges(&self) -> Vec<u32> {
        self.inner.ranges.iter().map(|r| *r as u32).collect()
    }

    #[napi]
    pub fn map(&self, pos: u32, assoc: Option<i32>) -> u32 {
        self.inner.map(pos as usize, assoc.unwrap_or(1)) as u32
    }

    #[napi]
    pub fn map_result(&self, pos: u32, assoc: Option<i32>) -> MapResult_ {
        MapResult_ {
            inner: self.inner.map_result(pos as usize, assoc.unwrap_or(1)),
        }
    }

    #[napi]
    pub fn recover(&self, value: u32) -> Option<u32> {
        self.inner.recover(value as usize).map(|v| v as u32)
    }

    #[napi]
    pub fn invert(&self) -> StepMap_ {
        StepMap_ {
            inner: self.inner.invert(),
        }
    }
}

// ---------------------------------------------------------------------------
// MapResult
// ---------------------------------------------------------------------------

#[napi]
pub struct MapResult_ {
    inner: MapResult,
}

#[napi]
impl MapResult_ {
    #[napi(getter)]
    pub fn pos(&self) -> u32 {
        self.inner.pos as u32
    }

    #[napi(getter)]
    pub fn deleted(&self) -> bool {
        self.inner.deleted()
    }

    #[napi(getter)]
    pub fn deleted_before(&self) -> bool {
        self.inner.deleted_before()
    }

    #[napi(getter)]
    pub fn deleted_after(&self) -> bool {
        self.inner.deleted_after()
    }
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

#[napi]
pub struct Mapping_ {
    inner: Mapping,
}

#[napi]
impl Mapping_ {
    #[napi(constructor)]
    pub fn new() -> Self {
        Mapping_ {
            inner: Mapping::new(),
        }
    }

    #[napi(getter)]
    pub fn maps(&self) -> Vec<StepMap_> {
        self.inner
            .maps
            .iter()
            .map(|m| StepMap_ { inner: m.clone() })
            .collect()
    }

    #[napi]
    pub fn map(&self, pos: u32, assoc: Option<i32>) -> u32 {
        self.inner.map(pos as usize, assoc.unwrap_or(1)) as u32
    }

    #[napi]
    pub fn map_result(&self, pos: u32, assoc: Option<i32>) -> MapResult_ {
        MapResult_ {
            inner: self.inner.map_result(pos as usize, assoc.unwrap_or(1)),
        }
    }

    #[napi]
    pub fn append_map(&mut self, map: &StepMap_, mirrors: Option<u32>) {
        self.inner
            .append_map(map.inner.clone(), mirrors.map(|m| m as usize));
    }

    #[napi]
    pub fn get_mirror(&self, n: u32) -> Option<u32> {
        self.inner.get_mirror(n as usize).map(|m| m as u32)
    }

    #[napi]
    pub fn set_mirror(&mut self, n: u32, m: u32) {
        self.inner.set_mirror(n as usize, m as usize);
    }

    #[napi]
    pub fn invert(&self) -> Mapping_ {
        Mapping_ {
            inner: self.inner.invert(),
        }
    }

    #[napi]
    pub fn slice(&self, from: u32, to: Option<u32>) -> Mapping_ {
        Mapping_ {
            inner: self.inner.slice(from as usize, to.map(|t| t as usize)),
        }
    }
}

// ---------------------------------------------------------------------------
// Step
// ---------------------------------------------------------------------------

#[napi]
pub struct Step_ {
    pub(crate) inner: Step<Dyn>,
}

#[napi]
impl Step_ {
    #[napi(factory)]
    pub fn from_json(schema: &crate::model::Schema, json: Value) -> napi::Result<Step_> {
        let step = schema
            .inner
            .with_types(|| serde_json::from_value::<Step<Dyn>>(json))
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid step JSON: {e}")))?;
        Ok(Step_ { inner: step })
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        serde_json::to_value(&self.inner).unwrap_or(Value::Null)
    }

    #[napi]
    pub fn apply(&self, doc: &Node_) -> napi::Result<Node_> {
        let result = doc.schema.with_types(|| self.inner.apply(&doc.inner));
        match result {
            Ok(node) => Ok(Node_ {
                schema: doc.schema.clone(),
                inner: node,
            }),
            Err(e) => Err(napi::Error::new(
                Status::InvalidArg,
                format!("Step apply failed: {e:?}"),
            )),
        }
    }

    #[napi]
    pub fn get_map(&self) -> StepMap_ {
        StepMap_ {
            inner: self.inner.get_map(),
        }
    }

    #[napi]
    pub fn invert(&self, doc: &Node_) -> Step_ {
        let step = doc.schema.with_types(|| self.inner.invert(&doc.inner));
        Step_ { inner: step }
    }

    #[napi]
    pub fn map(&self, mapping: &Mapping_) -> Option<Step_> {
        self.inner.map(&mapping.inner).map(|s| Step_ { inner: s })
    }

    #[napi]
    pub fn merge(&self, other: &Step_) -> Option<Step_> {
        self.inner.merge(&other.inner).map(|s| Step_ { inner: s })
    }

    #[napi(factory)]
    pub fn replace(from: u32, to: u32, slice: Option<&Slice_>, structure: Option<bool>) -> Step_ {
        let slice = slice
            .map(|s| s.inner.clone())
            .unwrap_or_else(|| Slice::new(Fragment::new(), 0, 0));
        Step_ {
            inner: Step::Replace(ReplaceStep {
                span: prosemirror::transform::Span {
                    from: from as usize,
                    to: to as usize,
                },
                slice,
                structure: structure.unwrap_or(false),
            }),
        }
    }

    #[napi(factory)]
    pub fn replace_around(
        from: u32,
        to: u32,
        gap_from: u32,
        gap_to: u32,
        slice: Option<&Slice_>,
        insert: u32,
        structure: Option<bool>,
    ) -> Step_ {
        let slice = slice
            .map(|s| s.inner.clone())
            .unwrap_or_else(|| Slice::new(Fragment::new(), 0, 0));
        Step_ {
            inner: Step::ReplaceAround(ReplaceAroundStep {
                span: prosemirror::transform::Span {
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

    #[napi(factory)]
    pub fn add_mark(from: u32, to: u32, mark: &Mark_) -> Step_ {
        Step_ {
            inner: Step::AddMark(AddMarkStep {
                span: prosemirror::transform::Span {
                    from: from as usize,
                    to: to as usize,
                },
                mark: mark.inner.clone(),
            }),
        }
    }

    #[napi(factory)]
    pub fn remove_mark(from: u32, to: u32, mark: &Mark_) -> Step_ {
        Step_ {
            inner: Step::RemoveMark(RemoveMarkStep {
                span: prosemirror::transform::Span {
                    from: from as usize,
                    to: to as usize,
                },
                mark: mark.inner.clone(),
            }),
        }
    }

    #[napi(factory)]
    pub fn add_node_mark(pos: u32, mark: &Mark_) -> Step_ {
        Step_ {
            inner: Step::AddNodeMark(AddNodeMarkStep {
                pos: pos as usize,
                mark: mark.inner.clone(),
            }),
        }
    }

    #[napi(factory)]
    pub fn remove_node_mark(pos: u32, mark: &Mark_) -> Step_ {
        Step_ {
            inner: Step::RemoveNodeMark(RemoveNodeMarkStep {
                pos: pos as usize,
                mark: mark.inner.clone(),
            }),
        }
    }

    #[napi(factory)]
    pub fn attr(pos: u32, attr: String, value: Value) -> Step_ {
        Step_ {
            inner: Step::Attr(AttrStep {
                pos: pos as usize,
                attr,
                value,
            }),
        }
    }

    #[napi(factory)]
    pub fn doc_attr(attr: String, value: Value) -> Step_ {
        Step_ {
            inner: Step::DocAttr(DocAttrStep { attr, value }),
        }
    }
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

#[napi]
pub struct Transform_ {
    schema: Arc<DynamicSchema>,
    inner: Transform<Dyn>,
}

#[napi]
impl Transform_ {
    #[napi(constructor)]
    pub fn new(doc: &Node_) -> Self {
        Transform_ {
            schema: doc.schema.clone(),
            inner: Transform::new(doc.inner.clone()),
        }
    }

    #[napi(getter)]
    pub fn doc(&self) -> Node_ {
        Node_ {
            schema: self.schema.clone(),
            inner: self.inner.before().clone(),
        }
    }

    #[napi(getter)]
    pub fn steps(&self) -> Vec<Step_> {
        self.inner
            .steps
            .iter()
            .map(|s| Step_ { inner: s.clone() })
            .collect()
    }

    #[napi(getter)]
    pub fn mapping(&self) -> Mapping_ {
        Mapping_ {
            inner: self.inner.mapping.clone(),
        }
    }

    #[napi(getter)]
    pub fn before(&self) -> Node_ {
        Node_ {
            schema: self.schema.clone(),
            inner: self.inner.before().clone(),
        }
    }

    #[napi(getter)]
    pub fn doc_changed(&self) -> bool {
        self.inner.doc_changed()
    }

    #[napi]
    pub fn step(&mut self, step: &Step_) -> napi::Result<()> {
        self.schema
            .with_types(|| self.inner.step(step.inner.clone()))
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Step failed: {e:?}")))?;
        Ok(())
    }

    #[napi]
    pub fn maybe_step(&mut self, step: &Step_) -> Option<String> {
        self.schema
            .with_types(|| self.inner.maybe_step(step.inner.clone()))
    }

    #[napi]
    pub fn replace(&mut self, from: u32, to: Option<u32>, slice: Option<&Slice_>) {
        let slice = slice.map(|s| s.inner.clone());
        self.schema.with_types(|| {
            self.inner
                .replace(from as usize, to.map(|t| t as usize), slice);
        });
    }

    #[napi]
    pub fn replace_with(&mut self, from: u32, to: u32, content: &Node_) {
        self.schema.with_types(|| {
            self.inner.replace_with(
                from as usize,
                to as usize,
                Fragment::from(vec![content.inner.clone()]),
            );
        });
    }

    #[napi]
    pub fn delete(&mut self, from: u32, to: u32) {
        self.schema.with_types(|| {
            self.inner.delete(from as usize, to as usize);
        });
    }

    #[napi]
    pub fn insert(&mut self, pos: u32, content: &Node_) {
        self.schema.with_types(|| {
            self.inner
                .insert(pos as usize, Fragment::from(vec![content.inner.clone()]));
        });
    }

    #[napi]
    pub fn add_mark(&mut self, from: u32, to: u32, mark: &Mark_) {
        self.schema.with_types(|| {
            self.inner
                .add_mark(from as usize, to as usize, mark.inner.clone());
        });
    }

    #[napi]
    pub fn remove_mark(&mut self, from: u32, to: u32, mark: Option<&Mark_>) {
        let mark = mark.map(|m| m.inner.clone());
        self.schema.with_types(|| {
            self.inner.remove_mark(from as usize, to as usize, mark);
        });
    }

    #[napi]
    pub fn add_node_mark(&mut self, pos: u32, mark: &Mark_) {
        self.schema.with_types(|| {
            self.inner.add_node_mark(pos as usize, mark.inner.clone());
        });
    }

    #[napi]
    pub fn remove_node_mark(&mut self, pos: u32, mark: &Mark_) {
        self.schema.with_types(|| {
            self.inner
                .remove_node_mark(pos as usize, mark.inner.clone());
        });
    }

    #[napi]
    pub fn replace_range(&mut self, from: u32, to: u32, slice: &Slice_) {
        self.schema.with_types(|| {
            self.inner
                .replace_range(from as usize, to as usize, slice.inner.clone());
        });
    }

    #[napi]
    pub fn replace_range_with(&mut self, from: u32, to: u32, node: &Node_) {
        self.schema.with_types(|| {
            self.inner
                .replace_range_with(from as usize, to as usize, node.inner.clone());
        });
    }

    #[napi]
    pub fn delete_range(&mut self, from: u32, to: u32) {
        self.schema.with_types(|| {
            self.inner.delete_range(from as usize, to as usize);
        });
    }

    #[napi]
    pub fn lift(
        &mut self,
        from: &ResolvedPos_,
        to: &ResolvedPos_,
        target: u32,
    ) -> napi::Result<()> {
        let range = self.schema.with_types(|| {
            NodeRange::resolve(&from.doc, from.pos, to.pos)
                .map_err(|e| napi::Error::new(Status::InvalidArg, format!("{e}")))
        })?;
        self.schema.with_types(|| {
            self.inner.lift(&range, target as usize);
        });
        Ok(())
    }

    #[napi]
    pub fn wrap(
        &mut self,
        from: &ResolvedPos_,
        to: &ResolvedPos_,
        wrappers: Vec<Value>,
    ) -> napi::Result<()> {
        let range = self.schema.with_types(|| {
            NodeRange::resolve(&from.doc, from.pos, to.pos)
                .map_err(|e| napi::Error::new(Status::InvalidArg, format!("{e}")))
        })?;
        let wrappers: Vec<prosemirror::transform::Wrapper<Dyn>> = wrappers
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
                        napi::Error::new(
                            Status::InvalidArg,
                            format!("Unknown node type: {type_name}"),
                        )
                    })?;
                Ok(prosemirror::transform::Wrapper { node_type, attrs })
            })
            .collect::<napi::Result<Vec<_>>>()?;
        self.schema.with_types(|| {
            self.inner.wrap(&range, &wrappers);
        });
        Ok(())
    }

    #[napi]
    pub fn split(
        &mut self,
        pos: u32,
        depth: Option<u32>,
        types: Option<Vec<Value>>,
    ) -> napi::Result<()> {
        let depth = depth.unwrap_or(1);
        let types: Option<Vec<DynamicNodeType>> = types
            .map(|types| {
                types
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
                                napi::Error::new(
                                    Status::InvalidArg,
                                    format!("Unknown node type: {type_name}"),
                                )
                            })?;
                        Ok(node_type)
                    })
                    .collect::<napi::Result<Vec<_>>>()
            })
            .transpose()?;
        self.schema.with_types(|| {
            self.inner.split(
                pos as usize,
                Some(depth as usize),
                types.as_ref().map(|v| v.as_slice()),
            );
        });
        Ok(())
    }

    #[napi]
    pub fn join(&mut self, pos: u32, depth: Option<u32>) {
        self.schema.with_types(|| {
            self.inner.join(pos as usize, depth.map(|d| d as usize));
        });
    }

    #[napi]
    pub fn set_block_type(
        &mut self,
        from: u32,
        to: Option<u32>,
        type_: &crate::model::NodeType_,
        attrs: Option<Value>,
    ) {
        let to = to.map(|t| t as usize).unwrap_or(from as usize);
        self.schema.with_types(|| {
            self.inner
                .set_block_type(from as usize, to, type_.inner, attrs);
        });
    }

    #[napi]
    pub fn set_node_markup(
        &mut self,
        pos: u32,
        type_: Option<&crate::model::NodeType_>,
        attrs: Option<Value>,
        marks: Option<Vec<&Mark_>>,
    ) {
        let marks =
            marks.map(|m| MarkSet::from_vec(m.iter().map(|mark| mark.inner.clone()).collect()));
        self.schema.with_types(|| {
            self.inner
                .set_node_markup(pos as usize, type_.map(|t| t.inner), attrs, marks);
        });
    }
}

// ---------------------------------------------------------------------------
// Structure helpers
// ---------------------------------------------------------------------------

#[napi]
pub fn lift_target(from: &ResolvedPos_, to: &ResolvedPos_) -> Option<u32> {
    let node_range: NodeRange<'_, Dyn> = from
        .schema
        .with_types(|| NodeRange::resolve(&from.doc, from.pos, to.pos).ok())?;
    from.schema
        .with_types(|| rs_lift_target(&node_range))
        .map(|t| t as u32)
}

#[napi]
pub fn find_wrapping(
    from: &ResolvedPos_,
    to: &ResolvedPos_,
    node_type: &crate::model::NodeType_,
    attrs: Option<Value>,
) -> Option<Vec<Value>> {
    let node_range: NodeRange<'_, Dyn> = from
        .schema
        .with_types(|| NodeRange::resolve(&from.doc, from.pos, to.pos).ok())?;
    from.schema.with_types(|| {
        rs_find_wrapping(&node_range, node_type.inner, |_nt| true).map(|wrappers| {
            wrappers
                .into_iter()
                .map(|w| {
                    serde_json::json!({
                        "type": from.schema.with_types(|| w.node_type.name().to_string()),
                        "attrs": w.attrs,
                    })
                })
                .collect()
        })
    })
}

#[napi]
pub fn can_split(doc: &Node_, pos: u32, depth: Option<u32>, types: Option<Vec<Value>>) -> bool {
    let types: Option<Vec<DynamicNodeType>> = types.map(|types| {
        types
            .into_iter()
            .map(|t| {
                let type_name = t
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let node_type = doc
                    .schema
                    .node_type_map
                    .get(&type_name)
                    .copied()
                    .map(|idx| DynamicNodeType { idx })
                    .unwrap_or(DynamicNodeType { idx: 0 });
                node_type
            })
            .collect()
    });
    doc.schema.with_types(|| {
        rs_can_split::<Dyn>(
            &doc.inner,
            pos as usize,
            depth.map(|d| d as usize),
            types.as_ref().map(|v| v.as_slice()),
        )
    })
}

#[napi]
pub fn can_join(doc: &Node_, pos: u32) -> bool {
    doc.schema
        .with_types(|| rs_can_join::<Dyn>(&doc.inner, pos as usize))
        .unwrap_or(false)
}

#[napi]
pub fn join_point(doc: &Node_, pos: u32, dir: Option<i32>) -> Option<u32> {
    doc.schema
        .with_types(|| rs_join_point::<Dyn>(&doc.inner, pos as usize, dir))
        .map(|p| p as u32)
}

#[napi]
pub fn insert_point(doc: &Node_, pos: u32, node_type: &crate::model::NodeType_) -> Option<u32> {
    doc.schema
        .with_types(|| rs_insert_point::<Dyn>(&doc.inner, pos as usize, node_type.inner))
        .map(|p| p as u32)
}

#[napi]
pub fn drop_point(doc: &Node_, pos: u32, slice: &Slice_) -> Option<u32> {
    doc.schema
        .with_types(|| rs_drop_point::<Dyn>(&doc.inner, pos as usize, &slice.inner))
        .map(|p| p as u32)
}
