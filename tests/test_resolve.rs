//! Tests ported from prosemirror-model/test/test-resolve.ts
//!
//! Verifies that `Node::resolve` produces the correct document structure
//! information at every position in a document, using the dynamic schema.

use prosemirror::dynamic::DynamicSchema;
use prosemirror::model::Node;

fn basic_spec_json() -> serde_json::Value {
    serde_json::json!({
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
    })
}

fn test_doc(schema: &DynamicSchema) -> prosemirror::dynamic::DynamicNode {
    schema.with_types(|| {
        schema
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
            .unwrap()
    })
}

#[test]
fn resolve_reflects_document_structure() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        // Position 0: top of doc
        let rp = d.resolve(0).unwrap();
        assert_eq!(rp.depth, 0);
        assert_eq!(rp.start(0), 0);
        assert_eq!(rp.end(0), 12);

        // Position 1: start of first paragraph
        let rp = d.resolve(1).unwrap();
        assert_eq!(rp.depth, 1);
        assert_eq!(rp.start(1), 1);
        assert_eq!(rp.end(1), 3);

        // Position 4: between p1 and blockquote
        let rp = d.resolve(4).unwrap();
        assert_eq!(rp.depth, 0);
        assert_eq!(rp.index(0), 1);

        // Position 12: end of doc
        let rp = d.resolve(12).unwrap();
        assert_eq!(rp.depth, 0);
    });
}

#[test]
fn resolve_start_end_consistency() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        for pos in 0..=12 {
            let rp = d.resolve(pos).unwrap();
            for depth in 0..=rp.depth {
                assert!(rp.start(depth) <= pos, "{}", "start({depth}) <= pos({pos})");
                assert!(rp.end(depth) >= pos, "{}", "end({depth}) >= pos({pos})");
            }
        }
    });
}

#[test]
fn resolve_before_after_consistency() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        for pos in 0..=12 {
            let rp = d.resolve(pos).unwrap();
            for depth in 1..=rp.depth {
                let b = rp.before(depth).unwrap();
                let s = rp.start(depth);
                assert_eq!(
                    b + 1,
                    s,
                    "before({depth}) + 1 == start({depth}) at pos={pos}"
                );
            }
        }
    });
}

#[test]
fn resolve_node_before_and_after() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        // At pos 2 (between 'a' and 'b' in p1)
        let rp = d.resolve(2).unwrap();
        let nb = rp.node_before().unwrap();
        assert_eq!(nb.text_content(), "a");
        let na = rp.node_after().unwrap();
        assert_eq!(na.text_content(), "b");

        // At pos 1 (start of p1), no node_before
        let rp = d.resolve(1).unwrap();
        assert!(rp.node_before().is_none());
    });
}

#[test]
fn resolve_shared_depth() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        let rp1 = d.resolve(1).unwrap();
        assert_eq!(rp1.shared_depth(3), 1);
        assert_eq!(rp1.shared_depth(6), 0);
    });
}

#[test]
fn resolve_same_parent() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        let rp1 = d.resolve(1).unwrap();
        let rp2 = d.resolve(2).unwrap();
        let rp4 = d.resolve(4).unwrap();
        assert!(rp1.same_parent(&rp2));
        assert!(!rp1.same_parent(&rp4));
    });
}

#[test]
fn resolve_marks_at_position() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = schema.with_types(|| {
        schema
            .node_from_json(&serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [
                        { "type": "text", "text": "hello " },
                        { "type": "text", "text": "world", "marks": [{"type": "em"}] }
                    ]
                }]
            }))
            .unwrap()
    });

    schema.with_types(|| {
        let rp = d.resolve(8).unwrap();
        let marks = rp.marks();
        assert!(!marks.is_empty(), "Should have marks inside em");
    });
}

#[test]
fn resolve_max_min() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        let rp1 = d.resolve(1).unwrap();
        let rp5 = d.resolve(5).unwrap();
        assert_eq!(rp1.max(&rp5).pos, 5);
        assert_eq!(rp1.min(&rp5).pos, 1);
    });
}

#[test]
fn document_node_sizes() {
    let schema = DynamicSchema::from_json(&basic_spec_json()).unwrap();
    let d = test_doc(&schema);

    schema.with_types(|| {
        assert_eq!(d.node_size(), 14);
        assert_eq!(d.content_size(), 12);
        assert_eq!(d.child_count(), 2);
    });
}
