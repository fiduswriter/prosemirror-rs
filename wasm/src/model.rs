//! WASM bindings for prosemirror-model types.
//!
//! Each struct wraps a `B*` inner value from `prosemirror::binding::model`
//! and forwards every method via `#[wasm_bindgen]`.

use std::sync::Arc;

use js_sys::{Array, Function, Object, Reflect};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use prosemirror::binding::model::*;
use prosemirror::dynamic::types::{DynamicMark, DynamicNode};
use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::{Fragment as ModelFragment, MarkSet, Node as ModelNode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn js_to_value(js: &JsValue) -> Result<Value, JsValue> {
    if js.is_null() || js.is_undefined() {
        return Ok(Value::Null);
    }
    serde_wasm_bindgen::from_value(js.clone())
        .map_err(|e| JsValue::from_str(&format!("JSON conversion failed: {e}")))
}

/// Convert a serde_json `Value` to a JavaScript value.
/// Unlike `serde_wasm_bindgen::to_value`, this produces plain JS objects
/// (not Maps) for JSON objects, and plain arrays for JSON arrays.
pub(crate) fn value_to_js(value: &Value) -> Result<JsValue, JsValue> {
    match value {
        Value::Null => Ok(JsValue::null()),
        Value::Bool(b) => Ok(JsValue::from_bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(JsValue::from_f64(i as f64))
            } else if let Some(u) = n.as_u64() {
                Ok(JsValue::from_f64(u as f64))
            } else if let Some(f) = n.as_f64() {
                Ok(JsValue::from_f64(f))
            } else {
                Err(JsValue::from_str(&format!("invalid number: {n}")))
            }
        }
        Value::String(s) => Ok(JsValue::from_str(s)),
        Value::Array(arr) => {
            let js_arr = Array::new();
            for v in arr {
                js_arr.push(&value_to_js(v)?);
            }
            Ok(js_arr.into())
        }
        Value::Object(map) => {
            let obj = Object::new();
            for (k, v) in map {
                Reflect::set(&obj, &JsValue::from_str(k), &value_to_js(v)?).map_err(|e| {
                    JsValue::from_str(&format!("Failed to set property {k}: {e:?}"))
                })?;
            }
            Ok(obj.into())
        }
    }
}

fn js_to_opt_str(js: &JsValue) -> Option<String> {
    if js.is_null() || js.is_undefined() {
        None
    } else {
        js.as_string()
    }
}

fn marks_to_js_array(schema: &Arc<DynamicSchema>, marks: &[DynamicMark]) -> Vec<Mark> {
    marks
        .iter()
        .map(|m| Mark {
            inner: BMark {
                schema: schema.clone(),
                inner: m.clone(),
            },
        })
        .collect()
}

fn mark_types_to_js_array(types: &[BMarkType]) -> Vec<MarkType> {
    types
        .iter()
        .map(|mt| MarkType {
            inner: BMarkType {
                schema: mt.schema.clone(),
                inner: mt.inner,
                name: mt.name.clone(),
            },
        })
        .collect()
}

fn js_to_fragment_input(
    _schema: &Arc<DynamicSchema>,
    js: &JsValue,
) -> Result<FragmentFromInput, JsValue> {
    if js.is_null() || js.is_undefined() {
        return Ok(FragmentFromInput::Null);
    }
    // Only Array-based detection is supported without JsCast.
    // For typed content, use the `content: Option<Fragment>` parameter directly.
    if Array::is_array(js) {
        let arr: &Array = js.dyn_ref().unwrap();
        let mut nodes = Vec::new();
        for item in arr.iter() {
            // Try to convert from JSON representation as fallback for array elements
            let val: Value = serde_wasm_bindgen::from_value(item.clone())
                .map_err(|e| JsValue::from_str(&format!("Invalid node JSON: {e}")))?;
            let node: DynamicNode = _schema.with_types(|| {
                serde_json::from_value::<DynamicNode>(val)
                    .map_err(|e| JsValue::from_str(&format!("Invalid node: {e}")))
            })?;
            nodes.push(node);
        }
        return Ok(FragmentFromInput::NodeArray(nodes));
    }
    // Fallback: try to parse as a single Node JSON
    if let Ok(val) = serde_wasm_bindgen::from_value::<Value>(js.clone()) {
        if val.is_object() && val.get("type").is_some() {
            let node: DynamicNode = _schema.with_types(|| {
                serde_json::from_value::<DynamicNode>(val)
                    .map_err(|_| JsValue::from_str("Invalid node"))
            })?;
            return Ok(FragmentFromInput::SingleNode(BNode {
                schema: _schema.clone(),
                inner: node,
            }));
        }
    }
    Err(JsValue::from_str(
        "Fragment.from: expected null, Array, or a JSON node object",
    ))
}

