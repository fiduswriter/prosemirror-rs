//! Dynamic schema constructed from a JSON SchemaSpec.

use crate::dynamic::content_expr::{self, ContentExpr};
use crate::dynamic::types::{
    DynamicMark, DynamicMarkType, DynamicNode, DynamicNodeType, DynamicNodeTypeData,
    DynamicMarkTypeData, DynTypeStore, DYN_TYPES,
};
use crate::model::{Fragment, MarkSet, Node, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Errors during dynamic schema construction.
#[derive(Debug)]
pub enum DynamicSchemaError {
    /// A content expression could not be parsed
    ContentExpr(content_expr::ContentExprError),
    /// A reference to an unknown node type
    UnknownNodeType(String),
    /// Invalid JSON structure
    InvalidSpec(String),
}

impl std::fmt::Display for DynamicSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContentExpr(e) => write!(f, "Content expression error: {}", e),
            Self::UnknownNodeType(name) => write!(f, "Unknown node type: {}", name),
            Self::InvalidSpec(msg) => write!(f, "Invalid spec: {}", msg),
        }
    }
}

impl std::error::Error for DynamicSchemaError {}

/// A JSON-serializable schema specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSpec {
    /// Node type specifications, keyed by name
    pub nodes: HashMap<String, NodeSpec>,
    /// Mark type specifications, keyed by name
    #[serde(default)]
    pub marks: HashMap<String, MarkSpec>,
    /// The name of the top-level node type (default: "doc")
    #[serde(default = "default_top_node", alias = "topNode")]
    pub top_node: String,
}

fn default_top_node() -> String { "doc".to_string() }

/// Specification for a single node type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpec {
    /// Content expression (e.g. "block+", "inline*")
    #[serde(default)]
    pub content: String,
    /// Group(s) this node belongs to (space-separated)
    #[serde(default)]
    pub group: String,
    /// Marks allowed in this node's content ("_" for all marks, "" for no marks)
    #[serde(default)]
    pub marks: Option<String>,
    /// Attribute specifications
    #[serde(default)]
    pub attrs: Option<HashMap<String, AttributeSpec>>,
    /// Whether this is an inline node
    #[serde(default)]
    pub inline: bool,
    /// Whether this is an atom (leaf) node
    #[serde(default)]
    pub atom: bool,
    /// Whether this is a defining node
    #[serde(default)]
    pub defining: bool,
    /// Whether this is isolating
    #[serde(default)]
    pub isolating: bool,
    /// Whether this is a code block
    #[serde(default)]
    pub code: bool,
    /// Whether this is draggable
    #[serde(default)]
    pub draggable: bool,
    /// Whether this node is selectable
    #[serde(default = "default_true")]
    pub selectable: bool,
    /// Whitespace handling mode
    #[serde(default)]
    pub whitespace: Option<String>,
}

fn default_true() -> bool { true }

fn split_space_separated_names(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|name| name.to_string())
        .collect()
}

/// Specification for an attribute.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttributeSpec {
    /// The default value for this attribute
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// Specification for a single mark type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSpec {
    /// Attribute specifications
    #[serde(default)]
    pub attrs: Option<HashMap<String, AttributeSpec>>,
    /// Whether this mark is inclusive
    #[serde(default = "default_true")]
    pub inclusive: bool,
    /// Which other marks this one excludes (defaults to this mark type itself)
    #[serde(default)]
    pub excludes: Option<String>,
    /// Group(s) this mark belongs to
    #[serde(default)]
    pub group: String,
    /// Whether this is a spanning mark
    #[serde(default = "default_true")]
    pub spanning: bool,
}

/// A fully compiled dynamic schema.
pub struct DynamicSchema {
    /// The node types in this schema
    pub node_types: Vec<DynamicNodeTypeData>,
    /// Map from node type name to index
    pub node_type_map: HashMap<String, usize>,
    /// The mark types in this schema
    pub mark_types: Vec<DynamicMarkTypeData>,
    /// Map from mark type name to index
    pub mark_type_map: HashMap<String, usize>,
    /// Groups: maps group name to list of node type indices
    pub node_groups: HashMap<String, Vec<usize>>,
    /// The name of the top node type
    pub top_node: String,
    /// Stored content expressions (kept alive for pointer stability)
    #[allow(dead_code)]
    content_exprs: Vec<ContentExpr>,
    /// The type store for thread-local access
    store: Box<DynTypeStore>,
}

