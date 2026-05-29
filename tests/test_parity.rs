//! Parity tests: compare Rust output against JS-generated fixture files.
//!
//! Each fixture JSON file under `tests/spec/expected/` contains a list of
//! test cases. For each case we apply the described operation to the input
//! document using the Rust API and then assert that the resulting document
//! equals the `expected` field captured from the reference JS implementation.

use prosemirror::dynamic::{DynamicNode, DynamicSchema};
use prosemirror::model::{Fragment, Slice};
use prosemirror::transform::Transform;

fn parity_schema() -> DynamicSchema {
    DynamicSchema::from_json(&serde_json::json!({
        "nodes": {
            "doc": {"content": "block+"},
            "paragraph": {"content": "inline*", "group": "block"},
            "blockquote": {"content": "block+", "group": "block", "defining": true},
            "horizontal_rule": {"group": "block"},
            "heading": {"attrs": {"level": {"default": 1}}, "content": "inline*", "group": "block", "defining": true},
            "code_block": {"content": "text*", "marks": "", "group": "block", "code": true, "defining": true},
            "text": {"group": "inline"},
            "image": {"inline": true, "attrs": {"src": {}, "alt": {"default": null}, "title": {"default": null}}, "group": "inline", "draggable": true},
            "hard_break": {"inline": true, "group": "inline"},
            "ordered_list": {"attrs": {"order": {"default": 1}}, "content": "list_item+", "group": "block"},
            "bullet_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph block*", "defining": true}
        },
        "marks": {
            "link": {"attrs": {"href": {}, "title": {"default": null}}, "inclusive": false},
            "em": {},
            "strong": {},
            "code": {}
        }
    })).unwrap()
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("spec")
        .join("expected")
        .join(format!("{}.json", name))
}

fn apply_operation(
    schema: &DynamicSchema,
    input: DynamicNode,
    case: &serde_json::Value,
    op_type: &str,
) -> DynamicNode {
    use prosemirror::dynamic::types::Dyn;

    let mut tr: Transform<Dyn> = Transform::new(input);
    match op_type {
        "insert" => {
            let pos = case["pos"].as_u64().unwrap() as usize;
            let fragment: Fragment<Dyn> = serde_json::from_value(case["content"].clone()).unwrap();
            tr.insert(pos, fragment);
        }
        "delete" => {
            let from = case["from"].as_u64().unwrap() as usize;
            let to = case["to"].as_u64().unwrap() as usize;
            tr.delete(from, to);
        }
        "addMark" => {
            let from = case["from"].as_u64().unwrap() as usize;
            let to = case["to"].as_u64().unwrap() as usize;
            let mark_name = case["mark"].as_str().unwrap();
            let mark = schema
                .mark_from_json(&serde_json::json!({"type": mark_name}))
                .unwrap();
            tr.add_mark(from, to, mark);
        }
        "removeMark" => {
            let from = case["from"].as_u64().unwrap() as usize;
            let to = case["to"].as_u64().unwrap() as usize;
            let mark_name = case["mark"].as_str().unwrap();
            let mark = schema
                .mark_from_json(&serde_json::json!({"type": mark_name}))
                .unwrap();
            tr.remove_mark(
                from,
                to,
                Some(prosemirror::transform::MarkOrType::Mark(mark)),
            );
        }
        "split" => {
            let pos = case["pos"].as_u64().unwrap() as usize;
            tr.split(pos, None, None).unwrap();
        }
        "replaceAround" => {
            let step: prosemirror::transform::Step<Dyn> =
                serde_json::from_value(case["step"].clone()).unwrap();
            let _ = tr.step(step);
        }
        "replace" => {
            let from = case["from"].as_u64().unwrap() as usize;
            let to = case["to"].as_u64().unwrap() as usize;
            let slice: Option<Slice<Dyn>> = if case["slice"].is_null() {
                None
            } else {
                Some(serde_json::from_value(case["slice"].clone()).unwrap())
            };
            tr.replace(from, Some(to), slice);
        }
        other => panic!("Unknown operation type: {}", other),
    }
    tr.doc.clone()
}

fn run_parity_test(fixture_name: &str) {
    let schema = parity_schema();
    let path = fixture_path(fixture_name);
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", path.display()));
    let fixture: serde_json::Value = serde_json::from_str(&contents).unwrap();
    let cases = fixture["cases"].as_array().unwrap();

    let mut failures: Vec<(String, serde_json::Value, serde_json::Value)> = Vec::new();

    for case in cases {
        let label = case["label"].as_str().unwrap_or("<unlabeled>");
        let op_type = case["type"].as_str().unwrap_or("<unknown>");

        let (actual_json, expected_json) = schema.with_types(|| {
            let input = schema.node_from_json(&case["input"]).unwrap();
            let result_doc = apply_operation(&schema, input, case, op_type);
            let actual = serde_json::to_value(&result_doc).unwrap();
            let expected = case["expected"].clone();
            (actual, expected)
        });

        if actual_json != expected_json {
            failures.push((label.to_string(), actual_json, expected_json));
        }
    }

    if !failures.is_empty() {
        let mut msg = format!("Parity failures in '{}':\n", fixture_name);
        for (label, actual, expected) in &failures {
            msg.push_str(&format!(
                "\n  FAIL: {}\n    actual:   {}\n    expected: {}\n",
                label,
                serde_json::to_string(actual).unwrap(),
                serde_json::to_string(expected).unwrap(),
            ));
        }
        panic!("{}", msg);
    }
}

#[test]
fn parity_transform_edit() {
    run_parity_test("transform_edit");
}

#[test]
fn parity_transform_marks() {
    run_parity_test("transform_marks");
}

#[test]
fn parity_transform_structure() {
    run_parity_test("transform_structure");
}

#[test]
fn parity_replace() {
    run_parity_test("replace");
}

#[test]
fn parity_transform_wrap_lift() {
    run_parity_test("transform_wrap_lift");
}