fn build_child_result(
    _schema: &Arc<DynamicSchema>,
    data: Option<(BNode, usize, usize)>,
) -> Option<Object> {
    data.map(|(node, index, offset)| {
        let obj = Object::new();
        let node_js = Node { inner: node };
        Reflect::set(&obj, &JsValue::from_str("node"), &JsValue::from(node_js)).unwrap();
        Reflect::set(
            &obj,
            &JsValue::from_str("index"),
            &JsValue::from_f64(index as f64),
        )
        .unwrap();
        Reflect::set(
            &obj,
            &JsValue::from_str("offset"),
            &JsValue::from_f64(offset as f64),
        )
        .unwrap();
        obj
    })
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// A ProseMirror document schema, built from a JSON spec.
#[wasm_bindgen]
pub struct Schema {
    pub(crate) inner: Arc<DynamicSchema>,
}

#[wasm_bindgen]
impl Schema {
    /// Create a new Schema from a JSON `SchemaSpec` string.
    #[wasm_bindgen(constructor)]
    pub fn new(spec_json: &str) -> Result<Schema, JsValue> {
        let value: Value = serde_json::from_str(spec_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid JSON: {e}")))?;
        let schema =
            DynamicSchema::from_json(&value).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Schema {
            inner: Arc::new(schema),
        })
    }

    /// All node types in this schema, keyed by name.
    #[wasm_bindgen(getter)]
    pub fn nodes(&self) -> Result<Object, JsValue> {
        let obj = Object::new();
        for (name, idx) in &self.inner.node_type_map {
            let nt = BNodeType::new(
                self.inner.clone(),
                prosemirror::dynamic::types::DynamicNodeType { idx: *idx },
                name.clone(),
            );
            let node_type = NodeType { inner: nt };
            Reflect::set(&obj, &JsValue::from_str(name), &JsValue::from(node_type))
                .map_err(|e| JsValue::from_str(&format!("Failed to set property: {e:?}")))?;
        }
        Ok(obj)
    }

    /// All mark types in this schema, keyed by name.
    #[wasm_bindgen(getter)]
    pub fn marks(&self) -> Result<Object, JsValue> {
        let obj = Object::new();
        for (name, idx) in &self.inner.mark_type_map {
            let mt = BMarkType {
                schema: self.inner.clone(),
                inner: prosemirror::dynamic::types::DynamicMarkType { idx: *idx },
                name: name.clone(),
            };
            let mark_type = MarkType { inner: mt };
            Reflect::set(&obj, &JsValue::from_str(name), &JsValue::from(mark_type))
                .map_err(|e| JsValue::from_str(&format!("Failed to set property: {e:?}")))?;
        }
        Ok(obj)
    }

    /// Create a node of the given type.
    ///
    /// `attrs` can be a JSON object or null.
    /// `content` is an optional Fragment.
    pub fn node(
        &self,
        type_name: &str,
        attrs: JsValue,
        content: Option<Fragment>,
        marks: Vec<Mark>,
    ) -> Result<Node, JsValue> {
        let attrs_val = js_to_value(&attrs)?;
        let fragment = match content {
            Some(f) => f.inner.inner.clone(),
            None => ModelFragment::new(),
        };

        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let marks_val = MarkSet::from_vec(dyn_marks);

        let inner = self
            .inner
            .node(type_name, attrs_val, fragment, marks_val)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(Node {
            inner: BNode {
                schema: self.inner.clone(),
                inner,
            },
        })
    }

    /// Create a text node, optionally with marks.
    pub fn text(&self, text: &str, marks: Vec<Mark>) -> Node {
        let inner_node = self.inner.text(text);
        let mut b_node = BNode {
            schema: self.inner.clone(),
            inner: inner_node,
        };

        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        if !dyn_marks.is_empty() {
            let mark_set = MarkSet::from_vec(dyn_marks);
            b_node.inner = self.inner.with_types(|| b_node.inner.mark(mark_set));
        }

        Node { inner: b_node }
    }

    /// Create a mark of the given type.
    pub fn mark(&self, type_name: &str, attrs: JsValue) -> Result<Mark, JsValue> {
        let attrs_val = js_to_value(&attrs)?;
        let mt = self
            .inner
            .mark_type(type_name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown mark type: {type_name}")))?;
        let b_mark_type = BMarkType {
            schema: self.inner.clone(),
            inner: mt,
            name: type_name.to_string(),
        };
        let b_mark = b_mark_type.create(attrs_val);
        Ok(Mark { inner: b_mark })
    }

    /// Create a node from a JSON representation.
    #[wasm_bindgen(js_name = nodeFromJSON)]
    pub fn node_from_json(&self, json: JsValue) -> Result<Node, JsValue> {
        let val = js_to_value(&json)?;
        let inner_node = self
            .inner
            .node_from_json(&val)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Node {
            inner: BNode {
                schema: self.inner.clone(),
                inner: inner_node,
            },
        })
    }

    /// Create a mark from a JSON representation.
    #[wasm_bindgen(js_name = markFromJSON)]
    pub fn mark_from_json(&self, json: JsValue) -> Result<Mark, JsValue> {
        let val = js_to_value(&json)?;
        let inner_mark = self
            .inner
            .mark_from_json(&val)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Mark {
            inner: BMark {
                schema: self.inner.clone(),
                inner: inner_mark,
            },
        })
    }

    /// The top-level node type (typically "doc").
    #[wasm_bindgen(getter, js_name = topNodeType)]
    pub fn top_node_type(&self) -> Option<NodeType> {
        b_schema_top_node_type(&self.inner).map(|nt| NodeType { inner: nt })
    }
}

// ---------------------------------------------------------------------------
// NodeType
// ---------------------------------------------------------------------------

/// A node type in the schema.
#[wasm_bindgen]
pub struct NodeType {
    pub(crate) inner: BNodeType,
}

#[wasm_bindgen]
impl NodeType {
    /// The schema this node type belongs to.
    #[wasm_bindgen(getter)]
    pub fn schema(&self) -> Schema {
        Schema {
            inner: self.inner.schema.clone(),
        }
    }

    /// The name of this node type.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    /// True if this is a block node type.
    #[wasm_bindgen(getter, js_name = isBlock)]
    pub fn is_block(&self) -> bool {
        self.inner.is_block()
    }

    /// True if this is an inline node type.
    #[wasm_bindgen(getter, js_name = isInline)]
    pub fn is_inline(&self) -> bool {
        self.inner.is_inline()
    }

    /// True if this is a textblock node type.
    #[wasm_bindgen(getter, js_name = isTextblock)]
    pub fn is_textblock(&self) -> bool {
        self.inner.is_textblock()
    }

    /// True if this is an atom node type.
    #[wasm_bindgen(getter, js_name = isAtom)]
    pub fn is_atom(&self) -> bool {
        self.inner.is_atom()
    }

    /// True if this is a leaf node type.
    #[wasm_bindgen(getter, js_name = isLeaf)]
    pub fn is_leaf(&self) -> bool {
        self.inner.is_leaf()
    }

    /// True if this is the text node type.
    #[wasm_bindgen(getter, js_name = isText)]
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    /// True if this node type allows inline content.
    #[wasm_bindgen(getter, js_name = inlineContent)]
    pub fn inline_content(&self) -> bool {
        self.inner.inline_content()
    }

    /// Get the ContentMatch for this node type's content expression.
    #[wasm_bindgen(getter, js_name = contentMatch)]
    pub fn content_match(&self) -> Option<ContentMatch> {
        self.inner
            .content_match()
            .map(|cm| ContentMatch { inner: cm })
    }

    /// True if this node type has required attributes.
    #[wasm_bindgen(js_name = hasRequiredAttrs)]
    pub fn has_required_attrs(&self) -> bool {
        self.inner.has_required_attrs()
    }

    /// True if this node type's content can be placed in the other node type.
    #[wasm_bindgen(js_name = compatibleContent)]
    pub fn compatible_content(&self, other: &NodeType) -> bool {
        self.inner.compatible_content(&other.inner)
    }

    /// The whitespace behaviour for this node type.
    #[wasm_bindgen(getter)]
    pub fn whitespace(&self) -> String {
        self.inner.whitespace()
    }

    /// True if this node type is rendered as pre-formatted (code).
    #[wasm_bindgen(getter, js_name = isCode)]
    pub fn is_code(&self) -> bool {
        self.inner.is_code()
    }

    /// Create a node of this type.
    pub fn create(
        &self,
        attrs: JsValue,
        content: Option<Fragment>,
        marks: Vec<Mark>,
    ) -> Result<Node, JsValue> {
        let attrs_val = js_to_value(&attrs)?;
        let fragment = match content {
            Some(f) => f.inner.inner.clone(),
            None => ModelFragment::new(),
        };

        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let marks_val = MarkSet::from_vec(dyn_marks);

        let inner = self.inner.create(attrs_val, fragment, marks_val);
        Ok(Node { inner })
    }

