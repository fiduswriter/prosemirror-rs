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

#[test]
fn test_find_wrapping_li() {
    let schema = DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "ordered_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph"},
            "text": {"group": "inline"},
        }
    }))
    .unwrap();

    schema.with_types(|| {
        let doc_type = schema.node_type("doc").unwrap();
        let li_type = schema.node_type("list_item").unwrap();

        let text = schema.text("foo");
        let para = schema
            .node(
                "paragraph",
                serde_json::Value::Null,
                Fragment::from(vec![text]),
                Default::default(),
            )
            .unwrap();
        let fragment = Fragment::from(vec![para.clone()]);

        println!(
            "doc valid_content for para: {}",
            doc_type.valid_content(&fragment)
        );

        let match0 = doc_type.content_match();
        println!("match0 state: {}", match0.state);
        println!("match0 valid_end: {}", match0.valid_end());
        println!("match0 edge_count: {}", match0.edge_count());
        for i in 0..match0.edge_count() {
            if let Some((t, m)) = match0.edge(i) {
                println!("  edge {}: type={}, next_state={}", i, t.name(), m.state);
            }
        }

        let match_after_para = doc_type.content_match().match_fragment(&fragment);
        println!("match_after_para: {:?}", match_after_para);

        let match_after_para = match_after_para.unwrap();

        println!("valid_end: {}", match_after_para.valid_end());
        println!("edge_count: {}", match_after_para.edge_count());

        for i in 0..match_after_para.edge_count() {
            if let Some((t, _m)) = match_after_para.edge(i) {
                let _name: &str = t.name();
                println!("  edge {}: type={}", i, _name);
            }
        }

        let wrapping = match_after_para.find_wrapping(li_type);
        println!("find_wrapping result: {:?}", wrapping);
        if let Some(ref w) = wrapping {
            for t in w {
                println!("  wrapper: {}", t.name());
            }
        }
        assert!(wrapping.is_some(), "find_wrapping should return Some");
    });
}
#[test]
fn test_groups() {
    let schema = prosemirror::dynamic::DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "ordered_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph"},
            "text": {"group": "inline"},
        }
    }))
    .unwrap();

    let doc_type = schema.node_type("doc").unwrap();
    let match0 = doc_type.content_match();
    println!("match0 edge_count: {}", match0.edge_count());
    for i in 0..match0.edge_count() {
        if let Some((t, m)) = match0.edge(i) {
            println!("  edge {}: type={}, next_state={}", i, t.name(), m.state);
        }
    }
}
#[test]
fn test_content_expr() {
    use prosemirror::dynamic::content_expr::parse_content_expr;
    use std::collections::HashMap;

    let mut groups = HashMap::new();
    groups.insert(
        "block".to_string(),
        vec!["paragraph".to_string(), "ordered_list".to_string()],
    );

    let expr = parse_content_expr("block+", &groups).unwrap();
    println!("states: {}", expr.states.len());
    for (i, state) in expr.states.iter().enumerate() {
        println!(
            "state {}: valid_end={}, edges={:?}",
            i, state.valid_end, state.edges
        );
    }
}
#[test]
fn test_spec_groups() {
    let spec = serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "ordered_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph"},
            "text": {"group": "inline"},
        }
    });

    let schema_spec: prosemirror::dynamic::schema::SchemaSpec =
        serde_json::from_value(spec).unwrap();
    for (name, node_spec) in &schema_spec.nodes {
        println!(
            "{}: group='{}', content='{}'",
            name, node_spec.group, node_spec.content
        );
    }
}
#[test]
fn test_schema_debug() {
    let schema = prosemirror::dynamic::DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "ordered_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph"},
            "text": {"group": "inline"},
        }
    }))
    .unwrap();

    schema.with_types(|| {
        use prosemirror::model::NodeType;
        let doc_type = schema.node_type("doc").unwrap();
        let match0 = doc_type.content_match();
        println!("doc content_match edge_count: {}", match0.edge_count());
        for i in 0..match0.edge_count() {
            if let Some((t, m)) = match0.edge(i) {
                println!("  edge: type={}, next_state={}", t.name(), m.state);
            }
        }
    });
}
#[test]
fn test_schema_order() {
    let spec = serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "ordered_list": {"content": "list_item+", "group": "block"},
            "bullet_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph"},
            "text": {"group": "inline"},
        }
    });

    let schema_spec: prosemirror::dynamic::schema::SchemaSpec =
        serde_json::from_value(spec).unwrap();
    for (name, _) in &schema_spec.nodes {
        println!("node: {}", name);
    }
}
#[test]
fn test_fill_before_doc() {
    let schema = prosemirror::dynamic::DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "blockquote": {"content": "block+", "group": "block"},
            "horizontal_rule": {"group": "block"},
            "heading": {"attrs": {"level": {"default": 1}}, "content": "inline*", "group": "block"},
            "code_block": {"content": "text*", "marks": "", "group": "block"},
            "text": {"group": "inline"},
            "ordered_list": {"content": "list_item+", "group": "block"},
            "bullet_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph block*"},
        }
    }))
    .unwrap();

    schema.with_types(|| {
        let doc_type = schema.node_type("doc").unwrap();
        let match0 = doc_type.content_match();
        let empty = prosemirror::model::Fragment::new();
        let fill = match0.fill_before(&empty, true, 0);
        println!("fill_before result: {:?}", fill);
        if let Some(f) = fill {
            for i in 0..f.child_count() {
                let child = f.child(i);
                println!("  child {}: type={}", i, child.r#type().name());
            }
        }
    });
}
#[test]
fn test_py_schema_fill_before() {
    let schema = prosemirror::dynamic::DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content": "block+", "attrs": {"meta": {"default": null}}},
            "paragraph": {"content": "inline*", "group": "block"},
            "blockquote": {"content": "block+", "group": "block", "defining": true},
            "horizontal_rule": {"group": "block"},
            "heading": {"attrs": {"level": {"default": 1}}, "content": "inline*", "group": "block", "defining": true},
            "code_block": {"content": "text*", "marks": "", "group": "block", "code": true},
            "text": {"group": "inline"},
            "image": {"inline": true, "attrs": {"src": {}, "alt": {"default": null}, "title": {"default": null}}, "group": "inline"},
            "hard_break": {"inline": true, "group": "inline"},
            "ordered_list": {"content": "list_item+", "group": "block", "attrs": {"order": {"default": 1}}},
            "bullet_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph block*", "defining": true}
        },
        "marks": {
            "link": {"attrs": {"href": {}, "title": {"default": null}}, "inclusive": false},
            "em": {},
            "strong": {},
            "code": {"code": true}
        }
    })).unwrap();

    schema.with_types(|| {
        let doc_type = schema.node_type("doc").unwrap();
        let match0 = doc_type.content_match();
        let empty = prosemirror::model::Fragment::new();
        let fill = match0.fill_before(&empty, true, 0);
        println!("fill_before result: {:?}", fill);
        if let Some(f) = fill {
            for i in 0..f.child_count() {
                let child = f.child(i);
                println!("  child {}: type={}", i, child.r#type().name());
            }
        }
    });
}
