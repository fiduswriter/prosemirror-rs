//! # The document transformations
//!
mod attr_step;
pub mod map;
mod mark_step;
mod node_mark_step;
mod replace;
mod replace_step;
mod step;
pub mod structure;
#[allow(clippy::module_inception)]
pub mod transform;

pub use attr_step::{AttrStep, DocAttrStep};
pub use map::{MapResult, Mappable, Mapping, StepMap};
pub use mark_step::{AddMarkStep, RemoveMarkStep};
pub use node_mark_step::{AddNodeMarkStep, RemoveNodeMarkStep};
pub use replace::{close_fragment, covered_depths, replace_step as smart_replace_step};
pub use replace_step::{ReplaceAroundStep, ReplaceStep};
pub use step::{StepError, StepKind, StepResult};
pub use structure::Wrapper;
pub use transform::{MarkOrType, Transform};
pub use util::Span;

mod util;

use crate::model::Schema;
use derivative::Derivative;
use serde::{Deserialize, Serialize};

/// A list of steps
#[allow(type_alias_bounds)]
pub type Steps<S: Schema> = Vec<Step<S>>;

/// Steps that can be applied on a document
#[derive(Derivative, Deserialize, Serialize)]
#[derivative(
    Debug(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = ""),
    Clone(bound = "")
)]
#[serde(bound = "", tag = "stepType", rename_all = "camelCase")]
pub enum Step<S: Schema> {
    /// Replace some content
    Replace(ReplaceStep<S>),
    /// Replace around some content
    ReplaceAround(ReplaceAroundStep<S>),
    /// Add a mark to a span
    AddMark(AddMarkStep<S>),
    /// Remove a mark from a span
    RemoveMark(RemoveMarkStep<S>),
    /// Add a mark to a node
    AddNodeMark(AddNodeMarkStep<S>),
    /// Remove a mark from a node
    RemoveNodeMark(RemoveNodeMarkStep<S>),
    /// Set an attribute on a node
    #[serde(rename = "attr")]
    Attr(AttrStep),
    /// Set an attribute on the document root
    #[serde(rename = "docAttr")]
    DocAttr(DocAttrStep),
}

impl<S: Schema> Step<S> {
    /// Apply the step to the given node
    pub fn apply(&self, doc: &S::Node) -> StepResult<S> {
        match self {
            Self::Replace(r_step) => r_step.apply(doc),
            Self::ReplaceAround(ra_step) => ra_step.apply(doc),
            Self::AddMark(am_step) => am_step.apply(doc),
            Self::RemoveMark(rm_step) => rm_step.apply(doc),
            Self::AddNodeMark(anm_step) => anm_step.apply(doc),
            Self::RemoveNodeMark(rnm_step) => rnm_step.apply(doc),
            Self::Attr(attr_step) => attr_step.apply(doc),
            Self::DocAttr(da_step) => da_step.apply(doc),
        }
    }

    /// Get the step map for this step
    pub fn get_map(&self) -> StepMap {
        match self {
            Self::Replace(r_step) => r_step.get_map(),
            Self::ReplaceAround(ra_step) => ra_step.get_map(),
            Self::AddMark(am_step) => am_step.get_map(),
            Self::RemoveMark(rm_step) => rm_step.get_map(),
            Self::AddNodeMark(anm_step) => anm_step.get_map(),
            Self::RemoveNodeMark(rnm_step) => rnm_step.get_map(),
            Self::Attr(a_step) => a_step.get_map(),
            Self::DocAttr(da_step) => da_step.get_map(),
        }
    }

    /// Get the inverse of this step
    pub fn invert(&self, doc: &S::Node) -> Step<S> {
        match self {
            Self::Replace(r_step) => r_step.invert(doc),
            Self::ReplaceAround(ra_step) => ra_step.invert(doc),
            Self::AddMark(am_step) => am_step.invert(doc),
            Self::RemoveMark(rm_step) => rm_step.invert(doc),
            Self::AddNodeMark(anm_step) => anm_step.invert(doc),
            Self::RemoveNodeMark(rnm_step) => rnm_step.invert(doc),
            Self::Attr(a_step) => a_step.invert(doc),
            Self::DocAttr(da_step) => da_step.invert(doc),
        }
    }

