//! Dynamic runtime types for nodes, marks, and their type descriptors.

use crate::dynamic::content_expr::ContentExpr;
use crate::model::{
    ContentMatch, Fragment, Mark, MarkSet, Node, NodeType, Schema, Text, TextNode,
};
use crate::model::MarkType;
use serde::{Deserialize, Deserializer, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::RangeBounds;

thread_local! {
    pub(crate) static DYN_TYPES: RefCell<Option<&'static DynTypeStore>> = const { RefCell::new(None) };
    static DYN_NODE_TYPE_NAMES: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
}

// The generic NodeType trait currently exposes names as &'static str. Dynamic
// schemas own names as Strings, so intern the small set of observed node type
// names to provide stable borrowed names without changing the public trait.
fn intern_node_type_name(name: &str) -> &'static str {
    DYN_NODE_TYPE_NAMES.with(|cell| {
        let mut names = cell.borrow_mut();
        if let Some(interned) = names.get(name).copied() {
            interned
        } else {
            let interned: &'static str = Box::leak(name.to_owned().into_boxed_str());
            names.insert(interned.to_owned(), interned);
            interned
        }
    })
}

/// Stores all type information for a dynamic schema.
pub struct DynTypeStore {
    pub(crate) node_types: Vec<DynamicNodeTypeData>,
    pub(crate) mark_types: Vec<DynamicMarkTypeData>,
    pub(crate) content_exprs: Vec<ContentExpr>,
}

/// Runtime data for a dynamic node type.
#[derive(Debug, Clone)]
pub struct DynamicNodeTypeData {
    /// The name of this node type
    pub name: String,
    /// Whether this is an inline type
    pub inline: bool,
    /// Whether this is an atom (leaf) type
    pub atom: bool,
    /// Whether this is a textblock
    pub textblock: bool,
    /// Whether this has inline content
    pub has_inline_content: bool,
    /// Index into the content_exprs array
    pub content_expr_idx: usize,
    /// Groups this type belongs to
    pub groups: Vec<String>,
    /// Attribute names with their defaults
    pub attrs: HashMap<String, serde_json::Value>,
    /// The set of allowed mark type names (None = all allowed)
    pub allowed_marks: Option<Vec<String>>,
}

/// Runtime data for a dynamic mark type.
#[derive(Debug, Clone)]
pub struct DynamicMarkTypeData {
    /// The name of this mark type
    pub name: String,
    /// Attribute names with their defaults
    pub attrs: HashMap<String, serde_json::Value>,
    /// Whether this mark is inclusive
    pub inclusive: bool,
    /// Which other mark types this one excludes
    pub excludes: Vec<String>,
    /// Groups this mark belongs to
    pub groups: Vec<String>,
}

/// A lightweight, Copy handle to a dynamic node type.
#[derive(Debug, Clone, Copy)]
pub struct DynamicNodeType {
    /// Index into the schema's node_types array
    pub idx: usize,
}

impl PartialEq for DynamicNodeType {
    fn eq(&self, other: &Self) -> bool { self.idx == other.idx }
}
impl Eq for DynamicNodeType {}
impl PartialOrd for DynamicNodeType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for DynamicNodeType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.idx.cmp(&other.idx) }
}
impl Hash for DynamicNodeType {
    fn hash<H: Hasher>(&self, state: &mut H) { self.idx.hash(state); }
}
impl fmt::Display for DynamicNodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "NodeType({})", self.idx) }
}

/// A lightweight, Copy handle to a dynamic mark type.
#[derive(Debug, Clone, Copy)]
pub struct DynamicMarkType {
    /// Index into the schema's mark_types array
    pub idx: usize,
}

impl PartialEq for DynamicMarkType {
    fn eq(&self, other: &Self) -> bool { self.idx == other.idx }
}
impl Eq for DynamicMarkType {}
impl PartialOrd for DynamicMarkType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for DynamicMarkType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering { self.idx.cmp(&other.idx) }
}
impl Hash for DynamicMarkType {
    fn hash<H: Hasher>(&self, state: &mut H) { self.idx.hash(state); }
}
impl MarkType for DynamicMarkType {}

/// A content match backed by a DFA, using an index into the schema's expr array.
#[derive(Debug, Clone, Copy)]
pub struct DynamicContentMatch {
    /// Index into the schema's content_exprs array
    pub expr_idx: usize,
    /// The current DFA state index
    pub state: usize,
}

