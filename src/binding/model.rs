//! Language-agnostic model binding layer.
//!
//! Both the Node.js and Python FFI bindings hold these structs internally and
//! delegate to their methods.  Adding a new method here makes it available to
//! every binding (Node, Python, future WASM, etc.).
#![allow(clippy::should_implement_trait)] // We expose `eq` methods that mirror
                                          // the JS API; they are not meant to implement `std::cmp::PartialEq`.

use std::borrow::Cow;
use std::sync::Arc;

use serde_json::Value;

use crate::dynamic::types::{
    Dyn, DynamicMark, DynamicMarkType, DynamicNode, DynamicNodeType, ParsedContentMatch,
};
use crate::dynamic::DynamicSchema;
use crate::model::{
    ContentMatch, Fragment, Mark, MarkSet, MarkType, Node, NodeType, ResolvedPos, Slice,
};
use crate::transform::structure::NodeRange;

// ---------------------------------------------------------------------------
// BNodeType
// ---------------------------------------------------------------------------

/// Common binding wrapper around `DynamicNodeType`.
pub struct BNodeType {
    pub schema: Arc<DynamicSchema>,
    pub inner: DynamicNodeType,
    pub name: String,
}

impl BNodeType {
    pub fn new(schema: Arc<DynamicSchema>, inner: DynamicNodeType, name: String) -> Self {
        Self {
            schema,
            inner,
            name,
        }
    }

    pub fn schema_arc(&self) -> Arc<DynamicSchema> {
        self.schema.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }

    pub fn is_inline(&self) -> bool {
        self.schema.with_types(|| !self.inner.is_block())
    }

    pub fn is_textblock(&self) -> bool {
        self.schema.with_types(|| self.inner.is_textblock())
    }

    pub fn is_atom(&self) -> bool {
        self.schema.with_types(|| self.inner.is_atom())
    }

    pub fn is_leaf(&self) -> bool {
        self.schema.with_types(|| {
            self.inner.is_atom() || self.inner.content_match().match_type(self.inner).is_none()
        })
    }

    pub fn is_text(&self) -> bool {
        self.name == "text"
    }

    pub fn inline_content(&self) -> bool {
        self.schema.with_types(|| self.inner.inline_content())
    }

