//! Dynamic runtime types for nodes, marks, and their type descriptors.

use crate::dynamic::content_expr::{ContentExpr, ContentExprError};
use crate::dynamic::schema::DynamicSchema;
use crate::model::MarkType;
use crate::model::{ContentMatch, Fragment, Mark, MarkSet, Node, NodeType, Schema, Text, TextNode};
use serde::{Deserialize, Deserializer, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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
    /// Whether this is isolating
    pub isolating: bool,
    /// Whether this is a defining node
    pub defining: bool,
    /// Whether this node defines context for its content
    pub defining_as_context: bool,
    /// Whether this node is defining for its content
    pub defining_for_content: bool,
    /// Whether this has required attributes
    pub has_required_attrs: bool,
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
    /// Attribute validators: attr name -> expected type string (e.g. "string|null")
    pub attr_validators: HashMap<String, String>,
    /// The set of allowed mark type names (None = all allowed)
    pub allowed_marks: Option<Vec<String>>,
    /// Whitespace handling mode for this node type (e.g. "pre")
    pub whitespace: Option<String>,
    /// Whether this node type represents a line break replacement
    pub linebreak_replacement: bool,
}

/// Runtime data for a dynamic mark type.
#[derive(Debug, Clone)]
pub struct DynamicMarkTypeData {
    /// The name of this mark type
    pub name: String,
    /// Attribute names with their defaults
    pub attrs: HashMap<String, serde_json::Value>,
    /// Attribute validators: attr name -> expected type string (e.g. "string|null")
    pub attr_validators: HashMap<String, String>,
    /// Whether this mark is inclusive
    pub inclusive: bool,
    /// Which other mark types this one excludes (raw names from spec)
    pub excludes: Vec<String>,
    /// Resolved excluded mark type indices
    pub excluded: Vec<usize>,
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
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}
impl Eq for DynamicNodeType {}
impl PartialOrd for DynamicNodeType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DynamicNodeType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.idx.cmp(&other.idx)
    }
}
impl Hash for DynamicNodeType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}
impl fmt::Display for DynamicNodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeType({})", self.idx)
    }
}

/// A lightweight, Copy handle to a dynamic mark type.
#[derive(Debug, Clone, Copy)]
pub struct DynamicMarkType {
    /// Index into the schema's mark_types array
    pub idx: usize,
}

impl PartialEq for DynamicMarkType {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}
impl Eq for DynamicMarkType {}
impl PartialOrd for DynamicMarkType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DynamicMarkType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.idx.cmp(&other.idx)
    }
}
impl Hash for DynamicMarkType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}

/// Return the ProseMirror-style type name for a JSON value
/// (used by attribute validation).
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "object",
        serde_json::Value::Object(_) => "object",
    }
}

/// Validate attribute values against a type's spec.
/// `spec_attrs` — the declared attributes with defaults.
/// `validators` — attr name -> expected type string (e.g. "string|null").
/// `attrs` — the actual values on the node/mark.
/// `type_kind` — "node" or "mark".
/// `type_name` — the type's name (e.g. "image").
pub fn check_attrs_helper(
    spec_attrs: &std::collections::HashMap<String, serde_json::Value>,
    validators: &std::collections::HashMap<String, String>,
    attrs: &serde_json::Value,
    type_kind: &str,
    type_name: &str,
) -> Result<(), String> {
    let obj = match attrs.as_object() {
        Some(o) => o,
        None => return Ok(()),
    };
    // Check for unsupported attributes
    for (key, _) in obj {
        if !spec_attrs.contains_key(key) {
            return Err(format!(
                "Unsupported attribute {} for {} of type {}",
                key, type_kind, type_name
            ));
        }
    }
    // Check validators
    for (attr_name, expected) in validators {
        let value = obj.get(attr_name).unwrap_or(&serde_json::Value::Null);
        let actual = json_type_name(value);
        let types: Vec<&str> = expected.split('|').collect();
        if !types.contains(&actual) {
            return Err(format!(
                "Expected value of type {} for attribute {} on type {}, got {}",
                expected, attr_name, type_name, actual
            ));
        }
    }
    Ok(())
}