impl PartialEq for DynamicContentMatch {
    fn eq(&self, other: &Self) -> bool { self.expr_idx == other.expr_idx && self.state == other.state }
}
impl Eq for DynamicContentMatch {}

impl ContentMatch<Dyn> for DynamicContentMatch {
    fn match_fragment_range<R: RangeBounds<usize>>(self, fragment: &Fragment<Dyn>, range: R) -> Option<Self> {
        use crate::model::util;
        let start = util::from(&range);
        let end = util::to(&range, fragment.child_count());
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let mut state = self.state;
            for child in &fragment.children()[start..end] {
                let name = &store.node_types[child.r#type().idx].name;
                state = expr.match_type(state, name)?;
            }
            Some(DynamicContentMatch { expr_idx: self.expr_idx, state })
        }).flatten()
    }

    fn valid_end(self) -> bool {
        with_types(|store| store.content_exprs[self.expr_idx].valid_end(self.state)).unwrap_or(false)
    }

    fn match_type(self, r#type: DynamicNodeType) -> Option<Self> {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let name = &store.node_types[r#type.idx].name;
            let next = expr.match_type(self.state, name)?;
            Some(DynamicContentMatch { expr_idx: self.expr_idx, state: next })
        }).flatten()
    }

    fn fill_before(self, after: &Fragment<Dyn>, to_end: bool, start_index: usize) -> Option<Fragment<Dyn>> {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let mut state = self.state;
            for i in 0..start_index {
                if let Some(child) = after.maybe_child(i) {
                    let name = &store.node_types[child.r#type().idx].name;
                    state = expr.match_type(state, name)?;
                }
            }
            let mut result = Vec::new();
            fill_before_impl(expr, state, after, start_index, to_end, &mut result, store)?;
            Some(Fragment::from(result))
        }).flatten()
    }

    fn find_wrapping(self, target: DynamicNodeType) -> Option<Vec<DynamicNodeType>> {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let name = &store.node_types[target.idx].name;
            if expr.match_type(self.state, name).is_some() { return Some(Vec::new()); }
            None
        }).flatten()
    }

    fn inline_content(self) -> bool {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            for i in 0..expr.edge_count(self.state) {
                if let Some((name, _)) = expr.edge(self.state, i) {
                    if name == "text" || name == "hard_break" || name == "image" { return true; }
                }
            }
            false
        }).unwrap_or(false)
    }

    fn edge_count(self) -> usize {
        with_types(|store| store.content_exprs[self.expr_idx].edge_count(self.state)).unwrap_or(0)
    }

    fn edge(self, n: usize) -> Option<(DynamicNodeType, Self)> {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let (name, next_state) = expr.edge(self.state, n)?;
            for (i, nt) in store.node_types.iter().enumerate() {
                if nt.name == name {
                    return Some((DynamicNodeType { idx: i }, DynamicContentMatch {
                        expr_idx: self.expr_idx, state: next_state,
                    }));
                }
            }
            None
        }).flatten()
    }
}