    /// Create a node of this type, returning an error if invalid.
    #[wasm_bindgen(js_name = createChecked)]
    pub fn create_checked(
        &self,
        attrs: JsValue,
        content: Option<Fragment>,
        marks: Vec<Mark>,
    ) -> Result<Node, JsValue> {
        let attrs_val = js_to_value(&attrs)?;
        let fragment = match content {
            Some(f) => f.inner.inner.clone(),
            None => ModelFragment::new(),
        };

        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let marks_val = MarkSet::from_vec(dyn_marks);

        let inner = self
            .inner
            .create_checked(attrs_val, fragment, marks_val)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Node { inner })
    }

    /// Create a node of this type and fill in the content if possible.
    #[wasm_bindgen(js_name = createAndFill)]
    pub fn create_and_fill(
        &self,
        attrs: JsValue,
        content: Option<Fragment>,
        marks: Vec<Mark>,
    ) -> Result<Option<Node>, JsValue> {
        let attrs_val = js_to_value(&attrs)?;
        let content_opt = content.map(|f| f.inner.inner.clone());

        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let marks_val = MarkSet::from_vec(dyn_marks);

        Ok(self
            .inner
            .create_and_fill(attrs_val, content_opt, marks_val)
            .map(|inner| Node { inner }))
    }

    /// Check whether the given fragment is valid content for this node type.
    #[wasm_bindgen(js_name = validContent)]
    pub fn valid_content(&self, fragment: &Fragment) -> bool {
        self.inner.valid_content(&fragment.inner.inner)
    }

    /// True if this node type allows the given mark type.
    #[wasm_bindgen(js_name = allowsMarkType)]
    pub fn allows_mark_type(&self, mark_type: &MarkType) -> bool {
        self.inner.allows_mark_type(&mark_type.inner)
    }

    /// True if this node type allows the given set of marks.
    #[wasm_bindgen(js_name = allowsMarks)]
    pub fn allows_marks(&self, marks: Vec<Mark>) -> bool {
        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let mark_set = MarkSet::from_vec(dyn_marks);
        self.inner.allows_marks(&mark_set)
    }

    /// True if this node type belongs to the given group.
    #[wasm_bindgen(js_name = isInGroup)]
    pub fn is_in_group(&self, group: &str) -> bool {
        self.inner.is_in_group(group)
    }

    /// Default attributes for this node type, as a JSON object.
    #[wasm_bindgen(getter)]
    pub fn attrs(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.attrs_defaults();
        value_to_js(&val)
    }

    /// The set of mark types allowed by this node type (null if all marks are allowed).
    #[wasm_bindgen(getter, js_name = markSet)]
    pub fn mark_set(&self) -> Option<Vec<MarkType>> {
        self.inner
            .mark_set()
            .map(|types| mark_types_to_js_array(&types))
    }

    /// Filter the given marks to only those allowed by this node type.
    #[wasm_bindgen(js_name = allowedMarks)]
    pub fn allowed_marks(&self, marks: Vec<Mark>) -> Vec<Mark> {
        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let filtered = self.inner.allowed_marks_filtered(dyn_marks);
        marks_to_js_array(&self.inner.schema, &filtered)
    }

    /// The spec for this node type as a JSON object.
    #[wasm_bindgen(getter)]
    pub fn spec(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.spec_json();
        value_to_js(&val)
    }
}

// ---------------------------------------------------------------------------
// MarkType
// ---------------------------------------------------------------------------

/// A mark type in the schema.
#[wasm_bindgen]
pub struct MarkType {
    pub(crate) inner: BMarkType,
}

#[wasm_bindgen]
impl MarkType {
    /// The schema this mark type belongs to.
    #[wasm_bindgen(getter)]
    pub fn schema(&self) -> Schema {
        Schema {
            inner: self.inner.schema.clone(),
        }
    }

    /// The name of this mark type.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    /// Create a mark of this type.
    pub fn create(&self, attrs: JsValue) -> Result<Mark, JsValue> {
        let attrs_val = js_to_value(&attrs)?;
        let inner = self.inner.create(attrs_val);
        Ok(Mark { inner })
    }

    /// Remove all marks of this type from the given set.
    #[wasm_bindgen(js_name = removeFromSet)]
    pub fn remove_from_set(&self, marks: Vec<Mark>) -> Vec<Mark> {
        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let result = self.inner.remove_from_set(dyn_marks);
        marks_to_js_array(&self.inner.schema, &result)
    }

    /// Find the first mark of this type in the given set.
    #[wasm_bindgen(js_name = isInSet)]
    pub fn is_in_set(&self, marks: Vec<Mark>) -> Option<Mark> {
        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner.is_in_set(&dyn_marks).map(|inner| Mark { inner })
    }

    /// True if this mark type excludes the given mark type.
    pub fn excludes(&self, other: &MarkType) -> bool {
        self.inner.excludes(&other.inner)
    }

    /// The spec for this mark type as a JSON object.
    #[wasm_bindgen(getter)]
    pub fn spec(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.spec_json();
        value_to_js(&val)
    }
}

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------

/// A mark — a piece of metadata attached to a node.
#[wasm_bindgen]
pub struct Mark {
    pub(crate) inner: BMark,
}

#[wasm_bindgen]
impl Mark {
    /// The mark type.
    #[wasm_bindgen(getter)]
    pub fn type_(&self) -> MarkType {
        MarkType {
            inner: self.inner.type_(),
        }
    }

    /// The mark's attributes as a JSON object.
    #[wasm_bindgen(getter)]
    pub fn attrs(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.attrs_json();
        value_to_js(&val)
    }

    /// JSON representation: `{type, attrs}`.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.to_json();
        value_to_js(&val)
    }

    /// Add this mark to a set of marks, returning a new sorted set.
    #[wasm_bindgen(js_name = addToSet)]
    pub fn add_to_set(&self, set: Vec<Mark>) -> Vec<Mark> {
        let dyn_marks: Vec<DynamicMark> = set.iter().map(|m| m.inner.inner.clone()).collect();
        let result = self.inner.add_to_set(dyn_marks);
        marks_to_js_array(&self.inner.schema, &result)
    }

    /// Remove this mark from a set of marks.
    #[wasm_bindgen(js_name = removeFromSet)]
    pub fn remove_from_set(&self, set: Vec<Mark>) -> Vec<Mark> {
        let dyn_marks: Vec<DynamicMark> = set.iter().map(|m| m.inner.inner.clone()).collect();
        let result = self.inner.remove_from_set(dyn_marks);
        marks_to_js_array(&self.inner.schema, &result)
    }

    /// True if this mark is in the given set.
    #[wasm_bindgen(js_name = isInSet)]
    pub fn is_in_set(&self, set: Vec<Mark>) -> bool {
        let dyn_marks: Vec<DynamicMark> = set.iter().map(|m| m.inner.inner.clone()).collect();
        self.inner.is_in_set(&dyn_marks)
    }

    /// True if this mark is equal to another mark.
    pub fn eq(&self, other: &Mark) -> bool {
        self.inner.eq(&other.inner)
    }

    /// Test whether two mark sets are equal (same marks in the same order).
    #[wasm_bindgen(js_name = sameSet)]
    pub fn same_set(a: Vec<Mark>, b: Vec<Mark>) -> bool {
        let marks_a: Vec<DynamicMark> = a.iter().map(|m| m.inner.inner.clone()).collect();
        let marks_b: Vec<DynamicMark> = b.iter().map(|m| m.inner.inner.clone()).collect();
        BMark::same_set(&marks_a, &marks_b)
    }

    /// Create a sorted, deduplicated mark set from an array of marks.
    #[wasm_bindgen(js_name = setFrom)]
    pub fn set_from(schema: &Schema, marks: Vec<Mark>) -> Vec<Mark> {
        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let result = BMark::set_from(&schema.inner, dyn_marks);
        marks_to_js_array(&schema.inner, &result)
    }
}