impl MarkType for DynamicMarkType {
    fn rank(self) -> usize {
        self.idx
    }
    fn excludes(self, other: Self) -> bool {
        with_types(|store| {
            store
                .mark_types
                .get(self.idx)
                .map(|data| data.excluded.contains(&other.idx))
                .unwrap_or(false)
        })
        .unwrap_or(false)
    }
    fn inclusive(self) -> bool {
        with_types(|store| {
            store
                .mark_types
                .get(self.idx)
                .map(|data| data.inclusive)
                .unwrap_or(true)
        })
        .unwrap_or(true)
    }
    fn check_attrs(self, attrs: &serde_json::Value) -> Result<(), String> {
        with_types(|store| {
            if let Some(mt) = store.mark_types.get(self.idx) {
                check_attrs_helper(&mt.attrs, &mt.attr_validators, attrs, "mark", &mt.name)
            } else {
                Ok(())
            }
        })
        .unwrap_or(Ok(()))
    }
}

/// A content match backed by a DFA, using an index into the schema's expr array.
#[derive(Debug, Clone, Copy)]
pub struct DynamicContentMatch {
    /// Index into the schema's content_exprs array
    pub expr_idx: usize,
    /// The current DFA state index
    pub state: usize,
}

impl PartialEq for DynamicContentMatch {
    fn eq(&self, other: &Self) -> bool {
        self.expr_idx == other.expr_idx && self.state == other.state
    }
}
impl Eq for DynamicContentMatch {}

impl ContentMatch<Dyn> for DynamicContentMatch {
    fn match_fragment_range<R: RangeBounds<usize>>(
        self,
        fragment: &Fragment<Dyn>,
        range: R,
    ) -> Option<Self> {
        use crate::model::util;
        let start = util::from(&range);
        let end = util::to(&range, fragment.child_count());
        if start > end || end > fragment.child_count() {
            return None;
        }
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let mut state = self.state;
            for child in &fragment.children()[start..end] {
                let name = &store.node_types[child.r#type().idx].name;
                state = expr.match_type(state, name)?;
            }
            Some(DynamicContentMatch {
                expr_idx: self.expr_idx,
                state,
            })
        })
        .flatten()
    }

    fn valid_end(self) -> bool {
        with_types(|store| store.content_exprs[self.expr_idx].valid_end(self.state))
            .unwrap_or(false)
    }

    fn match_type(self, r#type: DynamicNodeType) -> Option<Self> {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let name = &store.node_types[r#type.idx].name;
            let next = expr.match_type(self.state, name)?;
            Some(DynamicContentMatch {
                expr_idx: self.expr_idx,
                state: next,
            })
        })
        .flatten()
    }

    fn fill_before(
        self,
        after: &Fragment<Dyn>,
        to_end: bool,
        start_index: usize,
    ) -> Option<Fragment<Dyn>> {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            let mut seen = HashSet::new();
            seen.insert(self.state);
            let mut types = Vec::new();
            let result = fill_before_search(
                expr,
                self.state,
                after,
                start_index,
                to_end,
                &mut seen,
                &mut types,
                store,
                0,
            )?;
            Some(Fragment::from(result))
        })
        .flatten()
    }

    fn find_wrapping(self, target: DynamicNodeType) -> Option<Vec<DynamicNodeType>> {
        with_types(|store| {
            let target_name = &store.node_types[target.idx].name;
            let expr = &store.content_exprs[self.expr_idx];
            if expr.match_type(self.state, target_name).is_some() {
                return Some(Vec::new());
            }
            // BFS through wrapper types, matching JS computeWrapping
            let mut seen = HashSet::new();
            struct WrapEntry {
                expr_idx: usize,
                state: usize,
                wrapper_type: Option<usize>,
                via: Option<usize>,
            }
            let mut active: Vec<WrapEntry> = Vec::new();
            active.push(WrapEntry {
                expr_idx: self.expr_idx,
                state: self.state,
                wrapper_type: None,
                via: None,
            });
            let mut queue_idx = 0;
            while queue_idx < active.len() {
                let current_expr_idx = active[queue_idx].expr_idx;
                let current_state = active[queue_idx].state;
                let current_wrapper = active[queue_idx].wrapper_type;
                let current_via = active[queue_idx].via;
                queue_idx += 1;
                let current_expr = &store.content_exprs[current_expr_idx];
                if current_expr
                    .match_type(current_state, target_name)
                    .is_some()
                {
                    // Reconstruct the path, including current entry's wrapper type
                    let mut result = Vec::new();
                    if let Some(idx) = current_wrapper {
                        result.push(DynamicNodeType { idx });
                    }
                    let mut next_via = current_via;
                    while let Some(v) = next_via {
                        let entry = &active[v];
                        if let Some(idx) = entry.wrapper_type {
                            result.push(DynamicNodeType { idx });
                        }
                        next_via = entry.via;
                    }
                    result.reverse();
                    return Some(result);
                }
                for i in 0..current_expr.edge_count(current_state) {
                    let (name, next_state) = current_expr.edge(current_state, i)?;
                    let node_type_idx = store.node_types.iter().position(|nt| nt.name == name)?;
                    let nt = &store.node_types[node_type_idx];
                    // Skip leaf nodes and nodes with required attrs
                    if nt.atom || nt.has_required_attrs {
                        continue;
                    }
                    if !seen.insert(name.to_string()) {
                        continue;
                    }
                    // If we're not at the start, next_state must be a valid end
                    if current_wrapper.is_some() && !current_expr.valid_end(next_state) {
                        continue;
                    }
                    let wrapper_expr_idx = nt.content_expr_idx;
                    active.push(WrapEntry {
                        expr_idx: wrapper_expr_idx,
                        state: 0,
                        wrapper_type: Some(node_type_idx),
                        via: Some(queue_idx - 1),
                    });
                }
            }
            None
        })
        .flatten()
    }

    fn compatible(self, other: Self) -> bool {
        with_types(|store| {
            let expr_a = &store.content_exprs[self.expr_idx];
            let expr_b = &store.content_exprs[other.expr_idx];
            for i in 0..expr_a.edge_count(self.state) {
                let (name_a, _) = expr_a.edge(self.state, i)?;
                for j in 0..expr_b.edge_count(other.state) {
                    let (name_b, _) = expr_b.edge(other.state, j)?;
                    if name_a == name_b {
                        return Some(true);
                    }
                }
            }
            Some(false)
        })
        .flatten()
        .unwrap_or(false)
    }

    fn inline_content(self) -> bool {
        with_types(|store| {
            let expr = &store.content_exprs[self.expr_idx];
            for i in 0..expr.edge_count(self.state) {
                if let Some((name, _)) = expr.edge(self.state, i) {
                    if name == "text" || name == "hard_break" || name == "image" {
                        return true;
                    }
                }
            }
            false
        })
        .unwrap_or(false)
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
                    return Some((
                        DynamicNodeType { idx: i },
                        DynamicContentMatch {
                            expr_idx: self.expr_idx,
                            state: next_state,
                        },
                    ));
                }
            }
            None
        })
        .flatten()
    }
}