    // TODO(docs/PLAN.md): not yet wired up — `DynamicNodeType::content_match()`
    // returns the trait's `DynamicContentMatch` (a Copy state type), but our
    // `BContentMatch` wraps the higher-level `ParsedContentMatch`.  We need a
    // converter from `DynamicContentMatch` -> `ParsedContentMatch` (or to
    // change `BContentMatch`'s inner representation) before re-enabling.
    //
    pub fn content_match(&self) -> Option<BContentMatch> {
        let inner = self.schema.with_types(|| {
            let dcm = self.inner.content_match();
            ParsedContentMatch::from_dynamic(dcm, self.schema.clone())
        })?;
        Some(BContentMatch {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn has_required_attrs(&self) -> bool {
        self.schema.with_types(|| self.inner.has_required_attrs())
    }

    pub fn compatible_content(&self, other: &BNodeType) -> bool {
        self.schema
            .with_types(|| self.inner.compatible_content(other.inner))
    }

    pub fn whitespace(&self) -> String {
        self.schema.with_types(|| {
            self.inner
                .whitespace()
                .unwrap_or_else(|| "normal".to_string())
        })
    }

    pub fn is_code(&self) -> bool {
        self.whitespace() == "pre"
    }

    pub fn create(&self, attrs: Value, content: Fragment<Dyn>, marks: MarkSet<Dyn>) -> BNode {
        let inner = self
            .schema
            .with_types(|| self.inner.create(attrs, Some(&content), Some(&marks)));
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn create_checked(
        &self,
        attrs: Value,
        content: Fragment<Dyn>,
        marks: MarkSet<Dyn>,
    ) -> Result<BNode, String> {
        let inner = self
            .schema
            .with_types(|| self.inner.create(attrs, Some(&content), Some(&marks)));
        self.schema
            .with_types(|| inner.check().map_err(|e| e.to_string()))?;
        Ok(BNode {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn create_and_fill(
        &self,
        attrs: Value,
        content: Option<Fragment<Dyn>>,
        marks: MarkSet<Dyn>,
    ) -> Option<BNode> {
        let result = self.schema.with_types(|| {
            self.inner
                .create_and_fill(attrs, content.as_ref(), Some(&marks))
        });
        result.map(|inner| BNode {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn valid_content(&self, fragment: &Fragment<Dyn>) -> bool {
        self.schema
            .with_types(|| self.inner.valid_content(fragment))
    }

    pub fn allows_mark_type(&self, mark_type: &BMarkType) -> bool {
        self.schema
            .with_types(|| self.inner.allows_mark_type(mark_type.inner))
    }

    pub fn allows_marks(&self, marks: &MarkSet<Dyn>) -> bool {
        self.schema.with_types(|| self.inner.allow_marks(marks))
    }

    /// Whether the node type belongs to the named group.
    pub fn is_in_group(&self, group: &str) -> bool {
        self.schema.node_types[self.inner.idx]
            .groups
            .iter()
            .any(|g| g == group)
    }

    /// The default attribute values for this node type, as a JSON object.
    pub fn attrs_defaults(&self) -> Value {
        let map: serde_json::Map<String, Value> = self.schema.node_types[self.inner.idx]
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Value::Object(map)
    }

    /// The set of mark types this node type allows, as `BMarkType`s.
    /// Returns `None` if all marks are allowed (i.e. `marks` spec was `"_"`).
    pub fn mark_set(&self) -> Option<Vec<BMarkType>> {
        let nt = &self.schema.node_types[self.inner.idx];
        nt.allowed_marks.as_ref().map(|names| {
            names
                .iter()
                .filter_map(|name| {
                    let idx = self.schema.mark_type_map.get(name)?;
                    Some(BMarkType {
                        schema: self.schema.clone(),
                        inner: DynamicMarkType { idx: *idx },
                        name: name.clone(),
                    })
                })
                .collect::<Vec<_>>()
        })
    }

    /// Filter a set of marks to only those allowed by this node type.
    pub fn allowed_marks_filtered(&self, marks: Vec<DynamicMark>) -> Vec<DynamicMark> {
        let mark_set = MarkSet::<Dyn>::from_vec(marks);
        self.schema.with_types(|| {
            mark_set
                .iter()
                .filter(|m: &&DynamicMark| self.inner.allows_mark_type(m.r#type()))
                .cloned()
                .collect()
        })
    }

    /// Serialise the node-type spec as a JSON object (attrs, inline, atom, …).
    pub fn spec_json(&self) -> Value {
        let d = &self.schema.node_types[self.inner.idx];
        let mut obj = serde_json::Map::new();
        obj.insert("inline".into(), Value::Bool(d.inline));
        obj.insert("atom".into(), Value::Bool(d.atom));
        if let Some(ws) = &d.whitespace {
            obj.insert("whitespace".into(), Value::String(ws.clone()));
        }
        obj.insert(
            "group".into(),
            if d.groups.is_empty() {
                Value::Null
            } else {
                Value::String(d.groups.join(" "))
            },
        );
        let attrs: serde_json::Map<String, Value> = d
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        obj.insert("attrs".into(), Value::Object(attrs));
        Value::Object(obj)
    }
}

// ---------------------------------------------------------------------------
// BMarkType
// ---------------------------------------------------------------------------

/// Common binding wrapper around `DynamicMarkType`.
pub struct BMarkType {
    pub schema: Arc<DynamicSchema>,
    pub inner: DynamicMarkType,
    pub name: String,
}

impl BMarkType {
    pub fn schema_arc(&self) -> Arc<DynamicSchema> {
        self.schema.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn create(&self, attrs: Value) -> BMark {
        BMark {
            schema: self.schema.clone(),
            inner: DynamicMark {
                type_name: self.name.clone(),
                attrs,
            },
        }
    }

    /// Remove all marks of this type from the given set.
    pub fn remove_from_set(&self, marks: Vec<DynamicMark>) -> Vec<DynamicMark> {
        let idx = self.inner.idx;
        self.schema.with_types(|| {
            marks
                .into_iter()
                .filter(|m| m.r#type().idx != idx)
                .collect()
        })
    }

    /// Find the first mark of this type in the given set, if any.
    pub fn is_in_set(&self, marks: &[DynamicMark]) -> Option<BMark> {
        let idx = self.inner.idx;
        self.schema.with_types(|| {
            marks.iter().find(|m| m.r#type().idx == idx).map(|m| BMark {
                schema: self.schema.clone(),
                inner: m.clone(),
            })
        })
    }

    pub fn excludes(&self, other: &BMarkType) -> bool {
        self.schema.with_types(|| self.inner.excludes(other.inner))
    }

    /// Serialise the mark-type spec as a JSON object (attrs, inclusive, …).
    pub fn spec_json(&self) -> Value {
        let d = &self.schema.mark_types[self.inner.idx];
        let mut obj = serde_json::Map::new();
        obj.insert("inclusive".into(), Value::Bool(d.inclusive));
        obj.insert(
            "group".into(),
            if d.groups.is_empty() {
                Value::Null
            } else {
                Value::String(d.groups.join(" "))
            },
        );
        let attrs: serde_json::Map<String, Value> = d
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        obj.insert("attrs".into(), Value::Object(attrs));
        Value::Object(obj)
    }
}

// ---------------------------------------------------------------------------
// BMark
// ---------------------------------------------------------------------------

/// Common binding wrapper around `DynamicMark`.
pub struct BMark {
    pub schema: Arc<DynamicSchema>,
    pub inner: DynamicMark,
}

impl BMark {
    pub fn type_(&self) -> BMarkType {
        let name = self.inner.type_name.clone();
        let inner = self.schema.with_types(|| self.inner.r#type());
        BMarkType {
            schema: self.schema.clone(),
            inner,
            name,
        }
    }

    pub fn attrs_json(&self) -> Value {
        self.inner.attrs.clone()
    }

    /// JSON representation matching the JS `Mark.toJSON()` output: `{type, attrs}`.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "type": self.inner.type_name,
            "attrs": self.inner.attrs,
        })
    }

    pub fn add_to_set(&self, set: Vec<DynamicMark>) -> Vec<DynamicMark> {
        let mark_set = MarkSet::from_vec(set);
        self.schema.with_types(|| {
            self.inner
                .add_to_set(Cow::Owned(mark_set))
                .into_owned()
                .iter()
                .cloned()
                .collect()
        })
    }

    pub fn remove_from_set(&self, set: Vec<DynamicMark>) -> Vec<DynamicMark> {
        let mark_set = MarkSet::from_vec(set);
        self.schema.with_types(|| {
            self.inner
                .remove_from_set(Cow::Owned(mark_set))
                .into_owned()
                .iter()
                .cloned()
                .collect()
        })
    }

    pub fn is_in_set(&self, set: &[DynamicMark]) -> bool {
        let mark_set = MarkSet::from_vec(set.to_vec());
        self.schema.with_types(|| self.inner.is_in_set(&mark_set))
    }

    pub fn eq(&self, other: &BMark) -> bool {
        self.inner == other.inner
    }

    /// Test whether two mark sets are equal (same marks in the same order).
    pub fn same_set(a: &[DynamicMark], b: &[DynamicMark]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| x == y)
    }

    /// Create a sorted, deduplicated mark set from a slice of marks.
    pub fn set_from(schema: &Arc<DynamicSchema>, marks: Vec<DynamicMark>) -> Vec<DynamicMark> {
        if marks.is_empty() {
            return Vec::new();
        }
        // Add each mark to the set using ProseMirror's add_to_set logic to get
        // the canonical sorted & deduplicated representation.
        schema.with_types(|| {
            let mut result = MarkSet::new();
            for m in marks {
                result = m.add_to_set(std::borrow::Cow::Owned(result)).into_owned();
            }
            result.iter().cloned().collect()
        })
    }
}

// ---------------------------------------------------------------------------
// BFragment
// ---------------------------------------------------------------------------

/// Common binding wrapper around `Fragment<Dyn>`.
#[derive(Clone)]
pub struct BFragment {
    pub schema: Arc<DynamicSchema>,
    pub inner: Fragment<Dyn>,
}

impl BFragment {
    pub fn empty(schema: Arc<DynamicSchema>) -> BFragment {
        BFragment {
            schema,
            inner: Fragment::new(),
        }
    }

    pub fn from_json(schema: &Arc<DynamicSchema>, val: &Value) -> Result<BFragment, String> {
        let inner: Fragment<Dyn> = schema.with_types(|| {
            serde_json::from_value::<Fragment<Dyn>>(val.clone())
                .map_err(|e| format!("Invalid fragment JSON: {e}"))
        })?;
        Ok(BFragment {
            schema: schema.clone(),
            inner,
        })
    }

    pub fn size(&self) -> usize {
        self.inner.size()
    }

    pub fn child_count(&self) -> usize {
        self.inner.child_count()
    }

    pub fn child(&self, index: usize) -> Option<BNode> {
        self.inner.children().get(index).map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn maybe_child(&self, index: usize) -> Option<BNode> {
        self.inner.maybe_child(index).map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn first_child(&self) -> Option<BNode> {
        self.inner.first_child().map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn last_child(&self) -> Option<BNode> {
        self.inner.last_child().map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn cut(&self, from: usize, to: Option<usize>) -> BFragment {
        let to = to.unwrap_or_else(|| self.inner.size());
        let inner = self.schema.with_types(|| self.inner.cut(from..to));
        BFragment {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn append(&self, other: &BFragment) -> BFragment {
        let inner = self
            .schema
            .with_types(|| self.inner.clone().append(other.inner.clone()));
        BFragment {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn replace_child(&self, index: usize, node: DynamicNode) -> BFragment {
        let inner = self
            .schema
            .with_types(|| self.inner.replace_child(index, node).into_owned());
        BFragment {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn add_to_start(&self, node: DynamicNode) -> BFragment {
        let inner = self.schema.with_types(|| self.inner.add_to_start(node));
        BFragment {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn add_to_end(&self, node: DynamicNode) -> BFragment {
        let inner = self.schema.with_types(|| self.inner.add_to_end(node));
        BFragment {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn eq(&self, other: &BFragment) -> bool {
        self.inner == other.inner
    }

    pub fn find_diff_start(&self, other: &BFragment) -> Option<usize> {
        self.schema
            .with_types(|| self.inner.find_diff_start(&other.inner, 0))
    }

    pub fn find_diff_end(&self, other: &BFragment) -> Option<(usize, usize)> {
        self.schema.with_types(|| {
            self.inner
                .find_diff_end(&other.inner, self.inner.size(), other.inner.size())
        })
    }

    pub fn text_between(
        &self,
        from: usize,
        to: usize,
        block_sep: Option<&str>,
        leaf_text: Option<&str>,
    ) -> String {
        let mut buf = String::new();
        self.schema.with_types(|| {
            self.inner
                .text_between(&mut buf, false, from, to, block_sep, leaf_text)
        });
        buf
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(&self.inner).unwrap_or(Value::Null)
    }

    /// Iterate children with a Rust closure.  Bindings adapt their callback type
    /// to a Rust `FnMut`.
    pub fn for_each(&self, mut f: impl FnMut(&DynamicNode, usize, usize)) {
        self.schema
            .with_types(|| self.inner.for_each(|n, o, i| f(n, o, i)));
    }

    pub fn nodes_between(
        &self,
        from: usize,
        to: usize,
        f: &mut impl FnMut(&DynamicNode, usize, Option<&DynamicNode>, usize) -> bool,
    ) {
        self.schema
            .with_types(|| self.inner.nodes_between(from, to, f, 0, None));
    }
}

// ---------------------------------------------------------------------------
// BSlice
// ---------------------------------------------------------------------------

/// Common binding wrapper around `Slice<Dyn>`.
pub struct BSlice {
    pub schema: Arc<DynamicSchema>,
    pub inner: Slice<Dyn>,
}

impl BSlice {
    pub fn new(content: &BFragment, open_start: usize, open_end: usize) -> BSlice {
        BSlice {
            schema: content.schema.clone(),
            inner: Slice::new(content.inner.clone(), open_start, open_end),
        }
    }

    pub fn empty(schema: Arc<DynamicSchema>) -> BSlice {
        BSlice {
            schema,
            inner: Slice::new(Fragment::new(), 0, 0),
        }
    }

    pub fn content(&self) -> BFragment {
        BFragment {
            schema: self.schema.clone(),
            inner: self.inner.content.clone(),
        }
    }

    pub fn open_start(&self) -> usize {
        self.inner.open_start
    }

    pub fn open_end(&self) -> usize {
        self.inner.open_end
    }

    pub fn size(&self) -> usize {
        self.inner.size()
    }

    pub fn eq(&self, other: &BSlice) -> bool {
        self.inner == other.inner
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(&self.inner).unwrap_or(Value::Null)
    }

    pub fn from_json(schema: &Arc<DynamicSchema>, val: &Value) -> Result<BSlice, String> {
        let inner: Slice<Dyn> = schema.with_types(|| {
            serde_json::from_value::<Slice<Dyn>>(val.clone())
                .map_err(|e| format!("Invalid slice JSON: {e}"))
        })?;
        Ok(BSlice {
            schema: schema.clone(),
            inner,
        })
    }

    pub fn max_open(fragment: BFragment, open_isolating: bool) -> BSlice {
        let schema = fragment.schema.clone();
        let inner = schema.with_types(|| Slice::max_open(fragment.inner, open_isolating));
        BSlice { schema, inner }
    }
}

// ---------------------------------------------------------------------------
// BNode
// ---------------------------------------------------------------------------

/// Common binding wrapper around `DynamicNode`.
#[derive(Clone)]
pub struct BNode {
    pub schema: Arc<DynamicSchema>,
    pub inner: DynamicNode,
}

impl BNode {
    pub fn from_json(schema: &Arc<DynamicSchema>, val: &Value) -> Result<BNode, String> {
        let inner = schema
            .node_from_json(val)
            .map_err(|e| format!("Invalid node JSON: {e}"))?;
        Ok(BNode {
            schema: schema.clone(),
            inner,
        })
    }

    pub fn type_(&self) -> BNodeType {
        let inner = self.schema.with_types(|| self.inner.r#type());
        BNodeType {
            schema: self.schema.clone(),
            inner,
            name: self.inner.type_name.clone(),
        }
    }

    pub fn attrs_json(&self) -> Value {
        self.inner.attrs_json()
    }

    pub fn content(&self) -> Option<BFragment> {
        self.schema.with_types(|| {
            Node::content(&self.inner).map(|f| BFragment {
                schema: self.schema.clone(),
                inner: (*f).clone(),
            })
        })
    }

    pub fn marks_vec(&self) -> Vec<DynamicMark> {
        self.schema.with_types(|| {
            Node::marks(&self.inner)
                .map(|ms| ms.iter().cloned().collect())
                .unwrap_or_default()
        })
    }

    pub fn text(&self) -> Option<String> {
        self.schema
            .with_types(|| self.inner.text_node().map(|tn| tn.text.as_str().to_owned()))
    }

    pub fn text_content(&self) -> String {
        self.schema.with_types(|| self.inner.text_content())
    }

    pub fn node_size(&self) -> usize {
        self.schema.with_types(|| self.inner.node_size())
    }

    pub fn content_size(&self) -> usize {
        self.schema.with_types(|| self.inner.content_size())
    }

    pub fn child_count(&self) -> usize {
        self.inner.child_count()
    }

    pub fn child(&self, index: usize) -> Option<BNode> {
        Node::child(&self.inner, index).map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn maybe_child(&self, index: usize) -> Option<BNode> {
        Node::maybe_child(&self.inner, index).map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn first_child(&self) -> Option<BNode> {
        <DynamicNode as Node<Dyn>>::first_child(&self.inner).map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn last_child(&self) -> Option<BNode> {
        <DynamicNode as Node<Dyn>>::last_child(&self.inner).map(|n| BNode {
            schema: self.schema.clone(),
            inner: n.clone(),
        })
    }

    pub fn is_text(&self) -> bool {
        self.inner.is_text()
    }

    pub fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }

    pub fn is_inline(&self) -> bool {
        self.schema.with_types(|| self.inner.is_inline())
    }

    pub fn is_leaf(&self) -> bool {
        self.schema.with_types(|| self.inner.is_leaf())
    }

    pub fn is_textblock(&self) -> bool {
        self.schema.with_types(|| self.inner.is_textblock())
    }

    pub fn is_atom(&self) -> bool {
        self.schema.with_types(|| self.inner.is_atom())
    }

    pub fn inline_content(&self) -> bool {
        self.schema.with_types(|| self.inner.inline_content())
    }

    pub fn same_markup(&self, other: &BNode) -> bool {
        self.schema
            .with_types(|| self.inner.same_markup(&other.inner))
    }

    pub fn range_has_mark(&self, from: usize, to: usize, mark_type: DynamicMarkType) -> bool {
        self.schema
            .with_types(|| self.inner.range_has_mark(from, to, mark_type))
    }

    pub fn can_append(&self, other: &BNode) -> bool {
        self.schema
            .with_types(|| self.inner.can_append(&other.inner))
    }

    // TODO(docs/PLAN.md): blocked on the same `DynamicContentMatch` <->
    // `ParsedContentMatch` conversion as `BNodeType::content_match()`.
    //
    pub fn content_match_at(&self, index: usize) -> Result<BContentMatch, String> {
        let inner = self.schema.with_types(|| {
            let dcm = Node::content_match_at(&self.inner, index).map_err(|e| format!("{e}"))?;
            ParsedContentMatch::from_dynamic(dcm, self.schema.clone())
                .ok_or_else(|| "content_match_at: failed to convert match".to_string())
        })?;
        Ok(BContentMatch {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn slice(&self, from: usize, to: usize, include_parents: bool) -> BSlice {
        let inner = self.schema.with_types(|| {
            self.inner
                .slice(from..to, include_parents)
                .unwrap_or_else(|_| Slice::new(Fragment::new(), 0, 0))
        });
        BSlice {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn cut(&self, from: usize, to: usize) -> BNode {
        let inner = self
            .schema
            .with_types(|| self.inner.cut(from..to).into_owned());
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn replace(&self, from: usize, to: usize, slice: &BSlice) -> Result<BNode, String> {
        let inner = self.schema.with_types(|| {
            self.inner
                .replace(from..to, &slice.inner)
                .map_err(|e| format!("Replace failed: {e}"))
        })?;
        Ok(BNode {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn resolve(&self, pos: usize) -> BResolvedPos {
        BResolvedPos {
            schema: self.schema.clone(),
            doc: self.inner.clone(),
            pos,
        }
    }

    pub fn eq(&self, other: &BNode) -> bool {
        self.schema
            .with_types(|| <DynamicNode as Node<Dyn>>::eq(&self.inner, &other.inner))
    }

    pub fn to_json(&self, skip_defaults: bool) -> Value {
        self.schema.with_types(|| self.inner.to_json(skip_defaults))
    }

    pub fn to_debug_string(&self) -> String {
        self.schema.with_types(|| self.inner.to_debug_string())
    }

    pub fn text_between(
        &self,
        from: usize,
        to: usize,
        block_sep: Option<&str>,
        leaf_text: Option<&str>,
    ) -> String {
        self.schema
            .with_types(|| Node::text_between(&self.inner, from, to, block_sep, leaf_text))
    }

    pub fn mark(&self, marks: MarkSet<Dyn>) -> BNode {
        let inner = self.schema.with_types(|| self.inner.mark(marks));
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn copy(&self, content: Fragment<Dyn>) -> BNode {
        let inner = self
            .schema
            .with_types(|| self.inner.copy(|_| content.clone()));
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn check(&self) -> Result<(), String> {
        self.schema
            .with_types(|| self.inner.check().map_err(|e| e.to_string()))
    }

    pub fn node_at(&self, pos: usize) -> Option<BNode> {
        self.schema.with_types(|| {
            <DynamicNode as Node<Dyn>>::node_at(&self.inner, pos).map(|n| BNode {
                schema: self.schema.clone(),
                inner: n.clone(),
            })
        })
    }

    pub fn for_each(&self, f: &mut impl FnMut(&DynamicNode, usize, usize)) {
        self.schema
            .with_types(|| <DynamicNode as Node<Dyn>>::for_each(&self.inner, f));
    }

    pub fn nodes_between(
        &self,
        from: usize,
        to: usize,
        f: &mut impl FnMut(&DynamicNode, usize, Option<&DynamicNode>, usize) -> bool,
    ) {
        self.schema
            .with_types(|| <DynamicNode as Node<Dyn>>::nodes_between(&self.inner, from, to, f, 0));
    }

    pub fn descendants(
        &self,
        f: &mut impl FnMut(&DynamicNode, usize, Option<&DynamicNode>, usize) -> bool,
    ) {
        self.schema
            .with_types(|| <DynamicNode as Node<Dyn>>::descendants(&self.inner, f));
    }

    /// Test whether the node's type, attrs, and marks match a given combination.
    /// `marks` is `None` to skip marks check, `Some(&[])` to check for empty marks.
    pub fn has_markup(
        &self,
        type_: &BNodeType,
        attrs: Option<&Value>,
        marks: Option<&[DynamicMark]>,
    ) -> bool {
        if self.inner.type_name != type_.name {
            return false;
        }
        if let Some(a) = attrs {
            if &self.inner.attrs_json() != a {
                return false;
            }
        }
        if let Some(m) = marks {
            let node_marks = self.marks_vec();
            if node_marks.len() != m.len() {
                return false;
            }
            if !node_marks.iter().zip(m.iter()).all(|(a, b)| a == b) {
                return false;
            }
        }
        true
    }

    /// Returns `{node, index, offset}` of the child immediately after `pos`, or
    /// `None` if no such child exists.  (`pos` is relative to this node.)
    pub fn child_after(&self, pos: usize) -> Option<(BNode, usize, usize)> {
        self.schema.with_types(|| {
            let content = <DynamicNode as Node<Dyn>>::content(&self.inner)?;
            let children = content.children();
            let mut offset = 0usize;
            for (i, child) in children.iter().enumerate() {
                let end = offset + child.node_size();
                if end > pos {
                    return Some((
                        BNode {
                            schema: self.schema.clone(),
                            inner: child.clone(),
                        },
                        i,
                        offset,
                    ));
                }
                offset = end;
            }
            None
        })
    }

    /// Returns `{node, index, offset}` of the child immediately before `pos`, or
    /// `None` if no such child exists.  (`pos` is relative to this node.)
    pub fn child_before(&self, pos: usize) -> Option<(BNode, usize, usize)> {
        self.schema.with_types(|| {
            let content = <DynamicNode as Node<Dyn>>::content(&self.inner)?;
            let children = content.children();
            let mut offset = 0usize;
            for (i, child) in children.iter().enumerate() {
                let end = offset + child.node_size();
                if end >= pos {
                    if i == 0 {
                        return None;
                    }
                    // go back one child
                    let prev_child = &children[i - 1];
                    let prev_offset = offset - prev_child.node_size();
                    return Some((
                        BNode {
                            schema: self.schema.clone(),
                            inner: prev_child.clone(),
                        },
                        i - 1,
                        prev_offset,
                    ));
                }
                offset = end;
            }
            // pos is after all children — return last child
            if let Some(last) = children.last() {
                let last_offset = offset - last.node_size();
                Some((
                    BNode {
                        schema: self.schema.clone(),
                        inner: last.clone(),
                    },
                    children.len() - 1,
                    last_offset,
                ))
            } else {
                None
            }
        })
    }

    /// Check whether the given fragment can replace the content at `from..to`
    /// (child indices).  Optional `start`/`end` index into the replacement.
    pub fn can_replace(
        &self,
        from: usize,
        to: usize,
        replacement: Option<&BFragment>,
        start: usize,
        end: Option<usize>,
    ) -> bool {
        self.schema.with_types(|| {
            let frag = replacement.map(|r| &r.inner);
            let end_idx = end.unwrap_or_else(|| frag.map(|f| f.child_count()).unwrap_or(0));
            <DynamicNode as Node<Dyn>>::can_replace(&self.inner, from, to, frag, start..end_idx)
                .unwrap_or(false)
        })
    }

    /// Check whether a node of the given type (and optionally attrs) can replace
    /// the content between `from` and `to` (child indices).
    pub fn can_replace_with(&self, from: usize, to: usize, type_: &BNodeType) -> bool {
        self.schema.with_types(|| {
            <DynamicNode as Node<Dyn>>::can_replace_with(&self.inner, from, to, type_.inner)
        })
    }
}

// ---------------------------------------------------------------------------
// BResolvedPos
// ---------------------------------------------------------------------------

/// Common binding wrapper around a position to be resolved.
///
/// We store the document and the (unresolved) position rather than a
/// `ResolvedPos`, because the resolved form holds borrows into the document
/// and cannot easily be stored across FFI calls.
pub struct BResolvedPos {
    pub schema: Arc<DynamicSchema>,
    pub doc: DynamicNode,
    pub pos: usize,
}

impl BResolvedPos {
    fn with_resolved<R>(&self, f: impl FnOnce(&ResolvedPos<'_, Dyn>) -> R) -> Option<R> {
        self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .ok()
                .map(|r| f(&r))
        })
    }

    pub fn depth(&self) -> usize {
        self.with_resolved(|r| r.depth).unwrap_or(0)
    }

    pub fn parent_offset(&self) -> usize {
        self.with_resolved(|r| r.parent_offset).unwrap_or(0)
    }

    pub fn parent(&self) -> BNode {
        let inner = self
            .with_resolved(|r| r.parent().clone())
            .unwrap_or_else(|| self.doc.clone());
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn doc_node(&self) -> BNode {
        BNode {
            schema: self.schema.clone(),
            inner: self.doc.clone(),
        }
    }

    pub fn node(&self, depth: Option<usize>) -> BNode {
        let d = depth.unwrap_or_else(|| self.depth());
        let inner = self
            .with_resolved(|r| r.node(d).clone())
            .unwrap_or_else(|| self.doc.clone());
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn index(&self, depth: Option<usize>) -> usize {
        let d = depth.unwrap_or_else(|| self.depth());
        self.with_resolved(|r| r.index(d)).unwrap_or(0)
    }

    pub fn index_after(&self, depth: Option<usize>) -> usize {
        let d = depth.unwrap_or_else(|| self.depth());
        self.with_resolved(|r| r.index_after(d)).unwrap_or(0)
    }

    pub fn start(&self, depth: Option<usize>) -> usize {
        let d = depth.unwrap_or_else(|| self.depth());
        self.with_resolved(|r| r.start(d)).unwrap_or(0)
    }

    pub fn end(&self, depth: Option<usize>) -> usize {
        let d = depth.unwrap_or_else(|| self.depth());
        self.with_resolved(|r| r.end(d)).unwrap_or(0)
    }

    pub fn before(&self, depth: Option<usize>) -> Option<usize> {
        let d = depth.unwrap_or_else(|| self.depth());
        self.with_resolved(|r| r.before(d)).flatten()
    }

    pub fn after(&self, depth: Option<usize>) -> Option<usize> {
        let d = depth.unwrap_or_else(|| self.depth());
        self.with_resolved(|r| r.after(d)).flatten()
    }

    pub fn text_offset(&self) -> usize {
        self.with_resolved(|r| r.text_offset()).unwrap_or(0)
    }

    pub fn node_before(&self) -> Option<BNode> {
        self.with_resolved(|r| {
            r.node_before().map(|n| BNode {
                schema: self.schema.clone(),
                inner: n.into_owned(),
            })
        })
        .flatten()
    }

    pub fn node_after(&self) -> Option<BNode> {
        self.with_resolved(|r| {
            r.node_after().map(|n| BNode {
                schema: self.schema.clone(),
                inner: n.into_owned(),
            })
        })
        .flatten()
    }

    pub fn pos_at_index(&self, index: usize, depth: Option<usize>) -> usize {
        self.schema.with_types(|| {
            ResolvedPos::<Dyn>::resolve(&self.doc, self.pos)
                .map(|r| r.pos_at_index(index, depth))
                .unwrap_or(0)
        })
    }

    pub fn marks(&self) -> Vec<DynamicMark> {
        self.with_resolved(|r| r.marks().into_iter().collect())
            .unwrap_or_default()
    }

    pub fn marks_across(&self, end: &BResolvedPos) -> Option<Vec<DynamicMark>> {
        self.schema.with_types(|| {
            let a = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).ok()?;
            let b = ResolvedPos::<Dyn>::resolve(&end.doc, end.pos).ok()?;
            a.marks_across(&b)
        })
    }

    pub fn shared_depth(&self, pos: usize) -> usize {
        self.with_resolved(|r| r.shared_depth(pos)).unwrap_or(0)
    }

    pub fn block_range(&self, other: Option<&BResolvedPos>) -> Option<BNodeRange> {
        self.schema.with_types(|| {
            let rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).ok()?;
            let other_rp = other.and_then(|o| ResolvedPos::<Dyn>::resolve(&o.doc, o.pos).ok());
            let range = rp.block_range(other_rp.as_ref(), None)?;
            Some(BNodeRange {
                schema: self.schema.clone(),
                doc: self.doc.clone(),
                from_pos: range.from.pos,
                to_pos: range.to.pos,
                depth: range.depth,
            })
        })
    }

    pub fn same_parent(&self, other: &BResolvedPos) -> bool {
        self.schema
            .with_types(|| {
                let a = ResolvedPos::<Dyn>::resolve(&self.doc, self.pos).ok()?;
                let b = ResolvedPos::<Dyn>::resolve(&other.doc, other.pos).ok()?;
                Some(a.same_parent(&b))
            })
            .unwrap_or(false)
    }

    pub fn max(&self, other: &BResolvedPos) -> BResolvedPos {
        if self.pos >= other.pos {
            BResolvedPos {
                schema: self.schema.clone(),
                doc: self.doc.clone(),
                pos: self.pos,
            }
        } else {
            BResolvedPos {
                schema: other.schema.clone(),
                doc: other.doc.clone(),
                pos: other.pos,
            }
        }
    }

    pub fn min(&self, other: &BResolvedPos) -> BResolvedPos {
        if self.pos <= other.pos {
            BResolvedPos {
                schema: self.schema.clone(),
                doc: self.doc.clone(),
                pos: self.pos,
            }
        } else {
            BResolvedPos {
                schema: other.schema.clone(),
                doc: other.doc.clone(),
                pos: other.pos,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BNodeRange
// ---------------------------------------------------------------------------

/// Common binding wrapper around a `NodeRange`.
pub struct BNodeRange {
    pub schema: Arc<DynamicSchema>,
    pub doc: DynamicNode,
    pub from_pos: usize,
    pub to_pos: usize,
    pub depth: usize,
}

impl BNodeRange {
    /// Internal helper: materialise the borrow-based `NodeRange<'_, Dyn>` from
    /// the stored document/positions.  Returns `None` if either position can't
    /// be resolved.
    pub fn to_node_range(&self) -> Option<NodeRange<'_, Dyn>> {
        self.schema.with_types(|| {
            let from = ResolvedPos::<Dyn>::resolve(&self.doc, self.from_pos).ok()?;
            let to = ResolvedPos::<Dyn>::resolve(&self.doc, self.to_pos).ok()?;
            Some(NodeRange::new(from, to, self.depth))
        })
    }

    pub fn from_resolved_pos(&self) -> BResolvedPos {
        BResolvedPos {
            schema: self.schema.clone(),
            doc: self.doc.clone(),
            pos: self.from_pos,
        }
    }

    pub fn to_resolved_pos(&self) -> BResolvedPos {
        BResolvedPos {
            schema: self.schema.clone(),
            doc: self.doc.clone(),
            pos: self.to_pos,
        }
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn start(&self) -> usize {
        self.schema
            .with_types(|| {
                let from_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.from_pos).ok()?;
                from_rp.before(self.depth + 1)
            })
            .unwrap_or(0)
    }

    pub fn end(&self) -> usize {
        self.schema
            .with_types(|| {
                let to_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.to_pos).ok()?;
                to_rp.after(self.depth + 1)
            })
            .unwrap_or(0)
    }

    pub fn parent(&self) -> BNode {
        let inner = self
            .schema
            .with_types(|| {
                let from_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.from_pos).ok()?;
                Some(from_rp.node(self.depth).clone())
            })
            .unwrap_or_else(|| self.doc.clone());
        BNode {
            schema: self.schema.clone(),
            inner,
        }
    }

    pub fn start_index(&self) -> usize {
        self.schema
            .with_types(|| {
                let from_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.from_pos).ok()?;
                Some(from_rp.index(self.depth))
            })
            .unwrap_or(0)
    }

    pub fn end_index(&self) -> usize {
        self.schema
            .with_types(|| {
                let to_rp = ResolvedPos::<Dyn>::resolve(&self.doc, self.to_pos).ok()?;
                Some(to_rp.index_after(self.depth))
            })
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// BContentMatch
// ---------------------------------------------------------------------------

/// Common binding wrapper around `ParsedContentMatch` (DFA state for the
/// content expression of a node type).
pub struct BContentMatch {
    pub schema: Arc<DynamicSchema>,
    pub inner: ParsedContentMatch,
}

impl BContentMatch {
    pub fn parse(expr: &str, schema: &Arc<DynamicSchema>) -> Result<BContentMatch, String> {
        let inner = ParsedContentMatch::parse(expr, schema).map_err(|e| e.to_string())?;
        Ok(BContentMatch {
            schema: schema.clone(),
            inner,
        })
    }

    pub fn valid_end(&self) -> bool {
        self.inner.valid_end()
    }

    pub fn match_type(&self, type_: &BNodeType) -> Option<BContentMatch> {
        // Use the node type name directly — this is cross-schema safe because
        // BNodeType.name is stored by value, not by index.
        let inner = self.inner.match_type_by_name(&type_.name)?;
        Some(BContentMatch {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn match_fragment(
        &self,
        frag: &BFragment,
        _start: usize,
        _end: Option<usize>,
    ) -> Option<BContentMatch> {
        // The Rust API only supports matching whole fragments; start/end is
        // ignored.  (The JS API is also rarely called with non-default args.)
        // ParsedContentMatch::match_fragment now uses child.type_name directly,
        // making this cross-schema safe.
        let inner = self.inner.match_fragment(&frag.inner)?;
        Some(BContentMatch {
            schema: self.schema.clone(),
            inner,
        })
    }

    pub fn fill_before(
        &self,
        after: &BFragment,
        to_end: bool,
        start_index: usize,
    ) -> Option<BFragment> {
        // Use the after fragment's schema context when calling fill_before,
        // so that any filler nodes created have type indices matching the
        // fragment's schema (cross-schema safe).
        // The ParsedContentMatch methods use child.type_name directly for
        // matching, so the actual expression matching is also cross-schema safe.
        after.schema.with_types(|| {
            let inner = self.inner.fill_before(&after.inner, to_end, start_index)?;
            Some(BFragment {
                schema: after.schema.clone(),
                inner,
            })
        })
    }

    // TODO(docs/PLAN.md): `default_type`, `find_wrapping`, `edge_count`, and
    // `edge` need to be added to `ParsedContentMatch` (or we need to switch
    // BContentMatch's inner type to `DynamicContentMatch`).  These methods
    // are part of the public ProseMirror docs and must be exposed eventually.
    //
    pub fn default_type(&self) -> Option<BNodeType> {
        let nt = self.inner.default_type()?;
        let name = self.schema.node_types.get(nt.idx)?.name.clone();
        Some(BNodeType {
            schema: self.schema.clone(),
            inner: nt,
            name,
        })
    }

    pub fn find_wrapping(&self, target: &BNodeType) -> Option<Vec<BNodeType>> {
        let types = self.inner.find_wrapping(target.inner)?;
        Some(
            types
                .into_iter()
                .map(|nt| {
                    let name = self
                        .schema
                        .node_types
                        .get(nt.idx)
                        .map(|d| d.name.clone())
                        .unwrap_or_default();
                    BNodeType {
                        schema: self.schema.clone(),
                        inner: nt,
                        name,
                    }
                })
                .collect(),
        )
    }

    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    pub fn edge(&self, n: usize) -> Option<(BNodeType, BContentMatch)> {
        let (nt, next) = self.inner.edge(n)?;
        let name = self.schema.node_types.get(nt.idx)?.name.clone();
        Some((
            BNodeType {
                schema: self.schema.clone(),
                inner: nt,
                name,
            },
            BContentMatch {
                schema: self.schema.clone(),
                inner: next,
            },
        ))
    }
}

// ---------------------------------------------------------------------------
// Schema free functions
// ---------------------------------------------------------------------------

/// Return the top-level node type of a schema (the "doc" type by default).
pub fn b_schema_top_node_type(schema: &Arc<DynamicSchema>) -> Option<BNodeType> {
    let name = schema.top_node.clone();
    let idx = *schema.node_type_map.get(&name)?;
    Some(BNodeType {
        schema: schema.clone(),
        inner: DynamicNodeType { idx },
        name,
    })
}

// ---------------------------------------------------------------------------
// Fragment::from  (polymorphic)
// ---------------------------------------------------------------------------

/// The input accepted by `Fragment.from()` in ProseMirror JS.
pub enum FragmentFromInput {
    /// `null` / `undefined` → empty fragment
    Null,
    /// A single node
    SingleNode(BNode),
    /// An array of nodes
    NodeArray(Vec<DynamicNode>),
    /// An existing fragment (cloned)
    Fragment(BFragment),
}

/// Implement `Fragment.from(input)` — identical semantics to the JS original.
pub fn b_fragment_from(schema: Arc<DynamicSchema>, input: FragmentFromInput) -> BFragment {
    match input {
        FragmentFromInput::Null => BFragment::empty(schema),
        FragmentFromInput::SingleNode(n) => {
            let inner = schema.with_types(|| Fragment::from_array(vec![n.inner]));
            BFragment { schema, inner }
        }
        FragmentFromInput::NodeArray(nodes) => {
            let inner = schema.with_types(|| Fragment::from_array(nodes));
            BFragment { schema, inner }
        }
        FragmentFromInput::Fragment(f) => f,
    }
}