// ---------------------------------------------------------------------------
// Fragment
// ---------------------------------------------------------------------------

/// A fragment represents a node's collection of child nodes.
#[wasm_bindgen]
pub struct Fragment {
    pub(crate) inner: BFragment,
}

#[wasm_bindgen]
impl Fragment {
    /// Create a fragment from an array of child nodes.
    #[wasm_bindgen(js_name = fromArray)]
    pub fn from_array(schema: &Schema, nodes: Vec<Node>) -> Fragment {
        let dyn_nodes: Vec<DynamicNode> = nodes.iter().map(|n| n.inner.inner.clone()).collect();
        let inner = schema
            .inner
            .with_types(|| ModelFragment::from_array(dyn_nodes));
        Fragment {
            inner: BFragment {
                schema: schema.inner.clone(),
                inner,
            },
        }
    }

    /// Create a fragment from a polymorphic input (Node, Array of Node,
    /// Fragment, or null).
    pub fn from(schema: &Schema, input: JsValue) -> Result<Fragment, JsValue> {
        let frag_input = js_to_fragment_input(&schema.inner, &input)?;
        let b_frag = b_fragment_from(schema.inner.clone(), frag_input);
        Ok(Fragment { inner: b_frag })
    }

    /// Total size of the fragment (sum of child node sizes).
    #[wasm_bindgen(getter)]
    pub fn size(&self) -> usize {
        self.inner.size()
    }

    /// Number of child nodes.
    #[wasm_bindgen(getter, js_name = childCount)]
    pub fn child_count(&self) -> usize {
        self.inner.child_count()
    }

    /// Get the child at the given index, or null.
    pub fn child(&self, index: usize) -> Option<Node> {
        self.inner.child(index).map(|n| Node { inner: n })
    }

    /// Like `child`, but returns null if index is out of range.
    #[wasm_bindgen(js_name = maybeChild)]
    pub fn maybe_child(&self, index: usize) -> Option<Node> {
        self.inner.maybe_child(index).map(|n| Node { inner: n })
    }

    /// The first child, or null if empty.
    #[wasm_bindgen(getter, js_name = firstChild)]
    pub fn first_child(&self) -> Option<Node> {
        self.inner.first_child().map(|n| Node { inner: n })
    }

    /// The last child, or null if empty.
    #[wasm_bindgen(getter, js_name = lastChild)]
    pub fn last_child(&self) -> Option<Node> {
        self.inner.last_child().map(|n| Node { inner: n })
    }

    /// Cut out a sub-fragment between the given positions.
    pub fn cut(&self, from: usize, to: Option<usize>) -> Fragment {
        Fragment {
            inner: self.inner.cut(from, to),
        }
    }

    /// Append another fragment's children to this one.
    pub fn append(&self, other: &Fragment) -> Fragment {
        Fragment {
            inner: self.inner.append(&other.inner),
        }
    }

    /// Replace a child at the given index.
    #[wasm_bindgen(js_name = replaceChild)]
    pub fn replace_child(&self, index: usize, node: &Node) -> Fragment {
        Fragment {
            inner: self.inner.replace_child(index, node.inner.inner.clone()),
        }
    }

    /// Add a node to the start of the fragment.
    #[wasm_bindgen(js_name = addToStart)]
    pub fn add_to_start(&self, node: &Node) -> Fragment {
        Fragment {
            inner: self.inner.add_to_start(node.inner.inner.clone()),
        }
    }

    /// Add a node to the end of the fragment.
    #[wasm_bindgen(js_name = addToEnd)]
    pub fn add_to_end(&self, node: &Node) -> Fragment {
        Fragment {
            inner: self.inner.add_to_end(node.inner.inner.clone()),
        }
    }

    /// True if this fragment is equal to another.
    pub fn eq(&self, other: &Fragment) -> bool {
        self.inner.eq(&other.inner)
    }

    /// Find the first position at which this fragment differs from another.
    #[wasm_bindgen(js_name = findDiffStart)]
    pub fn find_diff_start(&self, other: &Fragment, pos: usize) -> Option<usize> {
        self.inner.find_diff_start(&other.inner, pos)
    }

    /// Find the position and dimensions at which this fragment ends differently.
    #[wasm_bindgen(js_name = findDiffEnd)]
    pub fn find_diff_end(&self, other: &Fragment, pos_a: usize, pos_b: usize) -> Option<JsValue> {
        self.inner
            .find_diff_end(&other.inner, pos_a, pos_b)
            .map(|(a, b)| {
                let obj = Object::new();
                Reflect::set(&obj, &JsValue::from_str("a"), &JsValue::from_f64(a as f64)).ok();
                Reflect::set(&obj, &JsValue::from_str("b"), &JsValue::from_f64(b as f64)).ok();
                obj.into()
            })
    }

    /// Get the text content between two positions.
    #[wasm_bindgen(js_name = textBetween)]
    pub fn text_between(
        &self,
        from: usize,
        to: usize,
        block_sep: JsValue,
        leaf_text: JsValue,
    ) -> String {
        let block_sep_opt = js_to_opt_str(&block_sep);
        let leaf_text_opt = js_to_opt_str(&leaf_text);
        self.inner
            .text_between(from, to, block_sep_opt.as_deref(), leaf_text_opt.as_deref())
    }