/// A `ContentMatch`-like object created by parsing a content expression
/// string against a specific schema's node types. Unlike `DynamicContentMatch`,
/// this holds the parsed `ContentExpr` directly and does not rely on the
/// global `DYN_TYPES` thread-local.
#[derive(Clone)]
pub struct ParsedContentMatch {
    expr: ContentExpr,
    schema: std::sync::Arc<DynamicSchema>,
    state: usize,
}

impl fmt::Debug for ParsedContentMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedContentMatch")
            .field("expr", &self.expr)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl ParsedContentMatch {
    /// Parse a content expression string using the node types from the given schema.
    pub fn parse(
        expr_str: &str,
        schema: &std::sync::Arc<DynamicSchema>,
    ) -> Result<Self, ContentExprError> {
        let mut groups: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (group_name, indices) in schema.node_groups.iter() {
            let names: Vec<String> = indices
                .iter()
                .map(|&i| schema.node_types[i].name.clone())
                .collect();
            groups.insert(group_name.clone(), names);
        }

        let node_type_names: std::collections::HashSet<String> =
            schema.node_types.iter().map(|nt| nt.name.clone()).collect();

        let expr =
            crate::dynamic::content_expr::parse_content_expr(expr_str, &groups, &node_type_names)?;
        Ok(ParsedContentMatch {
            expr,
            schema: schema.clone(),
            state: 0,
        })
    }

    /// Whether this match state represents a valid end of the content expression.
    pub fn valid_end(&self) -> bool {
        self.expr.valid_end(self.state)
    }

    /// Match a single node type, returning the next match state if successful.
    pub fn match_type(&self, node_type: DynamicNodeType) -> Option<Self> {
        let name = self.schema.node_types.get(node_type.idx)?.name.clone();
        let next_state = self.expr.match_type(self.state, &name)?;
        Some(ParsedContentMatch {
            expr: self.expr.clone(),
            schema: self.schema.clone(),
            state: next_state,
        })
    }

    /// Match a fragment of nodes, returning the final match state if successful.
    pub fn match_fragment(&self, fragment: &Fragment<Dyn>) -> Option<Self> {
        let mut state = self.state;
        for i in 0..fragment.child_count() {
            let child = fragment.child(i);
            let name = self.schema.node_types.get(child.r#type().idx)?.name.clone();
            state = self.expr.match_type(state, &name)?;
        }
        Some(ParsedContentMatch {
            expr: self.expr.clone(),
            schema: self.schema.clone(),
            state,
        })
    }

    /// Try to find a fragment of nodes that can be inserted before `after`
    /// to make the content match.
    pub fn fill_before(
        &self,
        after: &Fragment<Dyn>,
        to_end: bool,
        start_index: usize,
    ) -> Option<Fragment<Dyn>> {
        self.schema.with_types(|| {
            with_types(|store| {
                let mut seen = std::collections::HashSet::new();
                seen.insert(self.state);
                let mut types = Vec::new();
                let result = fill_before_search(
                    &self.expr,
                    self.state,
                    after,
                    start_index,
                    to_end,
                    &mut seen,
                    &mut types,
                    store,
                    0,
                )?;
                Some(Fragment::from(result))
            })
            .flatten()
        })
    }

    /// The number of outgoing edges from the current DFA state.
    pub fn edge_count(&self) -> usize {
        self.expr.edge_count(self.state)
    }

    /// The n-th outgoing edge: returns `(node_type, next_match_state)`.
    pub fn edge(&self, n: usize) -> Option<(DynamicNodeType, Self)> {
        let (name, next_state) = self.expr.edge(self.state, n)?;
        let node_type_idx = self
            .schema
            .node_types
            .iter()
            .position(|nt| nt.name == name)?;
        Some((
            DynamicNodeType { idx: node_type_idx },
            ParsedContentMatch {
                expr: self.expr.clone(),
                schema: self.schema.clone(),
                state: next_state,
            },
        ))
    }

    /// The default node type for filling a position (the first edge whose
    /// node type has no required attributes and whose own content match can
    /// reach a valid end).
    pub fn default_type(&self) -> Option<DynamicNodeType> {
        for i in 0..self.edge_count() {
            let Some((nt, _)) = self.edge(i) else {
                continue;
            };
            let Some(data) = self.schema.node_types.get(nt.idx) else {
                continue;
            };
            if !data.has_required_attrs {
                return Some(nt);
            }
        }
        None
    }

    /// BFS-based wrapping search (mirrors JS `ContentMatch.findWrapping`).
    /// Returns the shortest list of wrapper node types needed to wrap `target`
    /// inside the current content match position, or `None` if impossible.
    pub fn find_wrapping(&self, target: DynamicNodeType) -> Option<Vec<DynamicNodeType>> {
        let target_name = self.schema.node_types.get(target.idx)?.name.clone();
        if self.expr.match_type(self.state, &target_name).is_some() {
            return Some(Vec::new());
        }
        // BFS through wrapper types.
        struct WrapEntry {
            expr: crate::dynamic::content_expr::ContentExpr,
            state: usize,
            wrapper_type: Option<usize>,
            via: Option<usize>,
        }
        let mut active: Vec<WrapEntry> = Vec::new();
        active.push(WrapEntry {
            expr: self.expr.clone(),
            state: self.state,
            wrapper_type: None,
            via: None,
        });
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue_idx = 0;
        while queue_idx < active.len() {
            let current_state = active[queue_idx].state;
            let current_wrapper = active[queue_idx].wrapper_type;
            let current_via = active[queue_idx].via;
            let current_expr = active[queue_idx].expr.clone();
            queue_idx += 1;
            if current_expr
                .match_type(current_state, &target_name)
                .is_some()
            {
                // Reconstruct the wrapper path.
                let mut result = Vec::new();
                if let Some(idx) = current_wrapper {
                    result.push(DynamicNodeType { idx });
                }
                let mut next_via = current_via;
                while let Some(v) = next_via {
                    let entry = &active[v];
                    if let Some(idx) = entry.wrapper_type {
                        result.push(DynamicNodeType { idx });
                    }
                    next_via = entry.via;
                }
                result.reverse();
                return Some(result);
            }
            for i in 0..current_expr.edge_count(current_state) {
                let Some((name, next_state)) = current_expr.edge(current_state, i) else {
                    continue;
                };
                let Some(node_type_idx) =
                    self.schema.node_types.iter().position(|nt| nt.name == name)
                else {
                    continue;
                };
                let nt = &self.schema.node_types[node_type_idx];
                if nt.atom || nt.has_required_attrs {
                    continue;
                }
                if !seen.insert(name.to_string()) {
                    continue;
                }
                if current_wrapper.is_some() && !current_expr.valid_end(next_state) {
                    continue;
                }
                let wrapper_expr = self.schema.content_exprs.get(nt.content_expr_idx)?.clone();
                active.push(WrapEntry {
                    expr: wrapper_expr,
                    state: 0,
                    wrapper_type: Some(node_type_idx),
                    via: Some(queue_idx - 1),
                });
            }
        }
        None
    }

    /// Create a `ParsedContentMatch` from a `DynamicContentMatch`, cloning
    /// the relevant `ContentExpr` from the thread-local type store.
    /// **Must be called while the thread-local store is active** (i.e., inside
    /// a `schema.with_types(|| …)` closure).
    pub fn from_dynamic(
        dcm: DynamicContentMatch,
        schema: std::sync::Arc<DynamicSchema>,
    ) -> Option<Self> {
        with_types(|store| {
            let expr = store.content_exprs.get(dcm.expr_idx)?.clone();
            Some(ParsedContentMatch {
                expr,
                schema: schema.clone(),
                state: dcm.state,
            })
        })
        .flatten()
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_before_search(
    expr: &ContentExpr,
    state: usize,
    after: &Fragment<Dyn>,
    start_index: usize,
    to_end: bool,
    seen: &mut HashSet<usize>,
    types: &mut Vec<usize>,
    store: &DynTypeStore,
    depth: usize,
) -> Option<Vec<DynamicNode>> {
    if depth > 50 {
        panic!(
            "fill_before_search recursion too deep: depth={}, state={}, types={:?}",
            depth,
            state,
            types
                .iter()
                .map(|&i| &store.node_types[i].name)
                .collect::<Vec<_>>()
        );
    }
    // Try to match the after content from this state
    let mut match_state = state;
    let mut can_match = true;
    for i in start_index..after.child_count() {
        if let Some(child) = after.maybe_child(i) {
            let name = &store.node_types[child.r#type().idx].name;
            match expr.match_type(match_state, name) {
                Some(next) => match_state = next,
                None => {
                    can_match = false;
                    break;
                }
            }
        }
    }
    if can_match && (!to_end || expr.valid_end(match_state)) {
        // Create filler nodes for the accumulated types
        let mut result = Vec::new();
        for &type_idx in types.iter() {
            let attrs = serde_json::to_value(&store.node_types[type_idx].attrs).unwrap_or_default();
            let node_type = DynamicNodeType { idx: type_idx };
            let filler = node_type.create_and_fill(attrs, None, None)?;
            result.push(filler);
        }
        return Some(result);
    }

    // Explore edges
    for i in 0..expr.edge_count(state) {
        let (name, next_state) = expr.edge(state, i)?;
        let node_type_idx = store.node_types.iter().position(|nt| nt.name == name)?;

        // Skip text nodes and nodes with required attrs
        if name == "text" || store.node_types[node_type_idx].has_required_attrs {
            continue;
        }

        if !seen.insert(next_state) {
            continue;
        }

        types.push(node_type_idx);
        if let Some(result) = fill_before_search(
            expr,
            next_state,
            after,
            start_index,
            to_end,
            seen,
            types,
            store,
            depth + 1,
        ) {
            return Some(result);
        }
        types.pop();
    }

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

    fn find_linebreak_replacement_type(&self) -> Option<DynamicNodeType> {
        with_types(|store| {
            store
                .node_types
                .iter()
                .position(|nt| nt.linebreak_replacement)
                .map(|idx| DynamicNodeType { idx })
        })
        .flatten()
    }
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
                store
                    .node_types
                    .get(self.type_idx)
                    .map(|nt| nt.attrs.clone())
            })
            .flatten()
            .unwrap_or_default();

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
                obj.as_object_mut().unwrap().insert(
                    "text".to_string(),
                    serde_json::Value::String(tn.text.as_str().to_string()),
                );
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
        let mini_marks: Vec<serde_json::Value> =
            self.marks.iter().map(|m| m.to_mini_json()).collect();
        if !mini_marks.is_empty() {
            obj.as_object_mut()
                .unwrap()
                .insert("marks".to_string(), serde_json::Value::Array(mini_marks));
        }

        obj
    }

    /// Return a ProseMirror-style debug string for this node.
    ///
    /// Format: `type_name(child1, child2)` with marks wrapped outer-most.
    /// Used by language binding `__str__` / `toString` implementations.
    pub fn to_debug_string(&self) -> String {
        fn wrap_marks(name: &str, marks: &crate::model::MarkSet<Dyn>) -> String {
            let mut result = name.to_string();
            for m in marks.iter().rev() {
                result = format!("{}({})", m.type_name, result);
            }
            result
        }

        let name = &self.type_name;
        if let Some(tn) = self.text_node() {
            let text = tn.text.as_str().to_string();
            return wrap_marks(&format!("\"{text}\""), &self.marks);
        }
        if let Some(content) = self.content() {
            let inner = content
                .children()
                .iter()
                .map(|child| child.to_debug_string())
                .collect::<Vec<_>>()
                .join(", ");
            if inner.is_empty() {
                wrap_marks(name, &self.marks)
            } else {
                wrap_marks(&format!("{name}({inner})"), &self.marks)
            }
        } else {
            wrap_marks(name, &self.marks)
        }
    }
}

fn attrs_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Null, serde_json::Value::Object(m)) if m.is_empty() => true,
        (serde_json::Value::Object(m), serde_json::Value::Null) if m.is_empty() => true,
        _ => a == b,
    }
}