fn fill_before_impl(
    expr: &ContentExpr, state: usize, after: &Fragment<Dyn>, index: usize, to_end: bool,
    result: &mut Vec<DynamicNode>, store: &DynTypeStore,
) -> Option<()> {
    if to_end && expr.valid_end(state) { return Some(()); }
    if let Some(child) = after.maybe_child(index) {
        let name = &store.node_types[child.r#type().idx].name;
        let next = expr.match_type(state, name)?;
        result.push(child.clone());
        return fill_before_impl(expr, next, after, index + 1, to_end, result, store);
    }
    if !to_end && expr.valid_end(state) { return Some(()); }
    None
}

/// The dynamic schema type (zero-sized, used as the `S` parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dyn;

impl Schema for Dyn {
    type Node = DynamicNode;
    type Mark = DynamicMark;
    type MarkType = DynamicMarkType;
    type NodeType = DynamicNodeType;
    type ContentMatch = DynamicContentMatch;
}

pub(crate) fn with_types<R>(f: impl FnOnce(&DynTypeStore) -> R) -> Option<R> {
    DYN_TYPES.with(|cell| {
        let borrow = cell.borrow();
        borrow.map(f)
    })
}

/// Run a closure with the type store set, saving/restoring any previous value.
pub fn with_types_scope<R>(store: &'static DynTypeStore, f: impl FnOnce() -> R) -> R {
    DYN_TYPES.with(|cell| {
        let _prev = cell.borrow_mut().replace(store);
        // Note: we don't restore prev here because the closure may call with_types
        // recursively and those calls read from the cell
    });
    let result = f();
    DYN_TYPES.with(|cell| {
        cell.borrow_mut().take();
    });
    result
}

// ---------------------------------------------------------------------------
// DynamicNode — the core node type backed by Fragment<Dyn>
// ---------------------------------------------------------------------------

/// A dynamic node in the document tree.
///
/// Text nodes store a `TextNode<Dyn>`. Element and leaf nodes store a
/// `Fragment<Dyn>` for their children. This is the canonical representation
/// used by all model and transform operations.
#[derive(Debug, Clone)]
pub struct DynamicNode {
    /// The type index into the schema's node_types array
    pub type_idx: usize,
    /// The type name (cached for serialization)
    pub type_name: String,
    /// Attributes as a JSON value
    pub attrs: serde_json::Value,
    /// Marks on this node
    pub marks: MarkSet<Dyn>,
    inner: DynNodeInner,
}

#[derive(Debug, Clone)]
enum DynNodeInner {
    /// An element or leaf node with optional content
    Element { content: Fragment<Dyn> },
    /// A text node
    Text(TextNode<Dyn>),
}

impl DynamicNode {
    /// Recalculate from children — not needed when using Fragment, but
    /// kept for compatibility with `DynamicSchema::node_from_json`.
    pub fn recalc(&mut self, type_idx: usize) {
        self.type_idx = type_idx;
    }

    /// Serialize this node to the standard ProseMirror JSON format.
    ///
    /// When `skip_defaults` is `true`, attributes whose value matches the
    /// schema-defined default are omitted, matching ProseMirror's internal
    /// `toJSON` behaviour for minimised output.
    pub fn to_json(&self, skip_defaults: bool) -> serde_json::Value {
        if skip_defaults {
            self.to_mini_json()
        } else {
            // Use the serde Serialize impl
            serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
        }
    }

    /// ProseMirror-style "mini" JSON: omit attributes that match their
    /// schema-level defaults.
    ///
    /// Adapted from
    /// https://github.com/ProseMirror/prosemirror-model/blob/6d970507cd0da48653d3b72f2731a71a144a364b/src/node.js#L340-L351
    fn to_mini_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({ "type": self.type_name });

        let attrs = if self.attrs.is_object() {
            let map = self.attrs.as_object().unwrap();
            // Query the schema for the default attribute values of this node type
            let default_attrs = with_types(|store| {
                    store.node_types.get(self.type_idx).map(|nt| nt.attrs.clone())
                }).flatten().unwrap_or_default();

            let mut non_default = serde_json::Map::new();
            for (k, v) in map {
                let is_default = default_attrs
                    .get(k)
                    .map(|default| default == v)
                    .unwrap_or(false);
                if !is_default {
                    non_default.insert(k.clone(), v.clone());
                }
            }
            if non_default.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(non_default))
            }
        } else {
            None
        };

        if let Some(attrs_val) = attrs {
            obj.as_object_mut()
                .unwrap()
                .insert("attrs".to_string(), attrs_val);
        }

        // Content
        match &self.inner {
            DynNodeInner::Text(tn) => {
                obj.as_object_mut()
                    .unwrap()
                    .insert("text".to_string(), serde_json::Value::String(tn.text.as_str().to_string()));
            }
            DynNodeInner::Element { content } => {
                if content.child_count() > 0 {
                    let children: Vec<serde_json::Value> = content
                        .children()
                        .iter()
                        .map(|child| child.to_mini_json())
                        .collect();
                    obj.as_object_mut()
                        .unwrap()
                        .insert("content".to_string(), serde_json::Value::Array(children));
                }
            }
        }

        // Marks
        let mini_marks: Vec<serde_json::Value> = self
            .marks
            .iter()
            .map(|m| m.to_mini_json())
            .collect();
        if !mini_marks.is_empty() {
            obj.as_object_mut()
                .unwrap()
                .insert("marks".to_string(), serde_json::Value::Array(mini_marks));
        }

        obj
    }
}

impl PartialEq for DynamicNode {
    fn eq(&self, other: &Self) -> bool {
        self.type_idx == other.type_idx
            && self.attrs == other.attrs
            && self.marks == other.marks
            && match (&self.inner, &other.inner) {
                (DynNodeInner::Element { content: a }, DynNodeInner::Element { content: b }) => a == b,
                (DynNodeInner::Text(a), DynNodeInner::Text(b)) => a.text == b.text,
                _ => false,
            }
    }
}
impl Eq for DynamicNode {}