impl DynamicSchema {
    /// Build a [`DynamicSchema`] from a JSON `SchemaSpec`.
    ///
    /// The JSON object must have a `"nodes"` map (and optionally a `"marks"` map)
    /// following ProseMirror's schema-spec format.
    ///
    /// # Example
    ///
    /// ```
    /// use prosemirror::dynamic::DynamicSchema;
    /// use prosemirror::model::Node;
    ///
    /// let schema = DynamicSchema::from_json(&serde_json::json!({
    ///     "nodes": {
    ///         "doc":       { "content": "paragraph+" },
    ///         "paragraph": { "content": "text*", "group": "block" },
    ///         "text":      { "group": "inline" }
    ///     },
    ///     "marks": {}
    /// })).unwrap();
    ///
    /// let text = schema.with_types(|| {
    ///     let doc = schema.node_from_json(&serde_json::json!({
    ///         "type": "doc",
    ///         "content": [{
    ///             "type": "paragraph",
    ///             "content": [{"type": "text", "text": "hello"}]
    ///         }]
    ///     })).unwrap();
    ///     doc.text_content()
    /// });
    ///
    /// assert_eq!(text, "hello");
    /// ```
    pub fn from_json(json: &serde_json::Value) -> Result<Self, DynamicSchemaError> {
        let spec: SchemaSpec = serde_json::from_value(json.clone())
            .map_err(|e| DynamicSchemaError::InvalidSpec(e.to_string()))?;
        Self::from_spec(spec)
    }