    /// Map this step through a mapping
    pub fn map<M: Mappable>(&self, mapping: &M) -> Option<Step<S>> {
        match self {
            Self::Replace(r_step) => r_step.map(mapping),
            Self::ReplaceAround(ra_step) => ra_step.map(mapping),
            Self::AddMark(am_step) => am_step.map(mapping),
            Self::RemoveMark(rm_step) => rm_step.map(mapping),
            Self::AddNodeMark(anm_step) => anm_step.map(mapping),
            Self::RemoveNodeMark(rnm_step) => rnm_step.map(mapping),
            Self::Attr(a_step) => a_step.map_step(mapping).map(Step::Attr),
            Self::DocAttr(da_step) => da_step.map_step(mapping).map(Step::DocAttr),
        }
    }

    /// Try to merge this step with another
    pub fn merge(&self, other: &Step<S>) -> Option<Step<S>> {
        match self {
            Self::Replace(r_step) => r_step.merge(other),
            Self::ReplaceAround(ra_step) => ra_step.merge(other),
            Self::AddMark(am_step) => am_step.merge(other),
            Self::RemoveMark(rm_step) => rm_step.merge(other),
            Self::AddNodeMark(anm_step) => anm_step.merge(other),
            Self::RemoveNodeMark(rnm_step) => rnm_step.merge(other),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamic::types::Dyn;
    use crate::dynamic::{DynamicNode, DynamicSchema};
    use crate::model::Node;
    use crate::model::{Fragment, Slice};

    fn basic_schema() -> DynamicSchema {
        DynamicSchema::from_json(&serde_json::json!({
            "nodes": {
                "doc": { "content": "block+" },
                "paragraph": { "content": "inline*", "group": "block" },
                "blockquote": { "content": "block+", "group": "block" },
                "heading": { "attrs": { "level": { "default": 1 } }, "content": "inline*", "group": "block" },
                "text": { "group": "inline" },
                "image": { "inline": true, "attrs": { "src": {}, "alt": { "default": null } }, "group": "inline", "atom": true },
                "hard_break": { "inline": true, "group": "inline" }
            },
            "marks": { "strong": {}, "em": {} }
        })).unwrap()
    }

    #[test]
    fn test_attr_step_serialize() {
        let s: Step<Dyn> =
            serde_json::from_str(r#"{"stepType":"attr","pos":1,"attr":"level","value":2}"#)
                .unwrap();
        match &s {
            Step::Attr(a) => {
                assert_eq!(a.pos, 1);
                assert_eq!(a.attr, "level");
            }
            _ => panic!("Expected Attr variant"),
        }
    }

    #[test]
    fn test_doc_attr_step_serialize() {
        let s: Step<Dyn> =
            serde_json::from_str(r#"{"stepType":"docAttr","attr":"title","value":"hello"}"#)
                .unwrap();
        match &s {
            Step::DocAttr(d) => {
                assert_eq!(d.attr, "title");
            }
            _ => panic!("Expected DocAttr variant"),
        }
    }

    #[test]
    fn test_step_get_map() {
        let schema = basic_schema();
        schema.with_types(|| {
            let text_node = schema.text("abc");
            let s = Step::Replace::<Dyn>(ReplaceStep {
                span: Span { from: 5, to: 10 },
                slice: Slice::new(Fragment::from(vec![text_node]), 0, 0),
                structure: false,
            });
            let map = s.get_map();
            assert_eq!(map.ranges, vec![5, 5, 3]);
        });
    }

    #[test]
    fn replace_step_get_map_uses_open_slice_size() {
        let schema = basic_schema();
        schema.with_types(|| {
            let paragraph = schema
                .node_from_json(&serde_json::json!({
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "abc"}]
                }))
                .unwrap();
            let s = Step::Replace::<Dyn>(ReplaceStep {
                span: Span { from: 5, to: 10 },
                slice: Slice::new(Fragment::from(vec![paragraph]), 1, 1),
                structure: false,
            });
            let map = s.get_map();
            assert_eq!(map.ranges, vec![5, 5, 3]);
        });
    }

    #[test]
    fn structural_replace_detects_one_unit_text_content() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": "a"}]
                    }]
                }))
                .unwrap();
            let step = Step::Replace::<Dyn>(ReplaceStep {
                span: Span { from: 1, to: 2 },
                slice: Slice::default(),
                structure: true,
            });

            assert!(matches!(step.apply(&doc), Err(StepError::WouldOverwrite)));
        });
    }

    #[test]
    fn test_step_map_through_mapping() {
        let schema = basic_schema();
        schema.with_types(|| {
            let text_node = schema.text("abc");
            let s = Step::Replace::<Dyn>(ReplaceStep {
                span: Span { from: 5, to: 10 },
                slice: Slice::new(Fragment::from(vec![text_node]), 0, 0),
                structure: false,
            });
            let mut mapping = Mapping::new();
            mapping.append_map(StepMap::new(vec![0, 0, 3]), None);
            let mapped = s.map(&mapping).unwrap();
            match mapped {
                Step::Replace(r) => {
                    assert_eq!(r.span.from, 8);
                    assert_eq!(r.span.to, 13);
                }
                _ => panic!("Expected Replace variant"),
            }
        });
    }

    #[test]
    fn test_doc_attr_step_applies() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": "hello"}]
                    }]
                }))
                .unwrap();

            // Set a doc-level attribute
            let step: Step<Dyn> = Step::DocAttr(DocAttrStep {
                attr: "title".to_string(),
                value: serde_json::Value::String("test document".to_string()),
            });
            let result = step.apply(&doc);
            assert!(result.is_ok());
            let new_doc: DynamicNode = result.unwrap();
            let attrs = new_doc.attrs_json();
            assert_eq!(attrs["title"], "test document");

            // Verify content is preserved
            assert_eq!(new_doc.content_size(), doc.content_size());
            assert_eq!(new_doc.text_content(), "hello");
        });
    }

    #[test]
    fn test_doc_attr_step_replaces_attr() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": "hello"}]
                    }]
                }))
                .unwrap();

            // Set a doc-level attribute
            let step1: Step<Dyn> = Step::DocAttr(DocAttrStep {
                attr: "title".to_string(),
                value: serde_json::Value::String("first".to_string()),
            });
            let doc: DynamicNode = step1.apply(&doc).unwrap();
            assert_eq!(doc.attrs_json()["title"], "first");

            // Replace it
            let step2: Step<Dyn> = Step::DocAttr(DocAttrStep {
                attr: "title".to_string(),
                value: serde_json::Value::String("second".to_string()),
            });
            let doc: DynamicNode = step2.apply(&doc).unwrap();
            assert_eq!(doc.attrs_json()["title"], "second");
        });
    }

    #[test]
    fn test_doc_attr_step_multiple_attrs() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": "hello"}]
                    }]
                }))
                .unwrap();

            // Set multiple doc-level attributes
            let steps = vec![
                (
                    "documentstyle",
                    serde_json::Value::String("elephant".to_string()),
                ),
                ("tracked", serde_json::Value::Bool(false)),
                (
                    "citationstyle",
                    serde_json::Value::String("apa".to_string()),
                ),
                ("language", serde_json::Value::String("en-US".to_string())),
                ("papersize", serde_json::Value::String("A4".to_string())),
                (
                    "template",
                    serde_json::Value::String("Standard Article".to_string()),
                ),
                (
                    "import_id",
                    serde_json::Value::String("standard-article".to_string()),
                ),
            ];

            let mut doc: DynamicNode = doc;
            for (attr, value) in &steps {
                let step: Step<Dyn> = Step::DocAttr(DocAttrStep {
                    attr: attr.to_string(),
                    value: value.clone(),
                });
                doc = step.apply(&doc).unwrap();
            }

            let attrs = doc.attrs_json();
            assert_eq!(attrs["documentstyle"], "elephant");
            assert_eq!(attrs["tracked"], false);
            assert_eq!(attrs["citationstyle"], "apa");
            assert_eq!(attrs["language"], "en-US");
            assert_eq!(attrs["papersize"], "A4");
            assert_eq!(attrs["template"], "Standard Article");
            assert_eq!(attrs["import_id"], "standard-article");

            // Content preserved
            assert_eq!(doc.text_content(), "hello");
        });
    }

    #[test]
    fn test_doc_attr_step_complex_values() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema.node_from_json(&serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "test"}]
                }]
            })).unwrap();

            // Array value
            let step: Step<Dyn> = Step::DocAttr(DocAttrStep {
                attr: "languages".to_string(),
                value: serde_json::json!(["en-US", "fr", "de"]),
            });
            let doc: DynamicNode = step.apply(&doc).unwrap();
            assert_eq!(doc.attrs_json()["languages"][0], "en-US");

            // Object value
            let step: Step<Dyn> = Step::DocAttr(DocAttrStep {
                attr: "copyright".to_string(),
                value: serde_json::json!({"holder": false, "year": false, "freeToRead": true, "licenses": []}),
            });
            let doc: DynamicNode = step.apply(&doc).unwrap();
            assert_eq!(doc.attrs_json()["copyright"]["freeToRead"], true);
            assert_eq!(doc.attrs_json()["copyright"]["licenses"].as_array().unwrap().len(), 0);
        });
    }

    #[test]
    fn test_attr_step_applies_to_child_node() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [{
                        "type": "heading",
                        "attrs": {"level": 1},
                        "content": [{"type": "text", "text": "title"}]
                    }]
                }))
                .unwrap();

            // Change heading level from 1 to 2.
            // Position 0 is before the first child (the heading), which is what
            // node_at(0) returns — matching upstream JS Node.nodeAt semantics.
            let step: Step<Dyn> = Step::Attr(AttrStep {
                pos: 0,
                attr: "level".to_string(),
                value: serde_json::json!(2),
            });
            let result = step.apply(&doc);
            assert!(result.is_ok());
            let new_doc: DynamicNode = result.unwrap();
            let heading = new_doc.child(0).unwrap();
            let attrs = heading.attrs_json();
            assert_eq!(attrs["level"], 2);
        });
    }

    #[test]
    fn test_attr_step_invalid_position() {
        let schema = basic_schema();
        schema.with_types(|| {
            let doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": "hello"}]
                    }]
                }))
                .unwrap();

            // Position 999 is out of bounds
            let step: Step<Dyn> = Step::Attr(AttrStep {
                pos: 999,
                attr: "level".to_string(),
                value: serde_json::json!(2),
            });
            let result = step.apply(&doc);
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_add_mark_step_serialize() {
        let s: Step<Dyn> =
            serde_json::from_str(r#"{"stepType":"addMark","mark":{"type":"em"},"from":1,"to":5}"#)
                .unwrap();
        match &s {
            Step::AddMark(am) => {
                assert_eq!(am.span.from, 1);
                assert_eq!(am.span.to, 5);
            }
            _ => panic!("Expected AddMark variant"),
        }
    }

    #[test]
    fn test_replace_step_serialize() {
        let s: Step<Dyn> = serde_json::from_str(
            r#"{"stepType":"replace","from":3,"to":5,"slice":{"content":[{"type":"text","text":"hi"}]}}"#,
        ).unwrap();
        match &s {
            Step::Replace(r) => {
                assert_eq!(r.span.from, 3);
                assert_eq!(r.span.to, 5);
            }
            _ => panic!("Expected Replace variant"),
        }
    }
}