// ---------------------------------------------------------------------------
// Serde: DynamicNode serializes to/from the standard ProseMirror JSON format
// ---------------------------------------------------------------------------

/// Intermediate serde representation
#[derive(Serialize, Deserialize)]
struct DynamicNodeHelper {
    #[serde(rename = "type")]
    type_name: String,
    #[serde(default, skip_serializing_if = "is_default_attrs")]
    attrs: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    content: Vec<DynamicNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    marks: Vec<DynamicMark>,
}

fn is_default_attrs(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Object(m) => m.is_empty(),
        serde_json::Value::Null => true,
        _ => false,
    }
}

impl Serialize for DynamicNode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let marks_vec: Vec<DynamicMark> = self.marks.iter().cloned().collect();
        match &self.inner {
            DynNodeInner::Text(tn) => {
                let helper = DynamicNodeHelper {
                    type_name: self.type_name.clone(),
                    attrs: self.attrs.clone(),
                    content: Vec::new(),
                    text: Some(tn.text.as_str().to_string()),
                    marks: marks_vec,
                };
                helper.serialize(serializer)
            }
            DynNodeInner::Element { content } => {
                let helper = DynamicNodeHelper {
                    type_name: self.type_name.clone(),
                    attrs: self.attrs.clone(),
                    content: content.children().to_vec(),
                    text: None,
                    marks: marks_vec,
                };
                helper.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for DynamicNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let helper = DynamicNodeHelper::deserialize(deserializer)?;
        let type_idx = with_types(|store| {
            for (i, nt) in store.node_types.iter().enumerate() {
                if nt.name == helper.type_name { return Some(i); }
            }
            None
        }).flatten().unwrap_or(0);

        let marks_vec = helper.marks;
        let mut marks_set = MarkSet::new();
        for m in &marks_vec {
            marks_set.add(m);
        }

        if let Some(text) = helper.text {
            let text_obj = Text::from(text);
            Ok(DynamicNode {
                type_idx,
                type_name: helper.type_name,
                attrs: helper.attrs,
                marks: marks_set.clone(),
                inner: DynNodeInner::Text(TextNode { text: text_obj, marks: marks_set }),
            })
        } else {
            let frag = Fragment::from(helper.content);
            Ok(DynamicNode {
                type_idx,
                type_name: helper.type_name,
                attrs: helper.attrs,
                marks: marks_set,
                inner: DynNodeInner::Element { content: frag },
            })
        }
    }
}

// ---------------------------------------------------------------------------
// DynamicMark
// ---------------------------------------------------------------------------

/// A dynamic mark value.
#[derive(Debug, Clone, Serialize)]
pub struct DynamicMark {
    /// The mark type name
    #[serde(rename = "type")]
    pub type_name: String,
    /// Attributes — omitted when null or empty so serialization matches ProseMirror JS output.
    #[serde(skip_serializing_if = "is_default_attrs")]
    pub attrs: serde_json::Value,
}

#[derive(Deserialize)]
struct DynamicMarkHelper {
    #[serde(rename = "type")]
    type_name: String,
    #[serde(default)]
    attrs: serde_json::Value,
}

impl<'de> Deserialize<'de> for DynamicMark {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let helper = DynamicMarkHelper::deserialize(deserializer)?;
        if let Some(known) = with_types(|store| {
            store
                .mark_types
                .iter()
                .any(|mark_type| mark_type.name == helper.type_name)
        }) {
            if !known {
                return Err(serde::de::Error::custom(format!(
                    "Unknown mark type: {}",
                    helper.type_name
                )));
            }
        }

        Ok(DynamicMark {
            type_name: helper.type_name,
            attrs: helper.attrs,
        })
    }
}

impl DynamicMark {
    /// ProseMirror-style "mini" JSON for marks: omit attributes that
    /// match their schema-level defaults.
    ///
    /// Adapted from
    /// https://github.com/ProseMirror/prosemirror-model/blob/6d970507cd0da48653d3b72f2731a71a144a364b/src/mark.js#L76-L83
    fn to_mini_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({ "type": self.type_name });