    /// Build a schema from a parsed spec.
    pub fn from_spec(spec: SchemaSpec) -> Result<Self, DynamicSchemaError> {
        for name in spec.nodes.keys() {
            if spec.marks.contains_key(name) {
                return Err(DynamicSchemaError::InvalidSpec(format!(
                    "schema item name `{}` is used for both a node and a mark",
                    name
                )));
            }
        }

        if !spec.nodes.contains_key(&spec.top_node) {
            return Err(DynamicSchemaError::UnknownNodeType(spec.top_node.clone()));
        }

        let mut node_types_data = Vec::new();
        let mut node_type_map = HashMap::new();
        let mut node_groups: HashMap<String, Vec<usize>> = HashMap::new();
        let mut content_exprs = Vec::new();

        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for (name, node_spec) in &spec.nodes {
            if !node_spec.group.is_empty() {
                for g in node_spec.group.split(' ') {
                    groups.entry(g.to_string()).or_default().push(name.clone());
                }
            }
        }

        for (name, node_spec) in &spec.nodes {
            let idx = node_types_data.len();
            let content_expr = if node_spec.content.is_empty() {
                ContentExpr::empty()
            } else {
                content_expr::parse_content_expr(&node_spec.content, &groups)
                    .map_err(DynamicSchemaError::ContentExpr)?
            };
            content_exprs.push(content_expr);
            let content_expr_idx = content_exprs.len() - 1;

            let has_inline_content = !node_spec.content.is_empty()
                && (node_spec.content.contains("text") || node_spec.content.contains("inline"));
            let is_textblock =
                has_inline_content && (node_spec.inline || (name != "doc" && name != "blockquote"));
            let allowed_marks = match node_spec.marks.as_deref() {
                Some("_") => None,
                Some("") => Some(Vec::new()),
                Some(marks) => Some(split_space_separated_names(marks)),
                None => None,
            };
            let attrs = node_spec
                .attrs
                .as_ref()
                .map(|a| {
                    a.iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                v.default.clone().unwrap_or(serde_json::Value::Null),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let groups_list: Vec<String> = node_spec
                .group
                .split(' ')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            for g in &groups_list {
                node_groups.entry(g.clone()).or_default().push(idx);
            }
            node_type_map.insert(name.clone(), idx);
            node_types_data.push(DynamicNodeTypeData {
                name: name.clone(),
                // Text nodes are always inline in ProseMirror, even without explicit `inline: true`
                inline: node_spec.inline || name == "text",
                atom: node_spec.atom,
                textblock: is_textblock, has_inline_content, content_expr_idx,
                groups: groups_list, attrs, allowed_marks,
            });
        }

        let mut mark_types_data = Vec::new();
        let mut mark_type_map = HashMap::new();
        for (name, mark_spec) in &spec.marks {
            let idx = mark_types_data.len();
            let attrs = mark_spec
                .attrs
                .as_ref()
                .map(|a| {
                    a.iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                v.default.clone().unwrap_or(serde_json::Value::Null),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let excludes = match mark_spec.excludes.as_deref() {
                Some("") => Vec::new(),
                Some("_") => {
                    let mut names: Vec<_> = spec.marks.keys().cloned().collect();
                    names.sort();
                    names
                }
                Some(excludes) => split_space_separated_names(excludes),
                None => vec![name.clone()],
            };
            let groups_list: Vec<String> = mark_spec
                .group
                .split(' ')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            mark_type_map.insert(name.clone(), idx);
            mark_types_data.push(DynamicMarkTypeData {
                name: name.clone(), attrs, inclusive: mark_spec.inclusive, excludes, groups: groups_list,
            });
        }

        let store = Box::new(DynTypeStore {
            node_types: node_types_data.clone(),
            mark_types: mark_types_data.clone(),
            content_exprs: content_exprs.clone(),
        });

        Ok(DynamicSchema {
            node_types: node_types_data, node_type_map, mark_types: mark_types_data,
            mark_type_map, node_groups, top_node: spec.top_node, content_exprs, store,
        })
    }

    /// Set up the thread-local type store so that `DynamicNodeType` etc. can work.
    /// If the store is already set (nested call), this is a no-op that just runs the closure.
    pub fn with_types<R>(&self, f: impl FnOnce() -> R) -> R {
        let store_ref: &DynTypeStore = &self.store;
        let store_static: &'static DynTypeStore = unsafe { std::mem::transmute(store_ref) };
        let already_set = DYN_TYPES.with(|cell| {
            let already = cell.borrow().is_some();
            if !already {
                cell.borrow_mut().replace(store_static);
            }
            already
        });
        let result = f();
        if !already_set {
            DYN_TYPES.with(|cell| { cell.borrow_mut().take(); });
        }
        result
    }

    /// Get a node type by name.
    pub fn node_type(&self, name: &str) -> Option<DynamicNodeType> {
        self.node_type_map.get(name).map(|&idx| DynamicNodeType { idx })
    }

    /// Get a mark type by name.
    pub fn mark_type(&self, name: &str) -> Option<DynamicMarkType> {
        self.mark_type_map.get(name).map(|&idx| DynamicMarkType { idx })
    }

    /// Create a node from a JSON value.
    pub fn node_from_json(&self, json: &serde_json::Value) -> Result<DynamicNode, DynamicSchemaError> {
        self.with_types(|| {
            serde_json::from_value::<DynamicNode>(json.clone())
                .map_err(|e| DynamicSchemaError::InvalidSpec(e.to_string()))
        })
    }

    /// Create a mark from a JSON value.
    pub fn mark_from_json(&self, json: &serde_json::Value) -> Result<DynamicMark, DynamicSchemaError> {
        self.with_types(|| {
            serde_json::from_value::<DynamicMark>(json.clone())
                .map_err(|e| DynamicSchemaError::InvalidSpec(e.to_string()))
        })
    }

    /// Create a text node with the given text.
    pub fn text(&self, text: &str) -> DynamicNode {
        self.with_types(|| DynamicNode::text(text))
    }

    /// Create a node of the given type with content and marks.
    pub fn node(
        &self, type_name: &str, _attrs: serde_json::Value,
        content: Fragment<crate::dynamic::types::Dyn>, marks: MarkSet<crate::dynamic::types::Dyn>,
    ) -> Result<DynamicNode, DynamicSchemaError> {
        let idx = self.node_type_map.get(type_name)
            .copied()
            .ok_or_else(|| DynamicSchemaError::UnknownNodeType(type_name.to_string()))?;
        self.with_types(|| {
            let nt = DynamicNodeType { idx };
            Ok(nt.create_node(Some(&content), Some(&marks)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentMatch, Mark, NodeType};

    fn basic_spec_json() -> serde_json::Value {
        serde_json::json!({
            "nodes": {
                "doc": { "content": "block+" },
                "paragraph": { "content": "inline*", "group": "block" },
                "heading": {
                    "attrs": { "level": { "default": 1 } },
                    "content": "inline*",
                    "group": "block",
                    "defining": true
                },
                "text": { "group": "inline" },
                "image": {
                    "inline": true,
                    "attrs": { "src": {}, "alt": { "default": null }, "title": { "default": null } },
                    "group": "inline",
                    "atom": true
                },
                "horizontal_rule": { "group": "block" },
                "hard_break": { "inline": true, "group": "inline" }
            },
            "marks": {
                "strong": {},
                "em": {},
                "link": { "attrs": { "href": {}, "title": { "default": null } }, "inclusive": false }
            }
        })
    }

    #[test]
    fn test_schema_from_json() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        assert!(schema.node_type("doc").is_some());
        assert!(schema.node_type("paragraph").is_some());
        assert!(schema.node_type("heading").is_some());
        assert!(schema.node_type("text").is_some());
        assert!(schema.node_type("nonexistent").is_none());
        assert_eq!(schema.node_types.len(), 7);
        assert_eq!(schema.mark_types.len(), 3);
        let heading = &schema.node_types[schema.node_type_map["heading"]];
        assert!(heading.attrs.contains_key("level"));
        assert_eq!(heading.attrs["level"], serde_json::json!(1));
    }

    fn schema_error(json: serde_json::Value) -> DynamicSchemaError {
        match DynamicSchema::from_json(&json) {
            Ok(_) => panic!("schema should fail"),
            Err(err) => err,
        }
    }

    #[test]
    fn test_schema_accepts_camel_case_top_node() {
        let schema = DynamicSchema::from_json(&serde_json::json!({
            "topNode": "root",
            "nodes": {
                "root": { "content": "paragraph+" },
                "paragraph": { "content": "text*", "group": "block" },
                "text": { "group": "inline" }
            },
            "marks": {}
        }))
        .unwrap();

        assert_eq!(schema.top_node, "root");
        assert!(schema.node_type("root").is_some());
    }

    #[test]
    fn test_schema_rejects_missing_top_node() {
        let err = schema_error(serde_json::json!({
            "topNode": "root",
            "nodes": {
                "doc": { "content": "paragraph+" },
                "paragraph": { "content": "text*", "group": "block" },
                "text": { "group": "inline" }
            },
            "marks": {}
        }));

        assert!(matches!(
            err,
            DynamicSchemaError::UnknownNodeType(name) if name == "root"
        ));
    }

    #[test]
    fn test_schema_rejects_node_mark_name_collisions() {
        let err = schema_error(serde_json::json!({
            "nodes": {
                "doc": { "content": "text*" },
                "text": { "group": "inline" }
            },
            "marks": {
                "text": {}
            }
        }));

        assert!(matches!(
            err,
            DynamicSchemaError::InvalidSpec(message)
                if message.contains("both a node and a mark") && message.contains("text")
        ));
    }

    #[test]
    fn test_mark_spec_defaults_and_marks_sentinels() {
        let schema = DynamicSchema::from_json(&serde_json::json!({
            "nodes": {
                "doc": { "content": "block+" },
                "paragraph": { "content": "text*", "group": "block" },
                "code_block": { "content": "text*", "marks": "", "group": "block" },
                "all_marks_block": { "content": "text*", "marks": "_", "group": "block" },
                "text": { "group": "inline" }
            },
            "marks": {
                "strong": {},
                "em": { "excludes": "" },
                "link": { "excludes": "strong em" },
                "comment": { "excludes": "_" }
            }
        }))
        .unwrap();

        let strong_data = &schema.mark_types[schema.mark_type_map["strong"]];
        let em_data = &schema.mark_types[schema.mark_type_map["em"]];
        let link_data = &schema.mark_types[schema.mark_type_map["link"]];
        let comment_data = &schema.mark_types[schema.mark_type_map["comment"]];
        assert_eq!(strong_data.excludes, vec!["strong".to_string()]);
        assert!(em_data.excludes.is_empty());
        assert_eq!(
            link_data.excludes,
            vec!["strong".to_string(), "em".to_string()]
        );
        assert_eq!(
            comment_data.excludes,
            vec![
                "comment".to_string(),
                "em".to_string(),
                "link".to_string(),
                "strong".to_string()
            ]
        );

        schema.with_types(|| {
            let strong = schema.mark_type("strong").unwrap();
            let paragraph = schema.node_type("paragraph").unwrap();
            let code_block = schema.node_type("code_block").unwrap();
            let all_marks_block = schema.node_type("all_marks_block").unwrap();

            assert!(paragraph.allows_mark_type(strong));
            assert!(!code_block.allows_mark_type(strong));
            assert!(all_marks_block.allows_mark_type(strong));
        });
    }

    #[test]
    fn test_node_from_json() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        let doc_json = serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "Hello world" }]
            }]
        });
        let doc = schema.node_from_json(&doc_json).unwrap();
        assert_eq!(doc.r#type().idx, schema.node_type_map["doc"]);
        assert_eq!(doc.child_count(), 1);
        let para = doc.child(0).unwrap();
        assert_eq!(para.r#type().idx, schema.node_type_map["paragraph"]);
        assert_eq!(para.child(0).unwrap().text_content(), "Hello world");
    }

    #[test]
    fn test_mark_from_json_uses_schema_scope() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        let mark = schema
            .mark_from_json(&serde_json::json!({
                "type": "link",
                "attrs": { "href": "https://example.com" }
            }))
            .unwrap();

        assert_eq!(mark.type_name, "link");
        assert_eq!(
            mark.attrs.get("href").and_then(|value| value.as_str()),
            Some("https://example.com")
        );
        schema.with_types(|| {
            assert_eq!(mark.r#type().idx, schema.mark_type_map["link"]);
        });
    }

    #[test]
    fn test_mark_from_json_rejects_unknown_mark_types() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        let err = schema
            .mark_from_json(&serde_json::json!({ "type": "missing" }))
            .unwrap_err();

        assert!(matches!(
            err,
            DynamicSchemaError::InvalidSpec(message)
                if message.contains("Unknown mark type") && message.contains("missing")
        ));
    }

    #[test]
    fn test_node_from_json_rejects_unknown_nested_mark_types() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        let err = schema
            .node_from_json(&serde_json::json!({
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": "Hello",
                    "marks": [{ "type": "missing" }]
                }]
            }))
            .unwrap_err();

        assert!(matches!(
            err,
            DynamicSchemaError::InvalidSpec(message)
                if message.contains("Unknown mark type") && message.contains("missing")
        ));
    }

    #[test]
    fn test_content_matching() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        schema.with_types(|| {
            let doc_type = schema.node_type("doc").unwrap();
            let cm = doc_type.content_match();
            let para_type = schema.node_type("paragraph").unwrap();
            assert!(cm.match_type(para_type).is_some());
            assert!(!cm.valid_end());
        });
    }

    #[test]
    fn test_dynamic_node_type_name_and_atom_flags() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        schema.with_types(|| {
            let paragraph = schema.node_type("paragraph").unwrap();
            let horizontal_rule = schema.node_type("horizontal_rule").unwrap();
            let image = schema.node_type("image").unwrap();

            assert_eq!(paragraph.name(), "paragraph");
            assert_eq!(horizontal_rule.name(), "horizontal_rule");
            assert!(!paragraph.is_atom());
            assert!(horizontal_rule.is_atom());
            assert!(image.is_atom());
        });
    }

    #[test]
    fn test_dynamic_text_nodes_are_inline_leaves_without_schema_scope() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        let text = schema.text("hello");

        assert!(text.is_text());
        assert!(text.is_leaf());
        assert!(text.is_inline());
        assert!(!text.is_block());
    }

    #[test]
    fn test_round_trip() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        let doc_json = serde_json::json!({
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": 2 }, "content": [
                    { "type": "text", "text": "Title" }
                ]},
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "Hello ", "marks": [{"type": "em"}] },
                    { "type": "text", "text": "world", "marks": [{"type": "strong"}] }
                ]}
            ]
        });
        let doc = schema.with_types(|| schema.node_from_json(&doc_json).unwrap());
        assert_eq!(doc.child_count(), 2);
        assert_eq!(doc.child(0).unwrap().attrs["level"], 2);
        assert_eq!(doc.child(0).unwrap().child(0).unwrap().text_content(), "Title");

        // JSON round-trip
        let serialized = serde_json::to_value(&doc).unwrap();
        let doc2 = schema.with_types(|| schema.node_from_json(&serialized).unwrap());
        assert_eq!(doc, doc2);
    }

    #[test]
    fn test_node_size() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        schema.with_types(|| {
            let doc = schema.node_from_json(&serde_json::json!({
                "type": "doc",
                "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "hi" }] }]
            })).unwrap();
            // paragraph("hi") = 2(text) + 2(paragraph tokens) = 4
            // doc = 4(paragraph) + 2(doc tokens) = 6
            assert_eq!(doc.node_size(), 6);
            assert_eq!(doc.content_size(), 4);
        });
    }

    #[test]
    fn test_text_between() {
        let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
        schema.with_types(|| {
            let doc = schema.node_from_json(&serde_json::json!({
                "type": "doc",
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "hello" }] },
                    { "type": "paragraph", "content": [{ "type": "text", "text": "world" }] }
                ]
            })).unwrap();
            assert_eq!(doc.text_content(), "helloworld");
            assert_eq!(doc.child(0).unwrap().text_content(), "hello");
            assert_eq!(doc.child(1).unwrap().text_content(), "world");
        });
    }
}
