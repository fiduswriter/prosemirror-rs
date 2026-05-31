use std::collections::HashMap;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;

use prosemirror::binding::model::{
    b_fragment_from, b_schema_top_node_type, BContentMatch, BFragment, BMark, BMarkType, BNode,
    BNodeRange, BNodeType, BResolvedPos, BSlice, FragmentFromInput,
};
use prosemirror::dynamic::types::{Dyn, DynamicMarkType, DynamicNode, DynamicNodeType};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{Fragment, MarkSet, Node};

// ---------------------------------------------------------------------------
// Helpers (used by transform.rs)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub fn extract_fragment(val: &Value, schema: &DynamicSchema) -> napi::Result<Fragment<Dyn>> {
    if val.is_null() {
        return Ok(Fragment::new());
    }
    if let Ok(node) =
        serde_json::from_value::<prosemirror::dynamic::types::DynamicNode>(val.clone())
    {
        return Ok(schema.with_types(|| Fragment::from(vec![node])));
    }
    if let Ok(nodes) =
        serde_json::from_value::<Vec<prosemirror::dynamic::types::DynamicNode>>(val.clone())
    {
        return Ok(schema.with_types(|| Fragment::from_array(nodes)));
    }
    Err(napi::Error::new(
        Status::InvalidArg,
        "Expected null, node, or array of nodes".to_string(),
    ))
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
                        inner: BNodeType::new(
                            self.inner.clone(),
                            DynamicNodeType { idx: *idx },
                            name.clone(),
                        ),
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
                        inner: BMarkType {
                            schema: self.inner.clone(),
                            inner: DynamicMarkType { idx: *idx },
                            name: name.clone(),
                        },
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
            Some(Either::A(frag)) => frag.inner.inner.clone(),
            Some(Either::B(nodes)) => Fragment::from_array(
                nodes
                    .into_iter()
                    .map(|n| n.inner.inner.clone())
                    .collect::<Vec<_>>(),
            ),
            None => Fragment::new(),
        });
        let marks = marks
            .map(|m| {
                MarkSet::from_vec(m.into_iter().map(|mark| mark.inner.inner.clone()).collect())
            })
            .unwrap_or_else(MarkSet::new);
        let node = self
            .inner
            .node(&type_name, attrs, content, marks)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid node: {e}")))?;
        Ok(Node_ {
            inner: BNode {
                schema: self.inner.clone(),
                inner: node,
            },
        })
    }

    #[napi]
    pub fn text(&self, text: String, marks: Option<Vec<&Mark_>>) -> napi::Result<Node_> {
        let marks = match marks {
            Some(marks) => {
                MarkSet::from_vec(marks.into_iter().map(|m| m.inner.inner.clone()).collect())
            }
            None => MarkSet::new(),
        };
        let node = self
            .inner
            .with_types(|| DynamicNode::text(&text).mark(marks));
        Ok(Node_ {
            inner: BNode {
                schema: self.inner.clone(),
                inner: node,
            },
        })
    }

    #[napi]
    pub fn mark(&self, type_name: String, attrs: Option<Value>) -> napi::Result<Mark_> {
        let attrs = attrs.unwrap_or(Value::Null);
        Ok(Mark_ {
            inner: BMark {
                schema: self.inner.clone(),
                inner: prosemirror::dynamic::types::DynamicMark { type_name, attrs },
            },
        })
    }

    #[napi]
    pub fn node_from_json(&self, json: Value) -> napi::Result<Node_> {
        let node = self
            .inner
            .node_from_json(&json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid node JSON: {e}")))?;
        Ok(Node_ {
            inner: BNode {
                schema: self.inner.clone(),
                inner: node,
            },
        })
    }

    #[napi]
    pub fn mark_from_json(&self, json: Value) -> napi::Result<Mark_> {
        let mark = self
            .inner
            .mark_from_json(&json)
            .map_err(|e| napi::Error::new(Status::InvalidArg, format!("Invalid mark JSON: {e}")))?;
        Ok(Mark_ {
            inner: BMark {
                schema: self.inner.clone(),
                inner: mark,
            },
        })
    }

    #[napi(getter, js_name = "topNodeType")]
    pub fn top_node_type(&self) -> napi::Result<NodeType_> {
        b_schema_top_node_type(&self.inner)
            .map(|inner| NodeType_ { inner })
            .ok_or_else(|| napi::Error::new(Status::InvalidArg, "Unknown top node type"))
    }
}

// ---------------------------------------------------------------------------
// NodeType
// ---------------------------------------------------------------------------

#[napi]
pub struct NodeType_ {
    pub(crate) inner: BNodeType,
}

#[napi]
impl NodeType_ {
    #[napi(getter)]
    pub fn schema(&self) -> Schema {
        Schema {
            inner: self.inner.schema.clone(),
        }
    }

    #[napi(getter)]
    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[napi(getter)]
    pub fn is_block(&self) -> bool {
        self.inner.is_block()
    }

    #[napi(getter)]
    pub fn is_inline(&self) -> bool {
        self.inner.is_inline()
    }

    #[napi(getter)]
    pub fn is_textblock(&self) -> bool {
        self.inner.is_textblock()
    }

    #[napi(getter)]
    pub fn is_atom(&self) -> bool {
        self.inner.is_atom()
    }

