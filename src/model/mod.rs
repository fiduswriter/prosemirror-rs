//! # The document model
//!
//! Core types for representing ProseMirror documents: nodes, fragments, marks,
//! resolved positions, content matching, slicing, and replacing.
mod content;
mod fragment;
mod marks;
mod node;
mod replace;
mod resolved_pos;
mod schema;
pub(crate) mod util;

pub use content::{ContentMatch, ContentMatchError};
pub use fragment::Fragment;
pub use marks::{Mark, MarkSet};
pub use node::{Node, NodeType, SliceError, Text};
pub use replace::{InsertError, ReplaceError, Slice};
pub use resolved_pos::{NodeRange, ResolveErr, ResolvedNode, ResolvedPos};
pub use schema::{AttrNode, Block, Leaf, MarkType, Schema, TextNode};

pub(crate) use replace::replace;
pub(crate) use resolved_pos::Index;

#[cfg(test)]
mod tests {
    use super::{fragment::IndexError, Index, Node, ResolvedPos};
    use crate::dynamic::types::Dyn;
    use crate::dynamic::DynamicSchema;

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
    fn test_size() {
        let schema = basic_schema();
        schema.with_types(|| {
            let text_node = schema.text("Hello");
            assert_eq!(text_node.node_size(), 5);

            let emoji = schema.text("\u{1F60A}");
            assert_eq!(emoji.node_size(), 2);

            let test_3 = schema
                .node_from_json(&serde_json::json!({
                    "type": "paragraph",
                    "content": [
                        { "type": "text", "text": "Hallo" },
                        { "type": "text", "text": "Foo" }
                    ]
                }))
                .unwrap();
            assert_eq!(test_3.node_size(), 10);

            let ct_3 = test_3.content().unwrap();
            assert_eq!(ct_3.find_index(0, false), Ok(Index::new(0, 0)));
            assert_eq!(ct_3.find_index(1, false), Ok(Index::new(0, 0)));
            assert_eq!(ct_3.find_index(4, false), Ok(Index::new(0, 0)));
            assert_eq!(ct_3.find_index(5, false), Ok(Index::new(1, 5)));
            assert_eq!(ct_3.find_index(9, false), Err(IndexError::OutOfBounds(9)));
        });
    }

    #[test]
    fn text_between_on_text_node_returns_requested_substring() {
        let schema = basic_schema();
        schema.with_types(|| {
            let text = schema.text("hello");

            assert_eq!(text.text_between(0, 5, None, None), "hello");
            assert_eq!(text.text_between(1, 4, None, None), "ell");
            assert_eq!(text.text_between(2, 2, None, None), "");
            assert_eq!(text.text_between(3, 12, None, None), "lo");
            assert_eq!(text.text_between(10, 12, None, None), "");

            let emoji = schema.text("a😊b");
            assert_eq!(emoji.text_between(0, 1, None, None), "a");
            assert_eq!(emoji.text_between(1, 3, None, None), "😊");
            assert_eq!(emoji.text_between(3, 4, None, None), "b");
        });
    }

    #[test]
    fn test_resolve() {
        let schema = basic_schema();
        schema.with_types(|| {
            let test_doc = schema
                .node_from_json(&serde_json::json!({
                    "type": "doc",
                    "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "ab" }] },
                        { "type": "blockquote", "content": [{
                            "type": "paragraph", "content": [
                                { "type": "text", "text": "cd", "marks": [{"type": "em"}] },
                                { "type": "text", "text": "ef" }
                            ]
                        }]}
                    ]
                }))
                .unwrap();

            let pos = ResolvedPos::<Dyn>::resolve(&test_doc, 0).unwrap();
            assert_eq!(pos.depth, 0);
            assert_eq!(pos.start(0), 0);
            assert_eq!(pos.end(0), 12);

            let pos = ResolvedPos::<Dyn>::resolve(&test_doc, 1).unwrap();
            assert_eq!(pos.depth, 1);

            let pos = ResolvedPos::<Dyn>::resolve(&test_doc, 2).unwrap();
            assert_eq!(pos.depth, 1);
            let nb = pos.node_before().unwrap();
            assert_eq!(nb.text_content(), "a");
            let na = pos.node_after().unwrap();
            assert_eq!(na.text_content(), "b");

            let pos = ResolvedPos::<Dyn>::resolve(&test_doc, 12).unwrap();
            assert_eq!(pos.depth, 0);
        });
    }
}