    /// JSON representation of this fragment.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.to_json();
        value_to_js(&val)
    }

    /// Call a function for each direct child node.
    ///
    /// The callback receives `(node, offset, index)`.
    #[wasm_bindgen(js_name = forEach)]
    pub fn for_each(&self, f: &Function) -> Result<(), JsValue> {
        let this = JsValue::null();
        let mut should_stop = false;
        self.inner
            .for_each(&mut |node: &DynamicNode, offset, index| {
                if should_stop {
                    return;
                }
                let node_js = Node {
                    inner: BNode {
                        schema: self.inner.schema.clone(),
                        inner: node.clone(),
                    },
                };
                let result = f.call3(
                    &this,
                    &JsValue::from(node_js),
                    &JsValue::from_f64(offset as f64),
                    &JsValue::from_f64(index as f64),
                );
                match result {
                    Ok(r) => {
                        if let Some(b) = r.as_bool() {
                            if !b {
                                should_stop = true;
                            }
                        }
                    }
                    Err(_) => {
                        should_stop = true;
                    }
                }
            });
        Ok(())
    }

    /// Call a function for all descendant nodes between two positions.
    ///
    /// The callback receives `(node, pos, parent, index)`. Return false to stop.
    #[wasm_bindgen(js_name = nodesBetween)]
    pub fn nodes_between(&self, from: usize, to: usize, f: &Function) -> Result<(), JsValue> {
        let this = JsValue::null();
        self.inner
            .nodes_between(from, to, &mut |node: &DynamicNode, pos, parent, index| {
                let node_js = Node {
                    inner: BNode {
                        schema: self.inner.schema.clone(),
                        inner: node.clone(),
                    },
                };
                let parent_js = parent
                    .map(|p| {
                        JsValue::from(Node {
                            inner: BNode {
                                schema: self.inner.schema.clone(),
                                inner: p.clone(),
                            },
                        })
                    })
                    .unwrap_or_else(JsValue::null);
                let result = f.call4(
                    &this,
                    &JsValue::from(node_js),
                    &JsValue::from_f64(pos as f64),
                    &parent_js,
                    &JsValue::from_f64(index as f64),
                );
                match result {
                    Ok(r) => r.as_bool().unwrap_or(true),
                    Err(_) => false,
                }
            });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

/// A slice represents a piece cut out of a document.
#[wasm_bindgen]
pub struct Slice {
    pub(crate) inner: BSlice,
}

#[wasm_bindgen]
impl Slice {
    /// Create a new slice from a fragment.
    #[wasm_bindgen(constructor)]
    pub fn new(content: &Fragment, open_start: usize, open_end: usize) -> Slice {
        Slice {
            inner: BSlice::new(&content.inner, open_start, open_end),
        }
    }

    /// Create an empty slice for the given schema.
    pub fn empty(schema: &Schema) -> Slice {
        Slice {
            inner: BSlice::empty(schema.inner.clone()),
        }
    }

    /// The content fragment of this slice.
    #[wasm_bindgen(getter)]
    pub fn content(&self) -> Fragment {
        Fragment {
            inner: self.inner.content(),
        }
    }

    /// The open depth at the start.
    #[wasm_bindgen(getter, js_name = openStart)]
    pub fn open_start(&self) -> usize {
        self.inner.open_start()
    }

    /// The open depth at the end.
    #[wasm_bindgen(getter, js_name = openEnd)]
    pub fn open_end(&self) -> usize {
        self.inner.open_end()
    }

    /// The size of this slice.
    #[wasm_bindgen(getter)]
    pub fn size(&self) -> usize {
        self.inner.size()
    }

    /// True if this slice is equal to another.
    pub fn eq(&self, other: &Slice) -> bool {
        self.inner.eq(&other.inner)
    }

    /// JSON representation of this slice.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.to_json();
        value_to_js(&val)
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// A node in a ProseMirror document tree.
#[wasm_bindgen]
pub struct Node {
    pub(crate) inner: BNode,
}

#[wasm_bindgen]
impl Node {
    /// The node type.
    #[wasm_bindgen(getter)]
    pub fn type_(&self) -> NodeType {
        NodeType {
            inner: self.inner.type_(),
        }
    }

    /// The node's attributes as a JSON object.
    #[wasm_bindgen(getter)]
    pub fn attrs(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.attrs_json();
        value_to_js(&val)
    }

    /// The content fragment of this node.
    #[wasm_bindgen(getter)]
    pub fn content(&self) -> Option<Fragment> {
        self.inner.content().map(|f| Fragment { inner: f })
    }

    /// The marks on this node, as an array of Mark objects.
    #[wasm_bindgen(getter)]
    pub fn marks(&self) -> Vec<Mark> {
        let marks = self.inner.marks_vec();
        marks_to_js_array(&self.inner.schema, &marks)
    }

    /// The text content if this is a text node.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> Option<String> {
        self.inner.text()
    }

    /// The concatenated text of all text node descendants.
    #[wasm_bindgen(getter, js_name = textContent)]
    pub fn text_content(&self) -> String {
        self.inner.text_content()
    }

    /// The size of this node (including start/end tokens).
    #[wasm_bindgen(getter, js_name = nodeSize)]
    pub fn node_size(&self) -> usize {
        self.inner.node_size()
    }

    /// Number of child nodes.
    #[wasm_bindgen(getter, js_name = childCount)]
    pub fn child_count(&self) -> usize {
        self.inner.child_count()
    }

    /// True if this is a text node.
    #[wasm_bindgen(getter, js_name = isText)]
    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    /// True if this is a block node.
    #[wasm_bindgen(getter, js_name = isBlock)]
    pub fn is_block(&self) -> bool {
        self.inner.is_block()
    }

    /// True if this is an inline node.
    #[wasm_bindgen(getter, js_name = isInline)]
    pub fn is_inline(&self) -> bool {
        self.inner.is_inline()
    }

    /// True if this is a leaf node.
    #[wasm_bindgen(getter, js_name = isLeaf)]
    pub fn is_leaf(&self) -> bool {
        self.inner.is_leaf()
    }

    /// True if this is a textblock node.
    #[wasm_bindgen(getter, js_name = isTextblock)]
    pub fn is_textblock(&self) -> bool {
        self.inner.is_textblock()
    }

    /// True if this is an atom node.
    #[wasm_bindgen(getter, js_name = isAtom)]
    pub fn is_atom(&self) -> bool {
        self.inner.is_atom()
    }

    /// True if this node allows inline content.
    #[wasm_bindgen(getter, js_name = inlineContent)]
    pub fn inline_content(&self) -> bool {
        self.inner.inline_content()
    }

    /// The first child, or null.
    #[wasm_bindgen(getter, js_name = firstChild)]
    pub fn first_child(&self) -> Option<Node> {
        self.inner.first_child().map(|n| Node { inner: n })
    }

    /// The last child, or null.
    #[wasm_bindgen(getter, js_name = lastChild)]
    pub fn last_child(&self) -> Option<Node> {
        self.inner.last_child().map(|n| Node { inner: n })
    }

    /// Get the child at the given index.
    pub fn child(&self, index: usize) -> Option<Node> {
        self.inner.child(index).map(|n| Node { inner: n })
    }

    /// Like `child`, but returns null if index is out of range.
    #[wasm_bindgen(js_name = maybeChild)]
    pub fn maybe_child(&self, index: usize) -> Option<Node> {
        self.inner.maybe_child(index).map(|n| Node { inner: n })
    }

    /// True if this node has the same markup as another (same type, attrs, marks).
    #[wasm_bindgen(js_name = sameMarkup)]
    pub fn same_markup(&self, other: &Node) -> bool {
        self.inner.same_markup(&other.inner)
    }

    /// True if a mark of the given type exists in the range.
    #[wasm_bindgen(js_name = rangeHasMark)]
    pub fn range_has_mark(&self, from: usize, to: usize, mark_type: &MarkType) -> bool {
        self.inner.range_has_mark(from, to, mark_type.inner.inner)
    }

    /// True if the given node can be appended to this one.
    #[wasm_bindgen(js_name = canAppend)]
    pub fn can_append(&self, other: &Node) -> bool {
        self.inner.can_append(&other.inner)
    }

    /// Get the ContentMatch at the given child index.
    #[wasm_bindgen(js_name = contentMatchAt)]
    pub fn content_match_at(&self, index: usize) -> Result<ContentMatch, JsValue> {
        let inner = self
            .inner
            .content_match_at(index)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(ContentMatch { inner })
    }

    /// Get the text content between two positions.
    #[wasm_bindgen(js_name = textBetween)]
    pub fn text_between(
        &self,
        from: usize,
        to: usize,
        block_sep: JsValue,
        leaf_text: JsValue,
    ) -> String {
        let block_sep_opt = js_to_opt_str(&block_sep);
        let leaf_text_opt = js_to_opt_str(&leaf_text);
        self.inner
            .text_between(from, to, block_sep_opt.as_deref(), leaf_text_opt.as_deref())
    }

    /// Call a function for each direct child node.
    ///
    /// The callback receives `(node, offset, index)`.
    #[wasm_bindgen(js_name = forEach)]
    pub fn for_each(&self, f: &Function) -> Result<(), JsValue> {
        let this = JsValue::null();
        let mut should_stop = false;
        self.inner
            .for_each(&mut |node: &DynamicNode, offset, index| {
                if should_stop {
                    return;
                }
                let node_js = Node {
                    inner: BNode {
                        schema: self.inner.schema.clone(),
                        inner: node.clone(),
                    },
                };
                let result = f.call3(
                    &this,
                    &JsValue::from(node_js),
                    &JsValue::from_f64(offset as f64),
                    &JsValue::from_f64(index as f64),
                );
                match result {
                    Ok(r) => {
                        if let Some(b) = r.as_bool() {
                            if !b {
                                should_stop = true;
                            }
                        }
                    }
                    Err(_) => {
                        should_stop = true;
                    }
                }
            });
        Ok(())
    }

    /// Call a function for all nodes between two positions.
    ///
    /// The callback receives `(node, pos, parent, index)`. Return false to stop.
    #[wasm_bindgen(js_name = nodesBetween)]
    pub fn nodes_between(&self, from: usize, to: usize, f: &Function) -> Result<(), JsValue> {
        let this = JsValue::null();
        self.inner
            .nodes_between(from, to, &mut |node: &DynamicNode, pos, parent, index| {
                let node_js = Node {
                    inner: BNode {
                        schema: self.inner.schema.clone(),
                        inner: node.clone(),
                    },
                };
                let parent_js = parent
                    .map(|p| {
                        JsValue::from(Node {
                            inner: BNode {
                                schema: self.inner.schema.clone(),
                                inner: p.clone(),
                            },
                        })
                    })
                    .unwrap_or_else(JsValue::null);
                let result = f.call4(
                    &this,
                    &JsValue::from(node_js),
                    &JsValue::from_f64(pos as f64),
                    &parent_js,
                    &JsValue::from_f64(index as f64),
                );
                match result {
                    Ok(r) => r.as_bool().unwrap_or(true),
                    Err(_) => false,
                }
            });
        Ok(())
    }

    /// Call a function for all descendant nodes recursively.
    ///
    /// The callback receives `(node, pos, parent, index)`. Return false to stop.
    pub fn descendants(&self, f: &Function) -> Result<(), JsValue> {
        let this = JsValue::null();
        self.inner
            .descendants(&mut |node: &DynamicNode, pos, parent, index| {
                let node_js = Node {
                    inner: BNode {
                        schema: self.inner.schema.clone(),
                        inner: node.clone(),
                    },
                };
                let parent_js = parent
                    .map(|p| {
                        JsValue::from(Node {
                            inner: BNode {
                                schema: self.inner.schema.clone(),
                                inner: p.clone(),
                            },
                        })
                    })
                    .unwrap_or_else(JsValue::null);
                let result = f.call4(
                    &this,
                    &JsValue::from(node_js),
                    &JsValue::from_f64(pos as f64),
                    &parent_js,
                    &JsValue::from_f64(index as f64),
                );
                match result {
                    Ok(r) => r.as_bool().unwrap_or(true),
                    Err(_) => false,
                }
            });
        Ok(())
    }

    /// Cut out a slice from this node.
    /// Note: wasm-bindgen JS wrapper strips the 3rd bool param from the JS
    /// signature — use slice_with_parents for include_parents=true.
    pub fn slice(&self, from: usize, to: usize) -> Slice {
        let inner = self.inner.slice(from, to, false);
        Slice { inner }
    }

    /// Cut out a slice including parent nodes.
    #[wasm_bindgen(js_name = sliceWithParents)]
    pub fn slice_with_parents(&self, from: usize, to: usize) -> Slice {
        let inner = self.inner.slice(from, to, true);
        Slice { inner }
    }

    /// Cut out a portion of this node, returning a new node.
    pub fn cut(&self, from: usize, to: usize) -> Node {
        Node {
            inner: self.inner.cut(from, to),
        }
    }

    /// Replace content in this node.
    pub fn replace(&self, from: usize, to: usize, slice: &Slice) -> Result<Node, JsValue> {
        let inner = self
            .inner
            .replace(from, to, &slice.inner)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Node { inner })
    }

    /// Resolve a position in this node.
    pub fn resolve(&self, pos: usize) -> ResolvedPos {
        ResolvedPos {
            inner: self.inner.resolve(pos),
        }
    }

    /// Resolve a position without using a cache (same as `resolve` in this implementation).
    #[wasm_bindgen(js_name = resolveNoCache)]
    pub fn resolve_no_cache(&self, pos: usize) -> ResolvedPos {
        self.resolve(pos)
    }

    /// True if this node is equal to another.
    pub fn eq(&self, other: &Node) -> bool {
        self.inner.eq(&other.inner)
    }

    /// JSON representation of this node.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        let val = self.inner.to_json(false);
        value_to_js(&val)
    }

    /// Debug string representation.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        self.inner.to_debug_string()
    }

    /// Apply marks to this node.
    pub fn mark(&self, marks: Vec<Mark>) -> Node {
        let dyn_marks: Vec<DynamicMark> = marks.iter().map(|m| m.inner.inner.clone()).collect();
        let mark_set = MarkSet::from_vec(dyn_marks);
        Node {
            inner: self.inner.mark(mark_set),
        }
    }

    /// Create a copy of this node with new content.
    pub fn copy(&self, content: &Fragment) -> Node {
        Node {
            inner: self.inner.copy(content.inner.inner.clone()),
        }
    }

    /// Check if this node conforms to the schema.
    pub fn check(&self) -> Result<(), JsValue> {
        self.inner.check().map_err(|e| JsValue::from_str(&e))
    }

    /// Get the node at the given position, if any.
    #[wasm_bindgen(js_name = nodeAt)]
    pub fn node_at(&self, pos: usize) -> Option<Node> {
        self.inner.node_at(pos).map(|n| Node { inner: n })
    }

    /// Test whether the node matches given type, attrs, and marks.
    ///
    /// `marks` can be null (don't check marks), an empty array (check for no marks),
    /// or an array of Mark objects.
    #[wasm_bindgen(js_name = hasMarkup)]
    pub fn has_markup(
        &self,
        type_: &NodeType,
        attrs: JsValue,
        marks: Option<Vec<Mark>>,
    ) -> Result<bool, JsValue> {
        let attrs_opt = if attrs.is_null() || attrs.is_undefined() {
            None
        } else {
            Some(js_to_value(&attrs)?)
        };

        let marks_opt: Option<Vec<DynamicMark>> =
            marks.map(|m| m.iter().map(|mark| mark.inner.inner.clone()).collect());

        Ok(self
            .inner
            .has_markup(&type_.inner, attrs_opt.as_ref(), marks_opt.as_deref()))
    }

    /// Returns `{node, index, offset}` for the child immediately after `pos`,
    /// or null.
    #[wasm_bindgen(js_name = childAfter)]
    pub fn child_after(&self, pos: usize) -> Option<Object> {
        build_child_result(&self.inner.schema, self.inner.child_after(pos))
    }

    /// Returns `{node, index, offset}` for the child immediately before `pos`,
    /// or null.
    #[wasm_bindgen(js_name = childBefore)]
    pub fn child_before(&self, pos: usize) -> Option<Object> {
        build_child_result(&self.inner.schema, self.inner.child_before(pos))
    }

    /// Check whether the given fragment can replace the content at `from..to`.
    #[wasm_bindgen(js_name = canReplace)]
    pub fn can_replace(
        &self,
        from: usize,
        to: usize,
        replacement: Option<Fragment>,
        start: usize,
        end: Option<usize>,
    ) -> bool {
        let replacement_opt: Option<BFragment> = replacement.map(|f| f.inner.clone());
        let end_opt = end.unwrap_or(0);
        self.inner
            .can_replace(from, to, replacement_opt.as_ref(), start, Some(end_opt))
    }

    /// Check whether a node of the given type can replace the content at `from..to`.
    #[wasm_bindgen(js_name = canReplaceWith)]
    pub fn can_replace_with(&self, from: usize, to: usize, type_: &NodeType) -> bool {
        self.inner.can_replace_with(from, to, &type_.inner)
    }
}

