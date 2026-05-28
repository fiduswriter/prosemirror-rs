use std::collections::HashMap;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;

use prosemirror::dynamic::types::{
    Dyn, DynamicMark, DynamicMarkType, DynamicNode, DynamicNodeType, ParsedContentMatch,
};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{
    ContentMatch, Fragment, Mark, MarkSet, Node, NodeType, ResolvedPos, Slice,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn extract_fragment(val: &Value, schema: &DynamicSchema) -> napi::Result<Fragment<Dyn>> {
    if val.is_null() {
        return Ok(Fragment::new());
    }
    if let Ok(node) = serde_json::from_value::<DynamicNode>(val.clone()) {
        return Ok(schema.with_types(|| Fragment::from(vec![node])));
    }
    if let Ok(nodes) = serde_json::from_value::<Vec<DynamicNode>>(val.clone()) {
        return Ok(schema.with_types(|| Fragment::from(nodes)));
    }
    Err(napi::Error::new(
        Status::InvalidArg,
        "Expected null, node, or array of nodes".to_string(),
    ))
}

pub fn extract_markset(val: &Value, _schema: &DynamicSchema) -> napi::Result<MarkSet<Dyn>> {
    if val.is_null() || val.as_array().map(|a| a.is_empty()).unwrap_or(false) {
        return Ok(MarkSet::new());
    }
    let marks: Vec<DynamicMark> = serde_json::from_value(val.clone())
        .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid marks JSON: {e}")))?;
    Ok(MarkSet::from_vec(marks))
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[napi]
pub struct Schema {
    pub(crate) inner: Arc<DynamicSchema>,
}

#[napi]
impl Schema {
    #[napi(constructor)]
    pub fn new(spec: Value) -> napi::Result<Self> {
        let schema = DynamicSchema::from_json(&spec)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid schema: {e}")))?;
        Ok(Schema {
            inner: Arc::new(schema),
        })
    }

    #[napi(getter)]
    pub fn nodes(&self) -> napi::Result<HashMap<String, NodeType_>> {
        let mut map = HashMap::new();
        self.inner.with_types(|| {
            for (name, idx) in &self.inner.node_type_map {
                map.insert(
                    name.clone(),
                    NodeType_ {
                        schema: self.inner.clone(),
                        inner: DynamicNodeType { idx: *idx },
                        name: name.clone(),
                    },
                );
            }
        });
        Ok(map)
    }

    #[napi(getter)]
    pub fn marks(&self) -> napi::Result<HashMap<String, MarkType_>> {
        let mut map = HashMap::new();
        self.inner.with_types(|| {
            for (name, idx) in &self.inner.mark_type_map {
                map.insert(
                    name.clone(),
                    MarkType_ {
                        schema: self.inner.clone(),
                        inner: DynamicMarkType { idx: *idx },
                        name: name.clone(),
                    },
                );
            }
        });
        Ok(map)
    }

    #[napi]
    pub fn node(
        &self,
        type_name: String,
        attrs: Option<Value>,
        content: Option<Either<&Fragment_, Vec<&Node_>>>,
        marks: Option<Vec<&Mark_>>,
    ) -> napi::Result<Node_> {
        let attrs = attrs.unwrap_or(Value::Null);
        let content = self.inner.with_types(|| match content {
            Some(Either::A(frag)) => frag.inner.clone(),
            Some(Either::B(nodes)) => Fragment::from(
                nodes
                    .into_iter()
                    .map(|n| n.inner.clone())
                    .collect::<Vec<_>>(),
            ),
            None => Fragment::new(),
        });
        let marks = marks
            .map(|m| MarkSet::from_vec(m.into_iter().map(|mark| mark.inner.clone()).collect()))
            .unwrap_or_else(MarkSet::new);
        let node = self
            .inner
            .node(&type_name, attrs, content, marks)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid node: {e}")))?;
        Ok(Node_ {
            schema: self.inner.clone(),
            inner: node,
        })
    }

    #[napi]
    pub fn text(&self, text: String, marks: Option<Vec<&Mark_>>) -> napi::Result<Node_> {
        let marks = match marks {
            Some(marks) => MarkSet::from_vec(marks.into_iter().map(|m| m.inner.clone()).collect()),
            None => MarkSet::new(),
        };
        let node = self
            .inner
            .with_types(|| DynamicNode::text(&text).mark(marks));
        Ok(Node_ {
            schema: self.inner.clone(),
            inner: node,
        })
    }

    #[napi]
    pub fn mark(&self, type_name: String, attrs: Option<Value>) -> napi::Result<Mark_> {
        let attrs = attrs.unwrap_or(Value::Null);
        let mark = DynamicMark { type_name, attrs };
        Ok(Mark_ {
            schema: self.inner.clone(),
            inner: mark,
        })
    }

    #[napi]
    pub fn node_from_json(&self, json: Value) -> napi::Result<Node_> {
        let node = self
            .inner
            .node_from_json(&json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid node JSON: {e}")))?;
        Ok(Node_ {
            schema: self.inner.clone(),
            inner: node,
        })
    }

    #[napi]
    pub fn mark_from_json(&self, json: Value) -> napi::Result<Mark_> {
        let mark = self
            .inner
            .mark_from_json(&json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid mark JSON: {e}")))?;
        Ok(Mark_ {
            schema: self.inner.clone(),
            inner: mark,
        })
    }
}

// ---------------------------------------------------------------------------
// NodeType
// ---------------------------------------------------------------------------

#[napi]
pub struct NodeType_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicNodeType,
    pub(crate) name: String,
}

#[napi]
impl NodeType_ {
    #[napi(getter)]
    pub fn schema(&self) -> Schema {
        Schema {
            inner: self.schema.clone(),
        }
    }

    #[napi(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[napi(getter)]
    pub fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }

    #[napi(getter)]
    pub fn is_inline(&self) -> bool {
        self.schema.with_types(|| self.inner.is_inline())
    }

    #[napi(getter)]
    pub fn is_textblock(&self) -> bool {
        self.schema.with_types(|| self.inner.is_textblock())
    }

    #[napi(getter)]
    pub fn is_atom(&self) -> bool {
        self.schema.with_types(|| self.inner.is_atom())
    }

    #[napi(getter)]
    pub fn is_leaf(&self) -> bool {
        self.schema.with_types(|| {
            // Leaf = atom or content expr with no edges from start state
            self.inner.is_atom() || self.inner.content_match().match_type(self.inner).is_none()
        })
    }

    #[napi(getter)]
    pub fn inline_content(&self) -> bool {
        self.schema.with_types(|| self.inner.inline_content())
    }

    #[napi]
    pub fn create(
        &self,
        attrs: Option<Value>,
        content: Option<Either<&Fragment_, Vec<&Node_>>>,
        marks: Option<Vec<&Mark_>>,
    ) -> napi::Result<Node_> {
        let attrs = attrs.unwrap_or(Value::Null);
        let content = self.schema.with_types(|| match content {
            Some(Either::A(frag)) => frag.inner.clone(),
            Some(Either::B(nodes)) => Fragment::from(
                nodes
                    .into_iter()
                    .map(|n| n.inner.clone())
                    .collect::<Vec<_>>(),
            ),
            None => Fragment::new(),
        });
        let marks = match marks {
            Some(marks) => MarkSet::from_vec(marks.into_iter().map(|m| m.inner.clone()).collect()),
            None => MarkSet::new(),
        };
        let node = self
            .schema
            .with_types(|| self.inner.create(attrs, Some(&content), Some(&marks)));
        Ok(Node_ {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    #[napi]
    pub fn create_checked(
        &self,
        attrs: Option<Value>,
        content: Option<Either<&Fragment_, Vec<&Node_>>>,
        marks: Option<Vec<&Mark_>>,
    ) -> napi::Result<Node_> {
        let node = self.create(attrs, content, marks)?;
        node.check()?;
        Ok(node)
    }

    #[napi]
    pub fn valid_content(&self, fragment: &Fragment_) -> bool {
        self.schema
            .with_types(|| self.inner.valid_content(&fragment.inner))
    }

    #[napi]
    pub fn allows_mark_type(&self, mark_type: &MarkType_) -> bool {
        self.schema
            .with_types(|| self.inner.allows_mark_type(mark_type.inner))
    }
}

// ---------------------------------------------------------------------------
// MarkType
// ---------------------------------------------------------------------------

#[napi]
pub struct MarkType_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicMarkType,
    pub(crate) name: String,
}

#[napi]
impl MarkType_ {
    #[napi(getter)]
    pub fn schema(&self) -> Schema {
        Schema {
            inner: self.schema.clone(),
        }
    }

    #[napi(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[napi]
    pub fn create(&self, attrs: Option<Value>) -> napi::Result<Mark_> {
        let attrs = attrs.unwrap_or(Value::Null);
        let mark = DynamicMark {
            type_name: self.name.clone(),
            attrs,
        };
        Ok(Mark_ {
            schema: self.schema.clone(),
            inner: mark,
        })
    }
}

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------

#[napi]
pub struct Mark_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicMark,
}

#[napi]
impl Mark_ {
    #[napi(getter)]
    pub fn type_(&self) -> MarkType_ {
        let name = self.inner.type_name.clone();
        MarkType_ {
            schema: self.schema.clone(),
            inner: self.schema.with_types(|| self.inner.r#type()),
            name,
        }
    }

    #[napi(getter)]
    pub fn attrs(&self) -> Value {
        self.inner.attrs.clone()
    }

    #[napi]
    pub fn eq(&self, other: &Mark_) -> bool {
        self.inner == other.inner
    }

    #[napi]
    pub fn add_to_set(&self, set: Vec<&Mark_>) -> Vec<Mark_> {
        let mark_set = MarkSet::from_vec(set.iter().map(|m| m.inner.clone()).collect());
        let result = self.schema.with_types(|| {
            self.inner
                .add_to_set(std::borrow::Cow::Owned(mark_set))
                .into_owned()
        });
        result
            .iter()
            .map(|m| Mark_ {
                schema: self.schema.clone(),
                inner: m.clone(),
            })
            .collect()
    }

    #[napi]
    pub fn remove_from_set(&self, set: Vec<&Mark_>) -> Vec<Mark_> {
        let mark_set = MarkSet::from_vec(set.iter().map(|m| m.inner.clone()).collect());
        let result = self.schema.with_types(|| {
            self.inner
                .remove_from_set(std::borrow::Cow::Owned(mark_set))
                .into_owned()
        });
        result
            .iter()
            .map(|m| Mark_ {
                schema: self.schema.clone(),
                inner: m.clone(),
            })
            .collect()
    }

    #[napi]
    pub fn is_in_set(&self, set: Vec<&Mark_>) -> bool {
        let mark_set = MarkSet::from_vec(set.iter().map(|m| m.inner.clone()).collect());
        self.schema.with_types(|| self.inner.is_in_set(&mark_set))
    }
}

// ---------------------------------------------------------------------------
// Fragment
// ---------------------------------------------------------------------------

#[napi]
pub struct Fragment_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: Fragment<Dyn>,
}

#[napi]
impl Fragment_ {
    #[napi(constructor)]
    pub fn new() -> Self {
        Fragment_ {
            schema: Arc::new(DynamicSchema::default()),
            inner: Fragment::new(),
        }
    }

    #[napi(factory)]
    pub fn from_array(nodes: Vec<&Node_>) -> Fragment_ {
        let schema = nodes
            .first()
            .map(|n| n.schema.clone())
            .unwrap_or_else(|| Arc::new(DynamicSchema::default()));
        let frag = schema.with_types(|| {
            Fragment::from(
                nodes
                    .into_iter()
                    .map(|n| n.inner.clone())
                    .collect::<Vec<_>>(),
            )
        });
        Fragment_ {
            schema,
            inner: frag,
        }
    }

    #[napi]
    pub fn from_(nodes: Option<Vec<&Node_>>) -> Fragment_ {
        match nodes {
            None => Fragment_ {
                schema: Arc::new(DynamicSchema::default()),
                inner: Fragment::new(),
            },
            Some(nodes) => {
                let schema = nodes
                    .first()
                    .map(|n| n.schema.clone())
                    .unwrap_or_else(|| Arc::new(DynamicSchema::default()));
                let frag = schema.with_types(|| {
                    Fragment::from(
                        nodes
                            .into_iter()
                            .map(|n| n.inner.clone())
                            .collect::<Vec<_>>(),
                    )
                });
                Fragment_ {
                    schema,
                    inner: frag,
                }
            }
        }
    }

    #[napi(getter)]
    pub fn size(&self) -> u32 {
        self.inner.size() as u32
    }

    #[napi(getter)]
    pub fn child_count(&self) -> u32 {
        self.inner.child_count() as u32
    }

    #[napi]
    pub fn child(&self, index: u32) -> Node_ {
        let child = self
            .inner
            .children()
            .get(index as usize)
            .expect("Index out of bounds in Fragment.child");
        Node_ {
            schema: self.schema.clone(),
            inner: child.clone(),
        }
    }

    #[napi]
    pub fn cut(&self, from: u32, to: Option<u32>) -> Fragment_ {
        let to = to.map(|t| t as usize).unwrap_or(self.inner.size());
        let frag = self.schema.with_types(|| self.inner.cut(from as usize..to));
        Fragment_ {
            schema: self.schema.clone(),
            inner: frag,
        }
    }

    #[napi]
    pub fn append(&self, other: &Fragment_) -> Fragment_ {
        let frag = self
            .schema
            .with_types(|| self.inner.clone().append(other.inner.clone()));
        Fragment_ {
            schema: self.schema.clone(),
            inner: frag,
        }
    }

    #[napi]
    pub fn eq(&self, other: &Fragment_) -> bool {
        self.inner == other.inner
    }

    #[napi]
    pub fn find_diff_start(&self, other: &Fragment_) -> Option<u32> {
        self.schema.with_types(|| {
            self.inner
                .find_diff_start(&other.inner, 0)
                .map(|p| p as u32)
        })
    }

    #[napi]
    pub fn find_diff_end(&self, other: &Fragment_) -> Option<DiffEnd> {
        self.schema.with_types(|| {
            self.inner
                .find_diff_end(&other.inner, self.inner.size(), other.inner.size())
                .map(|(a, b)| DiffEnd {
                    a: a as u32,
                    b: b as u32,
                })
        })
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        serde_json::to_value(&self.inner).unwrap_or(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// DiffEnd
// ---------------------------------------------------------------------------

#[napi]
pub struct DiffEnd {
    pub a: u32,
    pub b: u32,
}

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

#[napi]
pub struct Slice_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: Slice<Dyn>,
}

#[napi]
impl Slice_ {
    #[napi(constructor)]
    pub fn new(content: &Fragment_, open_start: u32, open_end: u32) -> Self {
        Slice_ {
            schema: content.schema.clone(),
            inner: Slice::new(
                content.inner.clone(),
                open_start as usize,
                open_end as usize,
            ),
        }
    }

    #[napi(getter)]
    pub fn empty() -> Slice_ {
        Slice_ {
            schema: Arc::new(DynamicSchema::default()),
            inner: Slice::new(Fragment::new(), 0, 0),
        }
    }

    #[napi(getter)]
    pub fn content(&self) -> Fragment_ {
        Fragment_ {
            schema: self.schema.clone(),
            inner: self.inner.content.clone(),
        }
    }

    #[napi(getter)]
    pub fn open_start(&self) -> u32 {
        self.inner.open_start as u32
    }

    #[napi(getter)]
    pub fn open_end(&self) -> u32 {
        self.inner.open_end as u32
    }

    #[napi]
    pub fn eq(&self, other: &Slice_) -> bool {
        self.inner == other.inner
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        serde_json::to_value(&self.inner).unwrap_or(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[napi]
pub struct Node_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicNode,
}

#[napi]
impl Node_ {
    #[napi(factory)]
    pub fn from_json(schema: &Schema, json: Value) -> napi::Result<Node_> {
        schema.node_from_json(json)
    }

    #[napi(getter)]
    pub fn type_(&self) -> NodeType_ {
        NodeType_ {
            schema: self.schema.clone(),
            inner: self.schema.with_types(|| self.inner.r#type()),
            name: self.inner.type_name.clone(),
        }
    }

    #[napi(getter)]
    pub fn attrs(&self) -> Value {
        self.inner.attrs.clone()
    }

    #[napi(getter)]
    pub fn content(&self) -> Fragment_ {
        let frag = self
            .schema
            .with_types(|| self.inner.content().cloned().unwrap_or_default());
        Fragment_ {
            schema: self.schema.clone(),
            inner: frag,
        }
    }

    #[napi(getter)]
    pub fn marks(&self) -> Vec<Mark_> {
        self.schema.with_types(|| {
            self.inner
                .marks()
                .map(|m| {
                    m.iter()
                        .map(|mark| Mark_ {
                            schema: self.schema.clone(),
                            inner: mark.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    #[napi(getter)]
    pub fn text(&self) -> Option<String> {
        self.schema.with_types(|| {
            self.inner
                .text_node()
                .map(|tn| tn.text.as_str().to_string())
        })
    }

    #[napi(getter)]
    pub fn text_content(&self) -> String {
        self.schema.with_types(|| self.inner.text_content())
    }

    #[napi(getter)]
    pub fn node_size(&self) -> u32 {
        self.schema.with_types(|| self.inner.node_size()) as u32
    }

    #[napi(getter)]
    pub fn child_count(&self) -> u32 {
        self.inner.child_count() as u32
    }

    #[napi(getter)]
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    #[napi(getter)]
    pub fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }

    #[napi(getter)]
    pub fn is_inline(&self) -> bool {
        self.schema.with_types(|| self.inner.is_inline())
    }

    #[napi(getter)]
    pub fn is_leaf(&self) -> bool {
        self.schema.with_types(|| self.inner.is_leaf())
    }

    #[napi]
    pub fn child(&self, index: u32) -> Option<Node_> {
        match self.inner.child(index as usize) {
            Some(n) => Some(Node_ {
                schema: self.schema.clone(),
                inner: n.clone(),
            }),
            None => None,
        }
    }

    #[napi]
    pub fn first_child(&self) -> Option<Node_> {
        <DynamicNode as Node<Dyn>>::first_child(&self.inner).map(|n| Node_ {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    #[napi]
    pub fn last_child(&self) -> Option<Node_> {
        <DynamicNode as Node<Dyn>>::last_child(&self.inner).map(|n| Node_ {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    #[napi]
    pub fn slice(&self, from: u32, to: Option<u32>, include_parents: Option<bool>) -> Slice_ {
        let to = to.map(|t| t as usize).unwrap_or(self.inner.content_size());
        let include_parents = include_parents.unwrap_or(false);
        let slice = self.schema.with_types(|| {
            self.inner
                .slice(from as usize..to, include_parents)
                .unwrap_or_else(|_| Slice::new(Fragment::new(), 0, 0))
        });
        Slice_ {
            schema: self.schema.clone(),
            inner: slice,
        }
    }

    #[napi]
    pub fn cut(&self, from: u32, to: Option<u32>) -> Node_ {
        let to = to.map(|t| t as usize).unwrap_or(self.inner.content_size());
        let node = self
            .schema
            .with_types(|| self.inner.cut(from as usize..to).into_owned());
        Node_ {
            schema: self.schema.clone(),
            inner: node,
        }
    }

    #[napi]
    pub fn replace(&self, from: u32, to: u32, slice: &Slice_) -> napi::Result<Node_> {
        let node = self
            .schema
            .with_types(|| self.inner.replace(from as usize..to as usize, &slice.inner))
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Replace failed: {e}")))?;
        Ok(Node_ {
            schema: self.schema.clone(),
            inner: node,
        })
    }

    #[napi]
    pub fn resolve(&self, pos: u32) -> ResolvedPos_ {
        ResolvedPos_ {
            schema: self.schema.clone(),
            doc: self.inner.clone(),
            pos: pos as usize,
        }
    }

    #[napi]
    pub fn eq(&self, other: &Node_) -> bool {
        self.inner == other.inner
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        self.schema.with_types(|| self.inner.to_json(false))
    }

    #[napi]
    pub fn to_string(&self) -> String {
        self.schema.with_types(|| self.inner.to_debug_string())
    }

    #[napi]
    pub fn mark(&self, marks: Vec<&Mark_>) -> Node_ {
        let mark_set = MarkSet::from_vec(marks.iter().map(|m| m.inner.clone()).collect());
        let node = self.schema.with_types(|| self.inner.mark(mark_set));
        Node_ {
            schema: self.schema.clone(),
            inner: node,
        }
    }

    #[napi]
    pub fn copy(&self, content: &Fragment_) -> Node_ {
        let node = self
            .schema
            .with_types(|| self.inner.copy(|_| content.inner.clone()));
        Node_ {
            schema: self.schema.clone(),
            inner: node,
        }
    }

    #[napi]
    pub fn check(&self) -> napi::Result<()> {
        self.schema
            .with_types(|| self.inner.check())
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Check failed: {e}")))
    }

    #[napi]
    pub fn node_at(&self, pos: u32) -> Option<Node_> {
        self.schema.with_types(|| {
            <DynamicNode as Node<Dyn>>::node_at(&self.inner, pos as usize).map(|n| Node_ {
                schema: self.schema.clone(),
                inner: n.clone(),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// ResolvedPos
// ---------------------------------------------------------------------------

#[napi]
pub struct ResolvedPos_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) doc: DynamicNode,
    pub(crate) pos: usize,
}

impl ResolvedPos_ {
    fn with_resolved<R>(&self, f: impl FnOnce(&ResolvedPos<'_, Dyn>) -> R) -> Option<R> {
        self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .ok()
                .map(|r| f(&r))
        })
    }
}

#[napi]
impl ResolvedPos_ {
    #[napi(getter)]
    pub fn pos(&self) -> u32 {
        self.pos as u32
    }

    #[napi(getter)]
    pub fn depth(&self) -> u32 {
        self.with_resolved(|r| r.depth as u32).unwrap_or(0)
    }

    #[napi(getter)]
    pub fn parent(&self) -> Node_ {
        let node = self
            .with_resolved(|r| r.parent().clone())
            .unwrap_or_else(|| self.doc.clone());
        Node_ {
            schema: self.schema.clone(),
            inner: node,
        }
    }

    #[napi(getter)]
    pub fn parent_offset(&self) -> u32 {
        self.with_resolved(|r| r.parent_offset as u32).unwrap_or(0)
    }

    #[napi]
    pub fn node(&self, depth: Option<u32>) -> Node_ {
        let depth = depth
            .map(|d| d as usize)
            .unwrap_or_else(|| self.with_resolved(|r| r.depth).unwrap_or(0));
        let node = self
            .with_resolved(|r| r.node(depth).clone())
            .unwrap_or_else(|| self.doc.clone());
        Node_ {
            schema: self.schema.clone(),
            inner: node,
        }
    }

    #[napi]
    pub fn start(&self, depth: Option<u32>) -> u32 {
        let depth = depth
            .map(|d| d as usize)
            .unwrap_or_else(|| self.with_resolved(|r| r.depth).unwrap_or(0));
        self.with_resolved(|r| r.start(depth) as u32).unwrap_or(0)
    }

    #[napi]
    pub fn end(&self, depth: Option<u32>) -> u32 {
        let depth = depth
            .map(|d| d as usize)
            .unwrap_or_else(|| self.with_resolved(|r| r.depth).unwrap_or(0));
        self.with_resolved(|r| r.end(depth) as u32).unwrap_or(0)
    }

    #[napi]
    pub fn before(&self, depth: Option<u32>) -> u32 {
        let depth = depth
            .map(|d| d as usize)
            .unwrap_or_else(|| self.with_resolved(|r| r.depth).unwrap_or(0));
        self.with_resolved(|r| r.before(depth))
            .flatten()
            .map(|p| p as u32)
            .unwrap_or(0)
    }

    #[napi]
    pub fn after(&self, depth: Option<u32>) -> u32 {
        let depth = depth
            .map(|d| d as usize)
            .unwrap_or_else(|| self.with_resolved(|r| r.depth).unwrap_or(0));
        self.with_resolved(|r| r.after(depth))
            .flatten()
            .map(|p| p as u32)
            .unwrap_or(0)
    }

    #[napi(getter)]
    pub fn node_before(&self) -> Option<Node_> {
        self.with_resolved(|r| {
            r.node_before().map(|n| Node_ {
                schema: self.schema.clone(),
                inner: n.into_owned(),
            })
        })
        .flatten()
    }

    #[napi(getter)]
    pub fn node_after(&self) -> Option<Node_> {
        self.with_resolved(|r| {
            r.node_after().map(|n| Node_ {
                schema: self.schema.clone(),
                inner: n.into_owned(),
            })
        })
        .flatten()
    }

    #[napi]
    pub fn marks(&self) -> Vec<Mark_> {
        self.with_resolved(|r| {
            r.marks()
                .into_iter()
                .map(|m| Mark_ {
                    schema: self.schema.clone(),
                    inner: m,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    #[napi]
    pub fn pos_at_index(&self, index: u32, depth: Option<u32>) -> u32 {
        let depth = depth.map(|d| d as usize);
        self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .map(|r| r.pos_at_index(index as usize, depth) as u32)
                .unwrap_or(0)
        })
    }

    #[napi(js_name = "blockRange")]
    pub fn block_range(&self, other: Option<&ResolvedPos_>) -> Option<NodeRange_> {
        self.schema.with_types(|| {
            let rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).ok()?;
            let other_rp = other.and_then(|o| ResolvedPos::<Dyn>::resolve(&o.doc, o.pos).ok());
            let other_ref = other_rp.as_ref();
            let range = rp.block_range(other_ref, None)?;
            Some(NodeRange_ {
                schema: self.schema.clone(),
                doc: self.doc.clone(),
                from_pos: range.from.pos,
                to_pos: range.to.pos,
                depth: range.depth,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// NodeRange
// ---------------------------------------------------------------------------

#[napi]
pub struct NodeRange_ {
    schema: Arc<DynamicSchema>,
    doc: DynamicNode,
    from_pos: usize,
    to_pos: usize,
    depth: usize,
}

#[napi]
impl NodeRange_ {
    #[napi(getter)]
    pub fn from(&self) -> ResolvedPos_ {
        ResolvedPos_ {
            schema: self.schema.clone(),
            doc: self.doc.clone(),
            pos: self.from_pos,
        }
    }

    #[napi(getter)]
    pub fn to(&self) -> ResolvedPos_ {
        ResolvedPos_ {
            schema: self.schema.clone(),
            doc: self.doc.clone(),
            pos: self.to_pos,
        }
    }

    #[napi(getter)]
    pub fn depth(&self) -> u32 {
        self.depth as u32
    }

    #[napi(getter)]
    pub fn start(&self) -> u32 {
        self.schema
            .with_types(|| {
                let from_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.from_pos).ok()?;
                from_rp.before(self.depth + 1).map(|p| p as u32)
            })
            .unwrap_or(0)
    }

    #[napi(getter)]
    pub fn end(&self) -> u32 {
        self.schema
            .with_types(|| {
                let to_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.to_pos).ok()?;
                to_rp.after(self.depth + 1).map(|p| p as u32)
            })
            .unwrap_or(0)
    }

    #[napi(getter)]
    pub fn parent(&self) -> Node_ {
        self.schema
            .with_types(|| {
                let from_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.from_pos).ok()?;
                Some(from_rp.node(self.depth).clone())
            })
            .map(|node| Node_ {
                schema: self.schema.clone(),
                inner: node,
            })
            .unwrap_or_else(|| Node_ {
                schema: self.schema.clone(),
                inner: self.doc.clone(),
            })
    }
}

// ---------------------------------------------------------------------------
// ContentMatch
// ---------------------------------------------------------------------------

#[napi]
pub struct ContentMatch_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: ParsedContentMatch,
}

#[napi]
impl ContentMatch_ {
    #[napi]
    pub fn match_type(&self, type_: &NodeType_) -> Option<ContentMatch_> {
        self.schema.with_types(|| {
            self.inner.match_type(type_.inner).map(|m| ContentMatch_ {
                schema: self.schema.clone(),
                inner: m,
            })
        })
    }

    #[napi]
    pub fn match_fragment(&self, frag: &Fragment_) -> Option<ContentMatch_> {
        self.schema.with_types(|| {
            self.inner
                .match_fragment(&frag.inner)
                .map(|m| ContentMatch_ {
                    schema: self.schema.clone(),
                    inner: m,
                })
        })
    }

    #[napi]
    pub fn fill_before(
        &self,
        after: &Fragment_,
        to_end: Option<bool>,
        start_index: Option<u32>,
    ) -> Option<Fragment_> {
        let to_end = to_end.unwrap_or(false);
        let start_index = start_index.map(|s| s as usize).unwrap_or(0);
        self.schema.with_types(|| {
            self.inner
                .fill_before(&after.inner, to_end, start_index)
                .map(|f| Fragment_ {
                    schema: self.schema.clone(),
                    inner: f,
                })
        })
    }

    #[napi(getter)]
    pub fn valid_end(&self) -> bool {
        self.inner.valid_end()
    }
}

#[napi]
pub fn content_match_parse(expr: String, schema: &Schema) -> napi::Result<ContentMatch_> {
    let inner = ParsedContentMatch::parse(&expr, &schema.inner).map_err(|e| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("Content expression parse error: {e}"),
        )
    })?;
    Ok(ContentMatch_ {
        schema: schema.inner.clone(),
        inner,
    })
}