    #[napi(getter)]
    pub fn is_leaf(&self) -> bool {
        self.inner.is_leaf()
    }

    #[napi(getter)]
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    #[napi(getter)]
    pub fn inline_content(&self) -> bool {
        self.inner.inline_content()
    }

    #[napi(getter)]
    pub fn whitespace(&self) -> String {
        self.inner.whitespace()
    }

    #[napi(getter)]
    pub fn is_code(&self) -> bool {
        self.inner.is_code()
    }

    #[napi(getter)]
    pub fn content_match(&self) -> Option<ContentMatch_> {
        self.inner
            .content_match()
            .map(|cm| ContentMatch_ { inner: cm })
    }

    #[napi(getter)]
    pub fn has_required_attrs(&self) -> bool {
        self.inner.has_required_attrs()
    }

    #[napi]
    pub fn compatible_content(&self, other: &NodeType_) -> bool {
        self.inner.compatible_content(&other.inner)
    }

    #[napi]
    pub fn create(
        &self,
        attrs: Option<Value>,
        content: Option<Either<&Fragment_, Vec<&Node_>>>,
        marks: Option<Vec<&Mark_>>,
    ) -> napi::Result<Node_> {
        let attrs = attrs.unwrap_or(Value::Null);
        let content = self.inner.schema.with_types(|| match content {
            Some(Either::A(frag)) => frag.inner.inner.clone(),
            Some(Either::B(nodes)) => Fragment::from_array(
                nodes
                    .into_iter()
                    .map(|n| n.inner.inner.clone())
                    .collect::<Vec<_>>(),
            ),
            None => Fragment::new(),
        });
        let marks = match marks {
            Some(m) => MarkSet::from_vec(m.into_iter().map(|mk| mk.inner.inner.clone()).collect()),
            None => MarkSet::new(),
        };
        let node = self.inner.create(attrs, content, marks);
        Ok(Node_ { inner: node })
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
    pub fn create_and_fill(
        &self,
        attrs: Option<Value>,
        content: Option<Either<&Fragment_, Vec<&Node_>>>,
        marks: Option<Vec<&Mark_>>,
    ) -> napi::Result<Option<Node_>> {
        let attrs = attrs.unwrap_or(Value::Null);
        let content = self.inner.schema.with_types(|| match content {
            Some(Either::A(frag)) => Some(frag.inner.inner.clone()),
            Some(Either::B(nodes)) => Some(Fragment::from_array(
                nodes
                    .into_iter()
                    .map(|n| n.inner.inner.clone())
                    .collect::<Vec<_>>(),
            )),
            None => None,
        });
        let marks = match marks {
            Some(m) => MarkSet::from_vec(m.into_iter().map(|mk| mk.inner.inner.clone()).collect()),
            None => MarkSet::new(),
        };
        Ok(self
            .inner
            .create_and_fill(attrs, content, marks)
            .map(|n| Node_ { inner: n }))
    }

    #[napi]
    pub fn valid_content(&self, fragment: &Fragment_) -> bool {
        self.inner.valid_content(&fragment.inner.inner)
    }

    #[napi]
    pub fn allows_mark_type(&self, mark_type: &MarkType_) -> bool {
        self.inner.allows_mark_type(&mark_type.inner)
    }

    #[napi]
    pub fn allows_marks(&self, marks: Vec<&Mark_>) -> bool {
        let mark_set =
            MarkSet::from_vec(marks.into_iter().map(|m| m.inner.inner.clone()).collect());
        self.inner.allows_marks(&mark_set)
    }

    #[napi]
    pub fn is_in_group(&self, group: String) -> bool {
        self.inner.is_in_group(&group)
    }

    #[napi(getter)]
    pub fn attrs(&self) -> Value {
        self.inner.attrs_defaults()
    }

    #[napi(getter, js_name = "markSet")]
    pub fn mark_set(&self) -> Option<Vec<MarkType_>> {
        self.inner
            .mark_set()
            .map(|ms| ms.into_iter().map(|bmt| MarkType_ { inner: bmt }).collect())
    }

    #[napi]
    pub fn allowed_marks(&self, marks: Vec<&Mark_>) -> Vec<Mark_> {
        let raw: Vec<_> = marks.into_iter().map(|m| m.inner.inner.clone()).collect();
        let schema = self.inner.schema.clone();
        self.inner
            .allowed_marks_filtered(raw)
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: schema.clone(),
                    inner: m,
                },
            })
            .collect()
    }

    #[napi(getter)]
    pub fn spec(&self) -> Value {
        self.inner.spec_json()
    }
}

// ---------------------------------------------------------------------------
// MarkType
// ---------------------------------------------------------------------------

#[napi]
pub struct MarkType_ {
    pub(crate) inner: BMarkType,
}

#[napi]
impl MarkType_ {
    #[napi(getter)]
    pub fn schema(&self) -> Schema {
        Schema {
            inner: self.inner.schema.clone(),
        }
    }

    #[napi(getter)]
    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[napi]
    pub fn create(&self, attrs: Option<Value>) -> napi::Result<Mark_> {
        let attrs = attrs.unwrap_or(Value::Null);
        Ok(Mark_ {
            inner: self.inner.create(attrs),
        })
    }