// ---------------------------------------------------------------------------
// ResolvedPos
// ---------------------------------------------------------------------------

/// A resolved position in a document.
#[wasm_bindgen]
pub struct ResolvedPos {
    pub(crate) inner: BResolvedPos,
}

#[wasm_bindgen]
impl ResolvedPos {
    /// The absolute position.
    #[wasm_bindgen(getter)]
    pub fn pos(&self) -> usize {
        self.inner.pos
    }

    /// The depth of this position.
    #[wasm_bindgen(getter)]
    pub fn depth(&self) -> usize {
        self.inner.depth()
    }

    /// The parent node containing this position.
    #[wasm_bindgen(getter)]
    pub fn parent(&self) -> Node {
        Node {
            inner: self.inner.parent(),
        }
    }

    /// The document node.
    #[wasm_bindgen(getter)]
    pub fn doc(&self) -> Node {
        Node {
            inner: self.inner.doc_node(),
        }
    }

    /// The offset of this position into its parent text node.
    #[wasm_bindgen(getter, js_name = parentOffset)]
    pub fn parent_offset(&self) -> usize {
        self.inner.parent_offset()
    }

    /// The offset into the parent text node.
    #[wasm_bindgen(getter, js_name = textOffset)]
    pub fn text_offset(&self) -> usize {
        self.inner.text_offset()
    }