impl PartialEq for DynamicNode {
    fn eq(&self, other: &Self) -> bool {
        self.type_idx == other.type_idx
            && attrs_eq(&self.attrs, &other.attrs)
            && self.marks == other.marks
            && match (&self.inner, &other.inner) {
                (DynNodeInner::Element { content: a }, DynNodeInner::Element { content: b }) => {
                    a == b
                }
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
                if nt.name == helper.type_name {
                    return Some(i);
                }
            }
            None
        })
        .flatten()
        .unwrap_or(0);

        let marks_vec = helper.marks;
        let mut marks_set = MarkSet::new();
        for m in &marks_vec {
            marks_set.add(m);
        }

        // Normalize missing/null attrs to an empty object, matching ProseMirror JS behavior
        let attrs = if helper.attrs.is_null() {
            serde_json::Value::Object(Default::default())
        } else {
            helper.attrs
        };

        if let Some(text) = helper.text {
            let text_obj = Text::from(text);
            Ok(DynamicNode {
                type_idx,
                type_name: helper.type_name,
                attrs: attrs.clone(),
                marks: marks_set.clone(),
                inner: DynNodeInner::Text(TextNode {
                    text: text_obj,
                    marks: marks_set,
                }),
            })
        } else {
            let frag = Fragment::from(helper.content);
            Ok(DynamicNode {
                type_idx,
                type_name: helper.type_name,
                attrs,
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

        // Normalize missing/null attrs to an empty object, matching ProseMirror JS behavior
        let attrs = if helper.attrs.is_null() {
            serde_json::Value::Object(Default::default())
        } else {
            helper.attrs
        };

        Ok(DynamicMark {
            type_name: helper.type_name,
            attrs,
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
                store
                    .mark_types
                    .iter()
                    .find(|mt| mt.name == self.type_name)
                    .map(|mt| mt.attrs.clone())
            })
            .flatten()
            .unwrap_or_default();

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
    fn eq(&self, other: &Self) -> bool {
        self.type_name == other.type_name && attrs_eq(&self.attrs, &other.attrs)
    }
}
impl Eq for DynamicMark {}
impl Hash for DynamicMark {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_name.hash(state);
        // Normalize null to empty object for hashing consistency with eq
        let normalized = if self.attrs.is_null() {
            &serde_json::Value::Object(Default::default())
        } else {
            &self.attrs
        };
        normalized.hash(state);
    }
}

// ---------------------------------------------------------------------------
// NodeType impl
// ---------------------------------------------------------------------------

impl NodeType<Dyn> for DynamicNodeType {
    fn compatible_content(self, other: Self) -> bool {
        self.idx == other.idx || self.content_match().compatible(other.content_match())
    }

    fn valid_content(self, fragment: &Fragment<Dyn>) -> bool {
        with_types(|store| {
            let expr = &store.content_exprs[store.node_types[self.idx].content_expr_idx];
            let nt = &store.node_types[self.idx];
            let mut state = 0;
            for child in fragment.children() {
                match expr.match_type(state, &store.node_types[child.r#type().idx].name) {
                    Some(next) => state = next,
                    None => return false,
                }
                if let Some(allowed) = &nt.allowed_marks {
                    for mark in child.marks().unwrap_or(&MarkSet::new()).iter() {
                        let mark_data = &store.mark_types[mark.r#type().idx];
                        // allowed_marks stores raw spec entries which may be mark type
                        // names *or* mark group names (e.g. "annotation", "track").
                        // A mark is allowed if its own name OR any of its groups appears
                        // in the allowed list — matching upstream ProseMirror behaviour.
                        let ok = allowed.contains(&mark_data.name)
                            || mark_data.groups.iter().any(|g| allowed.contains(g));
                        if !ok {
                            return false;
                        }
                    }
                }
            }
            expr.valid_end(state)
        })
        .unwrap_or(false)
    }

    fn allows_mark_type(self, mark_type: DynamicMarkType) -> bool {
        with_types(|store| {
            let nt = &store.node_types[self.idx];
            match &nt.allowed_marks {
                Some(allowed) => {
                    let mark_data = &store.mark_types[mark_type.idx];
                    allowed.contains(&mark_data.name)
                        || mark_data.groups.iter().any(|g| allowed.contains(g))
                }
                None => true,
            }
        })
        .unwrap_or(true)
    }

    fn content_match(self) -> DynamicContentMatch {
        with_types(|store| DynamicContentMatch {
            expr_idx: store.node_types[self.idx].content_expr_idx,
            state: 0,
        })
        .unwrap_or(DynamicContentMatch {
            expr_idx: 0,
            state: 0,
        })
    }

    fn allow_marks(self, marks: &MarkSet<Dyn>) -> bool {
        with_types(|store| {
            let nt = &store.node_types[self.idx];
            match &nt.allowed_marks {
                Some(allowed) => {
                    for mark in marks {
                        let mark_data = &store.mark_types[mark.r#type().idx];
                        let ok = allowed.contains(&mark_data.name)
                            || mark_data.groups.iter().any(|g| allowed.contains(g));
                        if !ok {
                            return false;
                        }
                    }
                    true
                }
                None => true,
            }
        })
        .unwrap_or(true)
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
    fn is_isolating(self) -> bool {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .is_some_and(|node_type| node_type.isolating)
        })
        .unwrap_or(false)
    }
    fn has_required_attrs(self) -> bool {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .is_some_and(|node_type| node_type.has_required_attrs)
        })
        .unwrap_or(false)
    }
    fn is_textblock(self) -> bool {
        with_types(|store| store.node_types[self.idx].textblock).unwrap_or(false)
    }
    fn inline_content(self) -> bool {
        with_types(|store| store.node_types[self.idx].has_inline_content).unwrap_or(false)
    }
    fn is_defining(self) -> bool {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .is_some_and(|node_type| node_type.defining)
        })
        .unwrap_or(false)
    }
    fn is_defining_as_context(self) -> bool {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .is_some_and(|node_type| node_type.defining_as_context)
        })
        .unwrap_or(false)
    }
    fn is_defining_for_content(self) -> bool {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .is_some_and(|node_type| node_type.defining_for_content)
        })
        .unwrap_or(false)
    }
    fn whitespace(self) -> Option<String> {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .and_then(|node_type| node_type.whitespace.clone())
        })
        .flatten()
    }

    fn linebreak_replacement(self) -> bool {
        with_types(|store| {
            store
                .node_types
                .get(self.idx)
                .map(|node_type| node_type.linebreak_replacement)
                .unwrap_or(false)
        })
        .unwrap_or(false)
    }

    fn check_attrs(self, attrs: &serde_json::Value) -> Result<(), String> {
        with_types(|store| {
            if let Some(nt) = store.node_types.get(self.idx) {
                check_attrs_helper(&nt.attrs, &nt.attr_validators, attrs, "node", &nt.name)
            } else {
                Ok(())
            }
        })
        .unwrap_or(Ok(()))
    }

    fn create_node(
        self,
        content: Option<&Fragment<Dyn>>,
        marks: Option<&MarkSet<Dyn>>,
    ) -> DynamicNode {
        let (attrs, name) = with_types(|store| {
            let nt = &store.node_types[self.idx];
            (
                serde_json::to_value(&nt.attrs).unwrap_or_default(),
                nt.name.clone(),
            )
        })
        .unwrap_or_default();
        DynamicNode {
            type_idx: self.idx,
            type_name: name,
            attrs,
            marks: marks.cloned().unwrap_or_default(),
            inner: DynNodeInner::Element {
                content: content.cloned().unwrap_or_default(),
            },
        }
    }

    fn create(
        self,
        attrs: serde_json::Value,
        content: Option<&Fragment<Dyn>>,
        marks: Option<&MarkSet<Dyn>>,
    ) -> DynamicNode {
        let mut node = self.create_node(content, marks);
        if let serde_json::Value::Object(map) = attrs {
            for (key, value) in map {
                node = node.with_attr(&key, value);
            }
        }
        node
    }
}