        if let serde_json::Value::Object(map) = &self.attrs {
            let default_attrs = with_types(|store| {
                    store.mark_types.iter().find(|mt| mt.name == self.type_name).map(|mt| mt.attrs.clone())
                }).flatten().unwrap_or_default();

            let mut non_default = serde_json::Map::new();
            for (k, v) in map {
                let is_default = default_attrs
                    .get(k)
                    .map(|default| default == v)
                    .unwrap_or(false);
                if !is_default {
                    non_default.insert(k.clone(), v.clone());
                }
            }
            if !non_default.is_empty() {
                obj.as_object_mut()
                    .unwrap()
                    .insert("attrs".to_string(), serde_json::Value::Object(non_default));
            }
        }

        obj
    }
}

impl PartialEq for DynamicMark {
    fn eq(&self, other: &Self) -> bool { self.type_name == other.type_name && self.attrs == other.attrs }
}
impl Eq for DynamicMark {}
impl Hash for DynamicMark {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_name.hash(state);
        self.attrs.hash(state);
    }
}

// ---------------------------------------------------------------------------
// NodeType impl
// ---------------------------------------------------------------------------

impl NodeType<Dyn> for DynamicNodeType {
    fn compatible_content(self, other: Self) -> bool { self.idx == other.idx }

    fn valid_content(self, fragment: &Fragment<Dyn>) -> bool {
        with_types(|store| {
            let expr = &store.content_exprs[store.node_types[self.idx].content_expr_idx];
            let mut state = 0;
            for child in fragment.children() {
                match expr.match_type(state, &store.node_types[child.r#type().idx].name) {
                    Some(next) => state = next,
                    None => return false,
                }
            }
            expr.valid_end(state)
        }).unwrap_or(false)
    }

    fn allows_mark_type(self, mark_type: DynamicMarkType) -> bool {
        with_types(|store| {
            let nt = &store.node_types[self.idx];
            match &nt.allowed_marks {
                Some(allowed) => allowed.contains(&store.mark_types[mark_type.idx].name),
                None => true,
            }
        }).unwrap_or(true)
    }

    fn content_match(self) -> DynamicContentMatch {
        with_types(|store| DynamicContentMatch {
            expr_idx: store.node_types[self.idx].content_expr_idx, state: 0,
        }).unwrap_or(DynamicContentMatch { expr_idx: 0, state: 0 })
    }

    fn allow_marks(self, marks: &MarkSet<Dyn>) -> bool {
        with_types(|store| {
            let nt = &store.node_types[self.idx];
            match &nt.allowed_marks {
                Some(allowed) => {
                    for mark in marks {
                        if !allowed.contains(&store.mark_types[mark.r#type().idx].name) { return false; }
                    }
                    true
                }
                None => true,
            }
        }).unwrap_or(true)
    }

    fn is_block(self) -> bool {
        with_types(|store| !store.node_types[self.idx].inline).unwrap_or(true)
    }
    fn name(self) -> &'static str {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .map_or("", |node_type| intern_node_type_name(&node_type.name))
        })
        .unwrap_or("")
    }
    fn is_atom(self) -> bool {
        with_types(|store| {
            store.node_types.get(self.idx).is_some_and(|node_type| {
                node_type.atom
                    || store
                        .content_exprs
                        .get(node_type.content_expr_idx)
                        .is_some_and(|expr| expr.valid_end(0) && expr.edge_count(0) == 0)
            })
        })
        .unwrap_or(false)
    }
    fn is_textblock(self) -> bool {
        with_types(|store| store.node_types[self.idx].textblock).unwrap_or(false)
    }
    fn inline_content(self) -> bool {
        with_types(|store| store.node_types[self.idx].has_inline_content).unwrap_or(false)
    }

    fn create_node(self, content: Option<&Fragment<Dyn>>, marks: Option<&MarkSet<Dyn>>) -> DynamicNode {
        let (attrs, name) = with_types(|store| {
            let nt = &store.node_types[self.idx];
            (serde_json::to_value(&nt.attrs).unwrap_or_default(), nt.name.clone())
        }).unwrap_or_default();
        DynamicNode {
            type_idx: self.idx,
            type_name: name,
            attrs,
            marks: marks.cloned().unwrap_or_default(),
            inner: DynNodeInner::Element { content: content.cloned().unwrap_or_default() },
        }
    }
}

// ---------------------------------------------------------------------------
// Mark impl
// ---------------------------------------------------------------------------