    #[napi]
    pub fn remove_from_set(&self, set: Vec<&Mark_>) -> Vec<Mark_> {
        let marks: Vec<_> = set.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner
            .remove_from_set(marks)
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect()
    }

    #[napi]
    pub fn is_in_set(&self, set: Vec<&Mark_>) -> Option<Mark_> {
        let marks: Vec<_> = set.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner.is_in_set(&marks).map(|bm| Mark_ { inner: bm })
    }

    #[napi]
    pub fn excludes(&self, other: &MarkType_) -> bool {
        self.inner.excludes(&other.inner)
    }

    #[napi(getter)]
    pub fn spec(&self) -> Value {
        self.inner.spec_json()
    }
}

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------

#[napi]
pub struct Mark_ {
    pub(crate) inner: BMark,
}

#[napi]
impl Mark_ {
    #[napi(getter)]
    pub fn type_(&self) -> MarkType_ {
        MarkType_ {
            inner: self.inner.type_(),
        }
    }

    #[napi(getter)]
    pub fn attrs(&self) -> Value {
        self.inner.attrs_json()
    }

    #[napi]
    pub fn eq(&self, other: &Mark_) -> bool {
        self.inner.eq(&other.inner)
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        self.inner.to_json()
    }

    #[napi]
    pub fn add_to_set(&self, set: Vec<&Mark_>) -> Vec<Mark_> {
        let marks: Vec<_> = set.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner
            .add_to_set(marks)
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect()
    }

    #[napi]
    pub fn remove_from_set(&self, set: Vec<&Mark_>) -> Vec<Mark_> {
        let marks: Vec<_> = set.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner
            .remove_from_set(marks)
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect()
    }

    #[napi]
    pub fn is_in_set(&self, set: Vec<&Mark_>) -> bool {
        let marks: Vec<_> = set.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner.is_in_set(&marks)
    }

    #[napi(js_name = "sameSet")]
    pub fn same_set(a: Vec<&Mark_>, b: Vec<&Mark_>) -> bool {
        let av: Vec<_> = a.iter().map(|m| m.inner.inner.clone()).collect();
        let bv: Vec<_> = b.iter().map(|m| m.inner.inner.clone()).collect();
        BMark::same_set(&av, &bv)
    }

    #[napi(js_name = "setFrom")]
    pub fn set_from(schema: &Schema, marks: Option<Vec<&Mark_>>) -> Vec<Mark_> {
        let raw: Vec<_> = marks
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.inner.inner.clone())
            .collect();
        BMark::set_from(&schema.inner, raw)
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: schema.inner.clone(),
                    inner: m,
                },
            })
            .collect()
    }
}

#[napi]
pub struct Fragment_ {
    pub(crate) inner: BFragment,
}

#[napi]
impl Fragment_ {
    #[napi(constructor)]
    pub fn new() -> Self {
        Fragment_ {
            inner: BFragment::empty(Arc::new(DynamicSchema::default())),
        }
    }

    #[napi(factory)]
    pub fn from_array(nodes: Vec<&Node_>) -> Fragment_ {
        let schema = nodes
            .first()
            .map(|n| n.inner.schema.clone())
            .unwrap_or_else(|| Arc::new(DynamicSchema::default()));
        let frag = schema.with_types(|| {
            Fragment::from_array(
                nodes
                    .into_iter()
                    .map(|n| n.inner.inner.clone())
                    .collect::<Vec<_>>(),
            )
        });
        Fragment_ {
            inner: BFragment {
                schema,
                inner: frag,
            },
        }
    }