    /// The node immediately before this position, if any.
    #[wasm_bindgen(getter, js_name = nodeBefore)]
    pub fn node_before(&self) -> Option<Node> {
        self.inner.node_before().map(|n| Node { inner: n })
    }

    /// The node immediately after this position, if any.
    #[wasm_bindgen(getter, js_name = nodeAfter)]
    pub fn node_after(&self) -> Option<Node> {
        self.inner.node_after().map(|n| Node { inner: n })
    }

    /// Get the ancestor node at the given depth.
    pub fn node(&self, depth: Option<usize>) -> Node {
        Node {
            inner: self.inner.node(depth),
        }
    }

    /// The index into the ancestor at the given depth.
    pub fn index(&self, depth: Option<usize>) -> usize {
        self.inner.index(depth)
    }

    /// The index after the ancestor at the given depth.
    #[wasm_bindgen(js_name = indexAfter)]
    pub fn index_after(&self, depth: Option<usize>) -> usize {
        self.inner.index_after(depth)
    }

    /// The start position of the ancestor at the given depth.
    pub fn start(&self, depth: Option<usize>) -> usize {
        self.inner.start(depth)
    }

    /// The end position of the ancestor at the given depth.
    pub fn end(&self, depth: Option<usize>) -> usize {
        self.inner.end(depth)
    }

    /// The position before the ancestor at the given depth.
    pub fn before(&self, depth: Option<usize>) -> Option<usize> {
        self.inner.before(depth)
    }

    /// The position after the ancestor at the given depth.
    pub fn after(&self, depth: Option<usize>) -> Option<usize> {
        self.inner.after(depth)
    }

    /// The shared depth between this position and another.
    #[wasm_bindgen(js_name = sharedDepth)]
    pub fn shared_depth(&self, pos: usize) -> usize {
        self.inner.shared_depth(pos)
    }

    /// The marks at this position.
    pub fn marks(&self) -> Vec<Mark> {
        let marks = self.inner.marks();
        marks_to_js_array(&self.inner.schema, &marks)
    }

    /// The marks across this position and another.
    #[wasm_bindgen(js_name = marksAcross)]
    pub fn marks_across(&self, end: &ResolvedPos) -> Option<Vec<Mark>> {
        self.inner
            .marks_across(&end.inner)
            .map(|m| marks_to_js_array(&self.inner.schema, &m))
    }

    /// True if this position shares the same parent with another.
    #[wasm_bindgen(js_name = sameParent)]
    pub fn same_parent(&self, other: &ResolvedPos) -> bool {
        self.inner.same_parent(&other.inner)
    }

    /// Return the greater of this and another resolved position.
    pub fn max(&self, other: &ResolvedPos) -> ResolvedPos {
        ResolvedPos {
            inner: self.inner.max(&other.inner),
        }
    }

    /// Return the lesser of this and another resolved position.
    pub fn min(&self, other: &ResolvedPos) -> ResolvedPos {
        ResolvedPos {
            inner: self.inner.min(&other.inner),
        }
    }