impl Mark<Dyn> for DynamicMark {
    fn r#type(&self) -> DynamicMarkType {
        with_types(|store| {
            for (i, mt) in store.mark_types.iter().enumerate() {
                if mt.name == self.type_name { return DynamicMarkType { idx: i }; }
            }
            DynamicMarkType { idx: 0 }
        }).unwrap_or(DynamicMarkType { idx: 0 })
    }
}

// ---------------------------------------------------------------------------
// Node impl
// ---------------------------------------------------------------------------

impl Node<Dyn> for DynamicNode {
    fn text_node(&self) -> Option<&TextNode<Dyn>> {
        match &self.inner { DynNodeInner::Text(tn) => Some(tn), _ => None }
    }

    fn new_text_node(node: TextNode<Dyn>) -> Self {
        let type_idx = with_types(|store| {
            store.node_types.iter().position(|nt| nt.name == "text").unwrap_or(0)
        }).unwrap_or(0);
        DynamicNode {
            type_idx,
            type_name: "text".to_string(),
            attrs: serde_json::Value::Null,
            marks: node.marks.clone(),
            inner: DynNodeInner::Text(node),
        }
    }

    fn text<A: Into<String>>(text: A) -> Self {
        let s = text.into();
        let type_idx = with_types(|store| {
            store.node_types.iter().position(|nt| nt.name == "text").unwrap_or(0)
        }).unwrap_or(0);
        DynamicNode {
            type_idx,
            type_name: "text".to_string(),
            attrs: serde_json::Value::Null,
            marks: MarkSet::new(),
            inner: DynNodeInner::Text(TextNode { text: Text::from(s), marks: MarkSet::new() }),
        }
    }

    fn content(&self) -> Option<&Fragment<Dyn>> {
        match &self.inner { DynNodeInner::Element { content } => Some(content), _ => None }
    }

    fn marks(&self) -> Option<&MarkSet<Dyn>> {
        Some(&self.marks)
    }

    fn r#type(&self) -> DynamicNodeType { DynamicNodeType { idx: self.type_idx } }

    fn is_block(&self) -> bool {
        match &self.inner {
            DynNodeInner::Text(_) => false,
            DynNodeInner::Element { .. } => {
                with_types(|store| !store.node_types[self.type_idx].inline).unwrap_or(true)
            }
        }
    }

    fn is_inline(&self) -> bool {
        match &self.inner {
            DynNodeInner::Text(_) => true,
            DynNodeInner::Element { .. } => {
                with_types(|store| store.node_types[self.type_idx].inline).unwrap_or(false)
            }
        }
    }

    fn mark(&self, marks: MarkSet<Dyn>) -> Self {
        let mut node = self.clone();
        node.marks = marks.clone();
        // Also update the inner TextNode's marks so that same_markup() stays consistent.
        if let DynNodeInner::Text(ref mut tn) = node.inner {
            tn.marks = marks;
        }
        node
    }

    fn copy<F>(&self, map: F) -> Self
    where
        F: FnOnce(&Fragment<Dyn>) -> Fragment<Dyn>,
    {
        match &self.inner {
            DynNodeInner::Text(_) => self.clone(),
            DynNodeInner::Element { content } => {
                let new_content = map(content);
                DynamicNode {
                    type_idx: self.type_idx,
                    type_name: self.type_name.clone(),
                    attrs: self.attrs.clone(),
                    marks: self.marks.clone(),
                    inner: DynNodeInner::Element { content: new_content },
                }
            }
        }
    }

    fn attrs_json(&self) -> serde_json::Value { self.attrs.clone() }

    fn with_attr(&self, attr: &str, value: serde_json::Value) -> Self {
        let mut new_attrs = match &self.attrs {
            serde_json::Value::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        };
        new_attrs.insert(attr.to_string(), value);
        DynamicNode {
            type_idx: self.type_idx,
            type_name: self.type_name.clone(),
            attrs: serde_json::Value::Object(new_attrs),
            marks: self.marks.clone(),
            inner: self.inner.clone(),
        }
    }

    fn node_size(&self) -> usize {
        match &self.inner {
            DynNodeInner::Text(tn) => tn.text.len_utf16(),
            DynNodeInner::Element { content } => {
                if self.is_leaf() { 1 } else { content.size() + 2 }
            }
        }
    }

    fn content_size(&self) -> usize {
        match &self.inner {
            DynNodeInner::Text(_) => 0,
            DynNodeInner::Element { content } => content.size(),
        }
    }