// ---------------------------------------------------------------------------
// Mark impl
// ---------------------------------------------------------------------------

impl Mark<Dyn> for DynamicMark {
    fn r#type(&self) -> DynamicMarkType {
        with_types(|store| {
            for (i, mt) in store.mark_types.iter().enumerate() {
                if mt.name == self.type_name {
                    return DynamicMarkType { idx: i };
                }
            }
            DynamicMarkType { idx: 0 }
        })
        .unwrap_or(DynamicMarkType { idx: 0 })
    }

    fn attrs_json(&self) -> serde_json::Value {
        self.attrs.clone()
    }
}

// ---------------------------------------------------------------------------
// Node impl
// ---------------------------------------------------------------------------

impl Node<Dyn> for DynamicNode {
    fn text_node(&self) -> Option<&TextNode<Dyn>> {
        match &self.inner {
            DynNodeInner::Text(tn) => Some(tn),
            _ => None,
        }
    }

    fn new_text_node(node: TextNode<Dyn>) -> Self {
        let type_idx = with_types(|store| {
            store
                .node_types
                .iter()
                .position(|nt| nt.name == "text")
                .unwrap_or(0)
        })
        .unwrap_or(0);
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
            store
                .node_types
                .iter()
                .position(|nt| nt.name == "text")
                .unwrap_or(0)
        })
        .unwrap_or(0);
        DynamicNode {
            type_idx,
            type_name: "text".to_string(),
            attrs: serde_json::Value::Null,
            marks: MarkSet::new(),
            inner: DynNodeInner::Text(TextNode {
                text: Text::from(s),
                marks: MarkSet::new(),
            }),
        }
    }

    fn content(&self) -> Option<&Fragment<Dyn>> {
        match &self.inner {
            DynNodeInner::Element { content } => Some(content),
            _ => None,
        }
    }

    fn marks(&self) -> Option<&MarkSet<Dyn>> {
        Some(&self.marks)
    }

    fn r#type(&self) -> DynamicNodeType {
        DynamicNodeType { idx: self.type_idx }
    }

    fn find_linebreak_replacement_type(&self) -> Option<DynamicNodeType> {
        Dyn.find_linebreak_replacement_type()
    }

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
                    inner: DynNodeInner::Element {
                        content: new_content,
                    },
                }
            }
        }
    }

    fn attrs_json(&self) -> serde_json::Value {
        self.attrs.clone()
    }

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
                if self.is_leaf() {
                    1
                } else {
                    content.size() + 2
                }
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

    fn is_text(&self) -> bool {
        matches!(self.inner, DynNodeInner::Text(_))
    }
    fn is_leaf(&self) -> bool {
        match &self.inner {
            DynNodeInner::Text(_) => true,
            DynNodeInner::Element { .. } => {
                // A node is a leaf only if its *type* cannot hold any children —
                // i.e. the content-expression DFA has no outgoing edges from
                // the start state.  An empty-but-nullable type (e.g. `text*`)
                // is NOT a leaf even when it currently contains no children.
                with_types(|store| {
                    store
                        .node_types
                        .get(self.type_idx)
                        .and_then(|t| store.content_exprs.get(t.content_expr_idx))
                        .map(|expr| expr.states.first().is_none_or(|s| s.edges.is_empty()))
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
            store
                .node_types
                .iter()
                .position(|nt| nt.name == "text")
                .unwrap_or(0)
        })
        .unwrap_or(0);
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