    /// The position at the given index in the ancestor at the given depth.
    #[wasm_bindgen(js_name = posAtIndex)]
    pub fn pos_at_index(&self, index: usize, depth: Option<usize>) -> usize {
        self.inner.pos_at_index(index, depth)
    }

    /// The block range around this position, optionally extended to another.
    #[wasm_bindgen(js_name = blockRange)]
    pub fn block_range(&self, other: &ResolvedPos) -> Option<NodeRange> {
        self.inner
            .block_range(Some(&other.inner))
            .map(|nr| NodeRange { inner: nr })
    }
}

// ---------------------------------------------------------------------------
// NodeRange
// ---------------------------------------------------------------------------

/// A range across a single node in the document.
#[wasm_bindgen]
pub struct NodeRange {
    pub(crate) inner: BNodeRange,
}

#[wasm_bindgen]
impl NodeRange {
    /// The resolved position at the start of the range.
    #[wasm_bindgen(getter, js_name = "$from")]
    pub fn from_resolved(&self) -> ResolvedPos {
        ResolvedPos {
            inner: self.inner.from_resolved_pos(),
        }
    }

    /// The resolved position at the end of the range.
    #[wasm_bindgen(getter, js_name = "$to")]
    pub fn to_resolved(&self) -> ResolvedPos {
        ResolvedPos {
            inner: self.inner.to_resolved_pos(),
        }
    }

    /// The start position of the range.
    #[wasm_bindgen(getter)]
    pub fn from(&self) -> usize {
        self.inner.from_pos
    }

    /// The end position of the range.
    #[wasm_bindgen(getter)]
    pub fn to(&self) -> usize {
        self.inner.to_pos
    }

    /// The depth of the node that the range points into.
    #[wasm_bindgen(getter)]
    pub fn depth(&self) -> usize {
        self.inner.depth()
    }

    /// The start position of the range's node.
    #[wasm_bindgen(getter)]
    pub fn start(&self) -> usize {
        self.inner.start()
    }

    /// The end position of the range's node.
    #[wasm_bindgen(getter)]
    pub fn end(&self) -> usize {
        self.inner.end()
    }

    /// The parent node.
    #[wasm_bindgen(getter)]
    pub fn parent(&self) -> Node {
        Node {
            inner: self.inner.parent(),
        }
    }

    /// The start index of the range's node in its parent.
    #[wasm_bindgen(getter, js_name = startIndex)]
    pub fn start_index(&self) -> usize {
        self.inner.start_index()
    }

    /// The end index of the range's node in its parent.
    #[wasm_bindgen(getter, js_name = endIndex)]
    pub fn end_index(&self) -> usize {
        self.inner.end_index()
    }
}

// ---------------------------------------------------------------------------
// ContentMatch
// ---------------------------------------------------------------------------

/// A content match represents a state in the content expression DFA.
#[wasm_bindgen]
pub struct ContentMatch {
    pub(crate) inner: BContentMatch,
}

#[wasm_bindgen]
impl ContentMatch {
    /// True if this match state represents a valid end.
    #[wasm_bindgen(getter, js_name = validEnd)]
    pub fn valid_end(&self) -> bool {
        self.inner.valid_end()
    }

    /// Match a node type against this content expression, returning the next state.
    #[wasm_bindgen(js_name = matchType)]
    pub fn match_type(&self, type_: &NodeType) -> Option<ContentMatch> {
        self.inner
            .match_type(&type_.inner)
            .map(|cm| ContentMatch { inner: cm })
    }

    /// Match a fragment against this content expression.
    #[wasm_bindgen(js_name = matchFragment)]
    pub fn match_fragment(&self, frag: &Fragment) -> Option<ContentMatch> {
        self.inner
            .match_fragment(&frag.inner, 0, None)
            .map(|cm| ContentMatch { inner: cm })
    }

    /// Fill this content expression with default nodes before the given fragment.
    #[wasm_bindgen(js_name = fillBefore)]
    pub fn fill_before(
        &self,
        after: &Fragment,
        to_end: bool,
        start_index: usize,
    ) -> Option<Fragment> {
        self.inner
            .fill_before(&after.inner, to_end, start_index)
            .map(|f| Fragment { inner: f })
    }

    /// Get the default node type for this content match, if any.
    #[wasm_bindgen(js_name = defaultType)]
    pub fn default_type(&self) -> Option<NodeType> {
        self.inner.default_type().map(|nt| NodeType { inner: nt })
    }

    /// Find a wrapping of the given node type.
    #[wasm_bindgen(js_name = findWrapping)]
    pub fn find_wrapping(&self, target: &NodeType) -> Option<Array> {
        self.inner.find_wrapping(&target.inner).map(|types| {
            let arr = Array::new();
            for nt in types {
                arr.push(&JsValue::from(NodeType { inner: nt }));
            }
            arr
        })
    }

    /// Number of outgoing edges from this state.
    #[wasm_bindgen(js_name = edgeCount)]
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// The node type for the nth outgoing edge.
    #[wasm_bindgen(js_name = edgeType)]
    pub fn edge_type(&self, n: usize) -> Option<NodeType> {
        self.inner.edge(n).map(|(nt, _cm)| NodeType { inner: nt })
    }

    /// The next ContentMatch state for the nth outgoing edge.
    #[wasm_bindgen(js_name = edgeMatch)]
    pub fn edge_match(&self, n: usize) -> Option<ContentMatch> {
        self.inner
            .edge(n)
            .map(|(_nt, cm)| ContentMatch { inner: cm })
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Parse a content expression string into a ContentMatch.
///
/// `node_types` should be an object where each key is a node type name
/// and each value is either a plain object with a `group` property (e.g.
/// `{group: "block"}`) or a NodeType instance with a `spec()` method.
#[wasm_bindgen(js_name = contentMatchParse)]
pub fn content_match_parse(expr: &str, node_types: &Object) -> Result<ContentMatch, JsValue> {
    // Build a minimal schema from the node_types object.
    // Extract node type names and group info from the values.
    let keys = Object::keys(node_types);
    let mut nodes_map = serde_json::Map::new();

    for key in keys.iter() {
        let key_str: String = key
            .as_string()
            .ok_or_else(|| JsValue::from_str("node_types keys must be strings"))?;

        // Try to extract group info from the value's .group property
        let mut group = String::new();
        if let Ok(val) = Reflect::get(node_types, &key) {
            if let Ok(group_val) = Reflect::get(&val, &JsValue::from_str("group")) {
                if let Some(g) = group_val.as_string() {
                    group = g;
                }
            }
        }

        nodes_map.insert(key_str, serde_json::json!({"content": "", "group": group}));
    }

    let spec = serde_json::json!({
        "nodes": nodes_map,
        "marks": {}
    });

    let schema = DynamicSchema::from_json(&spec).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let arc_schema = Arc::new(schema);

    let inner = BContentMatch::parse(expr, &arc_schema).map_err(|e| JsValue::from_str(&e))?;
    Ok(ContentMatch { inner })
}