    fn child_count(&self) -> usize {
        match &self.inner {
            DynNodeInner::Text(_) => 0,
            DynNodeInner::Element { content } => content.child_count(),
        }
    }

    fn child(&self, index: usize) -> Option<&DynamicNode> {
        match &self.inner {
            DynNodeInner::Text(_) => None,
            DynNodeInner::Element { content } => content.children().get(index),
        }
    }

    fn maybe_child(&self, index: usize) -> Option<&DynamicNode> {
        match &self.inner {
            DynNodeInner::Text(_) => None,
            DynNodeInner::Element { content } => content.children().get(index),
        }
    }

    fn first_child(&self) -> Option<&DynamicNode> {
        match &self.inner {
            DynNodeInner::Text(_) => None,
            DynNodeInner::Element { content } => content.children().first(),
        }
    }

    fn text_content(&self) -> String {
        match &self.inner {
            DynNodeInner::Text(tn) => tn.text.as_str().to_string(),
            DynNodeInner::Element { content } => {
                let mut result = String::new();
                for child in content.children() {
                    result.push_str(&child.text_content());
                }
                result
            }
        }
    }

    fn is_text(&self) -> bool { matches!(self.inner, DynNodeInner::Text(_)) }
    fn is_leaf(&self) -> bool {
        match &self.inner {
            DynNodeInner::Text(_) => true,
            DynNodeInner::Element { .. } => {
                // A node is a leaf only if its *type* cannot hold any children —
                // i.e. the content-expression DFA has no outgoing edges from
                // the start state.  An empty-but-nullable type (e.g. `text*`)
                // is NOT a leaf even when it currently contains no children.
                with_types(|store| {
                    store.node_types
                        .get(self.type_idx)
                        .and_then(|t| store.content_exprs.get(t.content_expr_idx))
                        .map(|expr| {
                            expr.states.first().is_none_or(|s| s.edges.is_empty())
                        })
                })
                .flatten()
                .unwrap_or(true)
            }
        }
    }
}

impl From<TextNode<Dyn>> for DynamicNode {
    fn from(tn: TextNode<Dyn>) -> Self {
        let type_idx = with_types(|store| {
            store.node_types.iter().position(|nt| nt.name == "text").unwrap_or(0)
        }).unwrap_or(0);
        DynamicNode {
            type_idx,
            type_name: "text".to_string(),
            attrs: serde_json::Value::Null,
            marks: tn.marks.clone(),
            inner: DynNodeInner::Text(tn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DynamicMark;
    use serde_json::json;
    use std::hash::{Hash, Hasher};

    #[derive(Default)]
    struct RecordingHasher {
        bytes: Vec<u8>,
    }

    impl Hasher for RecordingHasher {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, bytes: &[u8]) {
            self.bytes.extend_from_slice(bytes);
        }
    }

    fn hash_trace(mark: &DynamicMark) -> Vec<u8> {
        let mut hasher = RecordingHasher::default();
        mark.hash(&mut hasher);
        hasher.bytes
    }

    #[test]
    fn dynamic_mark_omits_null_and_empty_attrs() {
        assert_eq!(
            serde_json::to_value(DynamicMark {
                type_name: "em".to_string(),
                attrs: serde_json::Value::Null,
            })
            .unwrap(),
            json!({ "type": "em" })
        );

        assert_eq!(
            serde_json::to_value(DynamicMark {
                type_name: "em".to_string(),
                attrs: json!({}),
            })
            .unwrap(),
            json!({ "type": "em" })
        );

        assert_eq!(
            serde_json::to_value(DynamicMark {
                type_name: "link".to_string(),
                attrs: json!({ "href": "https://example.com" }),
            })
            .unwrap(),
            json!({ "type": "link", "attrs": { "href": "https://example.com" } })
        );
    }

    #[test]
    fn dynamic_mark_hash_includes_attrs() {
        let first = DynamicMark {
            type_name: "link".to_string(),
            attrs: json!({"href": "https://example.com/a"}),
        };
        let same = DynamicMark {
            type_name: "link".to_string(),
            attrs: json!({"href": "https://example.com/a"}),
        };
        let different = DynamicMark {
            type_name: "link".to_string(),
            attrs: json!({"href": "https://example.com/b"}),
        };

        assert_eq!(first, same);
        assert_ne!(first, different);
        assert_eq!(hash_trace(&first), hash_trace(&same));
        assert_ne!(hash_trace(&first), hash_trace(&different));
    }
}