    #[napi(factory)]
    pub fn from_(input: Option<Either3<&Node_, Vec<&Node_>, &Fragment_>>) -> Fragment_ {
        let schema = match &input {
            Some(Either3::A(n)) => n.inner.schema.clone(),
            Some(Either3::B(ns)) => ns
                .first()
                .map(|n| n.inner.schema.clone())
                .unwrap_or_else(|| Arc::new(DynamicSchema::default())),
            Some(Either3::C(f)) => f.inner.schema.clone(),
            None => Arc::new(DynamicSchema::default()),
        };
        let finput = match input {
            None => FragmentFromInput::Null,
            Some(Either3::A(n)) => FragmentFromInput::SingleNode(n.inner.clone()),
            Some(Either3::B(ns)) => FragmentFromInput::NodeArray(
                ns.into_iter().map(|n| n.inner.inner.clone()).collect(),
            ),
            Some(Either3::C(f)) => FragmentFromInput::Fragment(f.inner.clone()),
        };
        Fragment_ {
            inner: b_fragment_from(schema, finput),
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

    #[napi(getter)]
    pub fn first_child(&self) -> Option<Node_> {
        self.inner.first_child().map(|n| Node_ { inner: n })
    }

    #[napi(getter)]
    pub fn last_child(&self) -> Option<Node_> {
        self.inner.last_child().map(|n| Node_ { inner: n })
    }

    #[napi]
    pub fn child(&self, index: u32) -> napi::Result<Node_> {
        self.inner
            .child(index as usize)
            .map(|n| Node_ { inner: n })
            .ok_or_else(|| napi::Error::new(Status::InvalidArg, "Index out of bounds"))
    }

    #[napi]
    pub fn maybe_child(&self, index: u32) -> Option<Node_> {
        self.inner
            .maybe_child(index as usize)
            .map(|n| Node_ { inner: n })
    }

    #[napi]
    pub fn replace_child(&self, index: u32, node: &Node_) -> Fragment_ {
        Fragment_ {
            inner: self
                .inner
                .replace_child(index as usize, node.inner.inner.clone()),
        }
    }

    #[napi]
    pub fn add_to_start(&self, node: &Node_) -> Fragment_ {
        Fragment_ {
            inner: self.inner.add_to_start(node.inner.inner.clone()),
        }
    }

    #[napi]
    pub fn add_to_end(&self, node: &Node_) -> Fragment_ {
        Fragment_ {
            inner: self.inner.add_to_end(node.inner.inner.clone()),
        }
    }

    #[napi]
    pub fn cut(&self, from: u32, to: Option<u32>) -> Fragment_ {
        Fragment_ {
            inner: self.inner.cut(from as usize, to.map(|t| t as usize)),
        }
    }

    #[napi]
    pub fn append(&self, other: &Fragment_) -> Fragment_ {
        Fragment_ {
            inner: self.inner.append(&other.inner),
        }
    }

    #[napi]
    pub fn eq(&self, other: &Fragment_) -> bool {
        self.inner.eq(&other.inner)
    }

    #[napi]
    pub fn find_diff_start(&self, other: &Fragment_) -> Option<u32> {
        self.inner.find_diff_start(&other.inner).map(|p| p as u32)
    }

    #[napi]
    pub fn find_diff_end(&self, other: &Fragment_) -> Option<DiffEnd> {
        self.inner
            .find_diff_end(&other.inner)
            .map(|(a, b)| DiffEnd {
                a: a as u32,
                b: b as u32,
            })
    }

    #[napi]
    pub fn text_between(
        &self,
        from: u32,
        to: u32,
        block_sep: Option<String>,
        leaf_text: Option<String>,
    ) -> String {
        self.inner.text_between(
            from as usize,
            to as usize,
            block_sep.as_deref(),
            leaf_text.as_deref(),
        )
    }

    #[napi]
    pub fn for_each(&self, env: Env, f: JsFunction) -> napi::Result<()> {
        let mut items: Vec<(DynamicNode, usize, usize)> = Vec::new();
        self.inner
            .for_each(&mut |node: &DynamicNode, offset, index| {
                items.push((node.clone(), offset, index))
            });
        for (node, offset, index) in items {
            let n = Node_ {
                inner: BNode {
                    schema: self.inner.schema.clone(),
                    inner: node,
                },
            };
            f.call(
                None,
                &[
                    n.into_instance(env)?.as_object(env).into_unknown(),
                    env.create_uint32(offset as u32)?.into_unknown(),
                    env.create_uint32(index as u32)?.into_unknown(),
                ],
            )?;
        }
        Ok(())
    }

    #[napi(js_name = "nodesBetween")]
    pub fn nodes_between(&self, env: Env, from: u32, to: u32, f: JsFunction) -> napi::Result<()> {
        let mut items: Vec<(DynamicNode, usize, Option<DynamicNode>, usize)> = Vec::new();
        self.inner.nodes_between(
            from as usize,
            to as usize,
            &mut |node: &DynamicNode, pos, parent, index| {
                items.push((node.clone(), pos, parent.cloned(), index));
                true
            },
        );
        for (node, pos, parent, index) in items {
            let n = Node_ {
                inner: BNode {
                    schema: self.inner.schema.clone(),
                    inner: node,
                },
            };
            let js_parent = match parent {
                Some(p) => {
                    let pn = Node_ {
                        inner: BNode {
                            schema: self.inner.schema.clone(),
                            inner: p,
                        },
                    };
                    pn.into_instance(env)?.as_object(env).into_unknown()
                }
                None => env.get_null()?.into_unknown(),
            };
            f.call(
                None,
                &[
                    n.into_instance(env)?.as_object(env).into_unknown(),
                    env.create_uint32(pos as u32)?.into_unknown(),
                    js_parent,
                    env.create_uint32(index as u32)?.into_unknown(),
                ],
            )?;
        }
        Ok(())
    }

    #[napi]
    pub fn descendants(&self, env: Env, f: JsFunction) -> napi::Result<()> {
        let mut items: Vec<(DynamicNode, usize, Option<DynamicNode>, usize)> = Vec::new();
        let size = self.inner.inner.size();
        self.inner.schema.with_types(|| {
            self.inner.inner.nodes_between(
                0,
                size,
                &mut |node: &DynamicNode, pos, parent, index| {
                    items.push((node.clone(), pos, parent.cloned(), index));
                    true
                },
                0,
                None,
            );
        });
        for (node, pos, parent, index) in items {
            let n = Node_ {
                inner: BNode {
                    schema: self.inner.schema.clone(),
                    inner: node,
                },
            };
            let js_parent = match parent {
                Some(p) => {
                    let pn = Node_ {
                        inner: BNode {
                            schema: self.inner.schema.clone(),
                            inner: p,
                        },
                    };
                    pn.into_instance(env)?.as_object(env).into_unknown()
                }
                None => env.get_null()?.into_unknown(),
            };
            f.call(
                None,
                &[
                    n.into_instance(env)?.as_object(env).into_unknown(),
                    env.create_uint32(pos as u32)?.into_unknown(),
                    js_parent,
                    env.create_uint32(index as u32)?.into_unknown(),
                ],
            )?;
        }
        Ok(())
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        self.inner.to_json()
    }
}

// ---------------------------------------------------------------------------
// NodeChildResult
// ---------------------------------------------------------------------------

/// Return type of [`Node_.child_after`] and [`Node_.child_before`].
#[napi]
pub struct NodeChildResult {
    pub(crate) inner_node: BNode,
    pub index: u32,
    pub offset: u32,
}

#[napi]
impl NodeChildResult {
    #[napi(getter)]
    pub fn node(&self) -> Node_ {
        Node_ {
            inner: self.inner_node.clone(),
        }
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
    pub(crate) inner: BSlice,
}

#[napi]
impl Slice_ {
    #[napi(constructor)]
    pub fn new(content: &Fragment_, open_start: u32, open_end: u32) -> Self {
        Slice_ {
            inner: BSlice::new(&content.inner, open_start as usize, open_end as usize),
        }
    }

    #[napi(getter)]
    pub fn empty() -> Slice_ {
        Slice_ {
            inner: BSlice::empty(Arc::new(DynamicSchema::default())),
        }
    }

    #[napi(getter)]
    pub fn content(&self) -> Fragment_ {
        Fragment_ {
            inner: self.inner.content(),
        }
    }

    #[napi(getter)]
    pub fn open_start(&self) -> u32 {
        self.inner.open_start() as u32
    }

    #[napi(getter)]
    pub fn open_end(&self) -> u32 {
        self.inner.open_end() as u32
    }

    #[napi(getter)]
    pub fn size(&self) -> u32 {
        self.inner.size() as u32
    }

    #[napi]
    pub fn eq(&self, other: &Slice_) -> bool {
        self.inner.eq(&other.inner)
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        self.inner.to_json()
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[napi]
pub struct Node_ {
    pub(crate) inner: BNode,
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
            inner: self.inner.type_(),
        }
    }

    #[napi(getter)]
    pub fn attrs(&self) -> Value {
        self.inner.attrs_json()
    }

    #[napi(getter)]
    pub fn content(&self) -> Fragment_ {
        Fragment_ {
            inner: self
                .inner
                .content()
                .unwrap_or_else(|| BFragment::empty(self.inner.schema.clone())),
        }
    }

    #[napi(getter)]
    pub fn marks(&self) -> Vec<Mark_> {
        self.inner
            .marks_vec()
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect()
    }

    #[napi(getter)]
    pub fn text(&self) -> Option<String> {
        self.inner.text()
    }

    #[napi(getter)]
    pub fn text_content(&self) -> String {
        self.inner.text_content()
    }

    #[napi(getter)]
    pub fn node_size(&self) -> u32 {
        self.inner.node_size() as u32
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
        self.inner.is_block()
    }

    #[napi(getter)]
    pub fn is_inline(&self) -> bool {
        self.inner.is_inline()
    }

    #[napi(getter)]
    pub fn is_leaf(&self) -> bool {
        self.inner.is_leaf()
    }

    #[napi(getter)]
    pub fn is_textblock(&self) -> bool {
        self.inner.is_textblock()
    }

    #[napi(getter)]
    pub fn is_atom(&self) -> bool {
        self.inner.is_atom()
    }

    #[napi(getter)]
    pub fn inline_content(&self) -> bool {
        self.inner.inline_content()
    }

    #[napi(getter)]
    pub fn first_child(&self) -> Option<Node_> {
        self.inner.first_child().map(|n| Node_ { inner: n })
    }

    #[napi(getter)]
    pub fn last_child(&self) -> Option<Node_> {
        self.inner.last_child().map(|n| Node_ { inner: n })
    }

    #[napi]
    pub fn child(&self, index: u32) -> Option<Node_> {
        self.inner.child(index as usize).map(|n| Node_ { inner: n })
    }

    #[napi]
    pub fn maybe_child(&self, index: u32) -> Option<Node_> {
        self.inner
            .maybe_child(index as usize)
            .map(|n| Node_ { inner: n })
    }

    #[napi]
    pub fn same_markup(&self, other: &Node_) -> bool {
        self.inner.same_markup(&other.inner)
    }

    #[napi]
    pub fn range_has_mark(&self, from: u32, to: u32, mark_type: &MarkType_) -> bool {
        self.inner
            .range_has_mark(from as usize, to as usize, mark_type.inner.inner)
    }

    #[napi]
    pub fn can_append(&self, other: &Node_) -> bool {
        self.inner.can_append(&other.inner)
    }

    #[napi]
    pub fn content_match_at(&self, index: u32) -> napi::Result<ContentMatch_> {
        self.inner
            .content_match_at(index as usize)
            .map(|cm| ContentMatch_ { inner: cm })
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    #[napi]
    pub fn text_between(
        &self,
        from: u32,
        to: u32,
        block_sep: Option<String>,
        leaf_text: Option<String>,
    ) -> String {
        self.inner.text_between(
            from as usize,
            to as usize,
            block_sep.as_deref(),
            leaf_text.as_deref(),
        )
    }

    #[napi]
    pub fn for_each(&self, env: Env, f: JsFunction) -> napi::Result<()> {
        let mut items: Vec<(DynamicNode, usize, usize)> = Vec::new();
        self.inner
            .for_each(&mut |node: &DynamicNode, offset, index| {
                items.push((node.clone(), offset, index))
            });
        for (node, offset, index) in items {
            let n = Node_ {
                inner: BNode {
                    schema: self.inner.schema.clone(),
                    inner: node,
                },
            };
            f.call(
                None,
                &[
                    n.into_instance(env)?.as_object(env).into_unknown(),
                    env.create_uint32(offset as u32)?.into_unknown(),
                    env.create_uint32(index as u32)?.into_unknown(),
                ],
            )?;
        }
        Ok(())
    }

    #[napi(js_name = "nodesBetween")]
    #[allow(clippy::too_many_arguments)]
    pub fn nodes_between(&self, env: Env, from: u32, to: u32, f: JsFunction) -> napi::Result<()> {
        let mut items: Vec<(DynamicNode, usize, Option<DynamicNode>, usize)> = Vec::new();
        self.inner.nodes_between(
            from as usize,
            to as usize,
            &mut |node: &DynamicNode, pos, parent, index| {
                items.push((node.clone(), pos, parent.cloned(), index));
                true
            },
        );
        for (node, pos, parent, index) in items {
            let n = Node_ {
                inner: BNode {
                    schema: self.inner.schema.clone(),
                    inner: node,
                },
            };
            let js_parent = match parent {
                Some(p) => {
                    let pn = Node_ {
                        inner: BNode {
                            schema: self.inner.schema.clone(),
                            inner: p,
                        },
                    };
                    pn.into_instance(env)?.as_object(env).into_unknown()
                }
                None => env.get_null()?.into_unknown(),
            };
            f.call(
                None,
                &[
                    n.into_instance(env)?.as_object(env).into_unknown(),
                    env.create_uint32(pos as u32)?.into_unknown(),
                    js_parent,
                    env.create_uint32(index as u32)?.into_unknown(),
                ],
            )?;
        }
        Ok(())
    }

    #[napi]
    pub fn descendants(&self, env: Env, f: JsFunction) -> napi::Result<()> {
        let mut items: Vec<(DynamicNode, usize, Option<DynamicNode>, usize)> = Vec::new();
        self.inner
            .descendants(&mut |node: &DynamicNode, pos, parent, index| {
                items.push((node.clone(), pos, parent.cloned(), index));
                true
            });
        for (node, pos, parent, index) in items {
            let n = Node_ {
                inner: BNode {
                    schema: self.inner.schema.clone(),
                    inner: node,
                },
            };
            let js_parent = match parent {
                Some(p) => {
                    let pn = Node_ {
                        inner: BNode {
                            schema: self.inner.schema.clone(),
                            inner: p,
                        },
                    };
                    pn.into_instance(env)?.as_object(env).into_unknown()
                }
                None => env.get_null()?.into_unknown(),
            };
            f.call(
                None,
                &[
                    n.into_instance(env)?.as_object(env).into_unknown(),
                    env.create_uint32(pos as u32)?.into_unknown(),
                    js_parent,
                    env.create_uint32(index as u32)?.into_unknown(),
                ],
            )?;
        }
        Ok(())
    }

    #[napi]
    pub fn slice(&self, from: u32, to: Option<u32>, include_parents: Option<bool>) -> Slice_ {
        let to = to.map(|t| t as usize).unwrap_or(self.inner.content_size());
        Slice_ {
            inner: self
                .inner
                .slice(from as usize, to, include_parents.unwrap_or(false)),
        }
    }

    #[napi]
    pub fn cut(&self, from: u32, to: Option<u32>) -> Node_ {
        let to = to.map(|t| t as usize).unwrap_or(self.inner.content_size());
        Node_ {
            inner: self.inner.cut(from as usize, to),
        }
    }

    #[napi]
    pub fn replace(&self, from: u32, to: u32, slice: &Slice_) -> napi::Result<Node_> {
        self.inner
            .replace(from as usize, to as usize, &slice.inner)
            .map(|n| Node_ { inner: n })
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    #[napi]
    pub fn resolve(&self, pos: u32) -> ResolvedPos_ {
        ResolvedPos_ {
            inner: self.inner.resolve(pos as usize),
        }
    }

    #[napi]
    pub fn eq(&self, other: &Node_) -> bool {
        self.inner.eq(&other.inner)
    }

    #[napi]
    pub fn to_json(&self) -> Value {
        self.inner.to_json(false)
    }

    #[napi]
    pub fn to_string(&self) -> String {
        self.inner.to_debug_string()
    }

    #[napi]
    pub fn mark(&self, marks: Vec<&Mark_>) -> Node_ {
        let mark_set = MarkSet::from_vec(marks.iter().map(|m| m.inner.inner.clone()).collect());
        Node_ {
            inner: self.inner.mark(mark_set),
        }
    }

    #[napi]
    pub fn copy(&self, content: &Fragment_) -> Node_ {
        Node_ {
            inner: self.inner.copy(content.inner.inner.clone()),
        }
    }

    #[napi]
    pub fn check(&self) -> napi::Result<()> {
        self.inner
            .check()
            .map_err(|e| napi::Error::new(Status::InvalidArg, e))
    }

    #[napi]
    pub fn node_at(&self, pos: u32) -> Option<Node_> {
        self.inner.node_at(pos as usize).map(|n| Node_ { inner: n })
    }

    #[napi(js_name = "hasMarkup")]
    pub fn has_markup(
        &self,
        type_: &NodeType_,
        attrs: Option<Value>,
        marks: Option<Vec<&Mark_>>,
    ) -> bool {
        let raw_marks: Option<Vec<_>> =
            marks.map(|ms| ms.iter().map(|m| m.inner.inner.clone()).collect());
        self.inner
            .has_markup(&type_.inner, attrs.as_ref(), raw_marks.as_deref())
    }

    #[napi(js_name = "canReplace")]
    pub fn can_replace(
        &self,
        from: u32,
        to: u32,
        replacement: Option<&Fragment_>,
        start: Option<u32>,
        end: Option<u32>,
    ) -> bool {
        self.inner.can_replace(
            from as usize,
            to as usize,
            replacement.map(|f| &f.inner),
            start.unwrap_or(0) as usize,
            end.map(|e| e as usize),
        )
    }

    #[napi(js_name = "canReplaceWith")]
    pub fn can_replace_with(&self, from: u32, to: u32, type_: &NodeType_) -> bool {
        self.inner
            .can_replace_with(from as usize, to as usize, &type_.inner)
    }

    #[napi(js_name = "childAfter")]
    pub fn child_after(&self, pos: u32) -> Option<NodeChildResult> {
        self.inner
            .child_after(pos as usize)
            .map(|(node, index, offset)| NodeChildResult {
                inner_node: node,
                index: index as u32,
                offset: offset as u32,
            })
    }

    #[napi(js_name = "childBefore")]
    pub fn child_before(&self, pos: u32) -> Option<NodeChildResult> {
        self.inner
            .child_before(pos as usize)
            .map(|(node, index, offset)| NodeChildResult {
                inner_node: node,
                index: index as u32,
                offset: offset as u32,
            })
    }
}

// ---------------------------------------------------------------------------
// ResolvedPos
// ---------------------------------------------------------------------------

#[napi]
pub struct ResolvedPos_ {
    pub(crate) inner: BResolvedPos,
}

#[napi]
impl ResolvedPos_ {
    #[napi(getter)]
    pub fn pos(&self) -> u32 {
        self.inner.pos as u32
    }

    #[napi(getter)]
    pub fn depth(&self) -> u32 {
        self.inner.depth() as u32
    }

    #[napi(getter)]
    pub fn parent(&self) -> Node_ {
        Node_ {
            inner: self.inner.parent(),
        }
    }

    #[napi(getter)]
    pub fn doc(&self) -> Node_ {
        Node_ {
            inner: self.inner.doc_node(),
        }
    }

    #[napi(getter)]
    pub fn parent_offset(&self) -> u32 {
        self.inner.parent_offset() as u32
    }

    #[napi(getter)]
    pub fn text_offset(&self) -> u32 {
        self.inner.text_offset() as u32
    }

    #[napi(getter)]
    pub fn node_before(&self) -> Option<Node_> {
        self.inner.node_before().map(|n| Node_ { inner: n })
    }

    #[napi(getter)]
    pub fn node_after(&self) -> Option<Node_> {
        self.inner.node_after().map(|n| Node_ { inner: n })
    }

    #[napi]
    pub fn node(&self, depth: Option<u32>) -> Node_ {
        Node_ {
            inner: self.inner.node(depth.map(|d| d as usize)),
        }
    }

    #[napi]
    pub fn index(&self, depth: Option<u32>) -> u32 {
        self.inner.index(depth.map(|d| d as usize)) as u32
    }

    #[napi]
    pub fn index_after(&self, depth: Option<u32>) -> u32 {
        self.inner.index_after(depth.map(|d| d as usize)) as u32
    }

    #[napi]
    pub fn start(&self, depth: Option<u32>) -> u32 {
        self.inner.start(depth.map(|d| d as usize)) as u32
    }

    #[napi]
    pub fn end(&self, depth: Option<u32>) -> u32 {
        self.inner.end(depth.map(|d| d as usize)) as u32
    }

    #[napi]
    pub fn before(&self, depth: Option<u32>) -> u32 {
        self.inner
            .before(depth.map(|d| d as usize))
            .map(|p| p as u32)
            .unwrap_or(0)
    }

    #[napi]
    pub fn after(&self, depth: Option<u32>) -> u32 {
        self.inner
            .after(depth.map(|d| d as usize))
            .map(|p| p as u32)
            .unwrap_or(0)
    }

    #[napi]
    pub fn shared_depth(&self, pos: u32) -> u32 {
        self.inner.shared_depth(pos as usize) as u32
    }

    #[napi]
    pub fn marks(&self) -> Vec<Mark_> {
        self.inner
            .marks()
            .into_iter()
            .map(|m| Mark_ {
                inner: BMark {
                    schema: self.inner.schema.clone(),
                    inner: m,
                },
            })
            .collect()
    }

    #[napi]
    pub fn marks_across(&self, end: &ResolvedPos_) -> Option<Vec<Mark_>> {
        self.inner.marks_across(&end.inner).map(|ms| {
            ms.into_iter()
                .map(|m| Mark_ {
                    inner: BMark {
                        schema: self.inner.schema.clone(),
                        inner: m,
                    },
                })
                .collect()
        })
    }

    #[napi]
    pub fn same_parent(&self, other: &ResolvedPos_) -> bool {
        self.inner.same_parent(&other.inner)
    }

    #[napi]
    pub fn max(&self, other: &ResolvedPos_) -> ResolvedPos_ {
        ResolvedPos_ {
            inner: self.inner.max(&other.inner),
        }
    }

    #[napi]
    pub fn min(&self, other: &ResolvedPos_) -> ResolvedPos_ {
        ResolvedPos_ {
            inner: self.inner.min(&other.inner),
        }
    }

    #[napi]
    pub fn pos_at_index(&self, index: u32, depth: Option<u32>) -> u32 {
        self.inner
            .pos_at_index(index as usize, depth.map(|d| d as usize)) as u32
    }

    #[napi(js_name = "blockRange")]
    pub fn block_range(&self, other: Option<&ResolvedPos_>) -> Option<NodeRange_> {
        self.inner
            .block_range(other.map(|o| &o.inner))
            .map(|nr| NodeRange_ { inner: nr })
    }
}

// ---------------------------------------------------------------------------
// NodeRange
// ---------------------------------------------------------------------------

#[napi]
pub struct NodeRange_ {
    pub(crate) inner: BNodeRange,
}

impl NodeRange_ {
    pub(crate) fn to_node_range(
        &self,
    ) -> Option<prosemirror::transform::structure::NodeRange<'_, Dyn>> {
        self.inner.to_node_range()
    }
}

#[napi]
impl NodeRange_ {
    #[napi(getter)]
    pub fn from(&self) -> ResolvedPos_ {
        ResolvedPos_ {
            inner: self.inner.from_resolved_pos(),
        }
    }

    #[napi(getter)]
    pub fn to(&self) -> ResolvedPos_ {
        ResolvedPos_ {
            inner: self.inner.to_resolved_pos(),
        }
    }

    #[napi(getter)]
    pub fn depth(&self) -> u32 {
        self.inner.depth() as u32
    }

    #[napi(getter)]
    pub fn start(&self) -> u32 {
        self.inner.start() as u32
    }

    #[napi(getter)]
    pub fn end(&self) -> u32 {
        self.inner.end() as u32
    }

    #[napi(getter)]
    pub fn parent(&self) -> Node_ {
        Node_ {
            inner: self.inner.parent(),
        }
    }

    #[napi(getter, js_name = "startIndex")]
    pub fn start_index(&self) -> u32 {
        self.inner.start_index() as u32
    }

    #[napi(getter, js_name = "endIndex")]
    pub fn end_index(&self) -> u32 {
        self.inner.end_index() as u32
    }
}

// ---------------------------------------------------------------------------
// ContentMatch
// ---------------------------------------------------------------------------

#[napi]
pub struct ContentMatch_ {
    pub(crate) inner: BContentMatch,
}

#[napi]
impl ContentMatch_ {
    #[napi]
    pub fn match_type(&self, type_: &NodeType_) -> Option<ContentMatch_> {
        self.inner
            .match_type(&type_.inner)
            .map(|cm| ContentMatch_ { inner: cm })
    }

    #[napi]
    pub fn match_fragment(&self, frag: &Fragment_) -> Option<ContentMatch_> {
        self.inner
            .match_fragment(&frag.inner, 0, None)
            .map(|cm| ContentMatch_ { inner: cm })
    }

    #[napi]
    pub fn fill_before(
        &self,
        after: &Fragment_,
        to_end: Option<bool>,
        start_index: Option<u32>,
    ) -> Option<Fragment_> {
        self.inner
            .fill_before(
                &after.inner,
                to_end.unwrap_or(false),
                start_index.unwrap_or(0) as usize,
            )
            .map(|f| Fragment_ { inner: f })
    }

    #[napi(getter)]
    pub fn valid_end(&self) -> bool {
        self.inner.valid_end()
    }

    #[napi(getter)]
    pub fn default_type(&self) -> Option<NodeType_> {
        self.inner.default_type().map(|nt| NodeType_ { inner: nt })
    }

    #[napi]
    pub fn find_wrapping(&self, target: &NodeType_) -> Option<Vec<NodeType_>> {
        self.inner.find_wrapping(&target.inner).map(|types| {
            types
                .into_iter()
                .map(|nt| NodeType_ { inner: nt })
                .collect()
        })
    }

    #[napi(getter)]
    pub fn edge_count(&self) -> u32 {
        self.inner.edge_count() as u32
    }

    #[napi]
    pub fn edge_type(&self, n: u32) -> Option<NodeType_> {
        self.inner
            .edge(n as usize)
            .map(|(nt, _)| NodeType_ { inner: nt })
    }

    #[napi]
    pub fn edge_match(&self, n: u32) -> Option<ContentMatch_> {
        self.inner
            .edge(n as usize)
            .map(|(_, cm)| ContentMatch_ { inner: cm })
    }
}

#[napi]
pub fn content_match_parse(expr: String, schema: &Schema) -> napi::Result<ContentMatch_> {
    BContentMatch::parse(&expr, &schema.inner)
        .map(|inner| ContentMatch_ { inner })
        .map_err(|e| napi::Error::new(napi::Status::InvalidArg, e))
}

/// `Mark.none` — the empty mark set constant (a JS static property).
#[napi(js_name = "markNone")]
pub fn mark_none() -> Vec<Mark_> {
    Vec::new()
}
