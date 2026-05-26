//! Regression tests for transform wrapping behavior.

use prosemirror::dynamic::types::Dyn;
use prosemirror::dynamic::{DynamicNode, DynamicSchema};
use prosemirror::model::{ContentMatch, Fragment, MarkSet, Node, NodeType};
use prosemirror::transform::structure::NodeRange;
use prosemirror::transform::Transform;

fn wrapper_schema() -> DynamicSchema {
    DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": { "content": "block+" },
            "paragraph": { "content": "text*", "group": "block" },
            "text": { "group": "inline" },
            "outer": { "content": "good_wrapper", "group": "block" },
            "good_wrapper": { "content": "paragraph" },
            "bad_wrapper": { "content": "paragraph" }
        },
        "marks": {}
    }))
    .unwrap()
}

#[test]
fn wrap_rejects_wrapper_stack_when_content_match_fails() {
    let schema = wrapper_schema();

    schema.with_types(|| {
        let doc: DynamicNode = schema
            .node_from_json(&serde_json::json!({
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "x" }]
                }]
            }))
            .unwrap();

        let outer = schema.node_type("outer").unwrap();
        let bad_wrapper = schema.node_type("bad_wrapper").unwrap();
        let bad_content =
            Fragment::from(vec![bad_wrapper.create_node(Some(&Fragment::new()), None)]);

        assert!(
            outer.content_match().match_fragment(&bad_content).is_none(),
            "test schema should make bad_wrapper invalid inside outer"
        );

        let range_doc = doc.clone();
        let range = NodeRange::new(
            range_doc.resolve(0).unwrap(),
            range_doc.resolve(range_doc.content_size()).unwrap(),
            0,
        );

        let wrappers = [
            (outer, None::<MarkSet<Dyn>>),
            (bad_wrapper, None::<MarkSet<Dyn>>),
        ];
        let mut tr = Transform::new(doc);

        tr.wrap(&range, &wrappers);

        assert!(
            tr.steps.is_empty(),
            "invalid nested wrappers must not produce a ReplaceAroundStep"
        );
    });
}
