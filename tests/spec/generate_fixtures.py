#!/usr/bin/env python3
"""
Generate JSON test fixtures from prosemirror-py.

This script uses prosemirror-py (the Python ProseMirror implementation)
to compute expected outputs for a comprehensive set of test cases. The
generated JSON files are then consumed by the Rust test suite to verify
that prosemirror-rs produces identical results.

Usage:
    cd prosemirror-py && python ../prosemirror-rs/tests/spec/generate_fixtures.py

The fixtures are written to tests/spec/expected/*.json
"""

import json
import os
from pathlib import Path
from typing import Any

from prosemirror.model import Fragment, Node, Schema, Slice
from prosemirror.model.schema import SchemaSpec
from prosemirror.transform import (
    AddMarkStep,
    Mapping,
    RemoveMarkStep,
    ReplaceStep,
    StepMap,
    Transform,
)

# ---------------------------------------------------------------------------
# Schema definition (shared across all test categories)
# ---------------------------------------------------------------------------

basic_spec: SchemaSpec[Any, Any] = {
    "nodes": {
        "doc": {"content": "block+"},
        "paragraph": {
            "content": "inline*",
            "group": "block",
            "parseDOM": [{"tag": "p"}],
        },
        "blockquote": {
            "content": "block+",
            "group": "block",
            "defining": True,
            "parseDOM": [{"tag": "blockquote"}],
        },
        "horizontal_rule": {"group": "block", "parseDOM": [{"tag": "hr"}]},
        "heading": {
            "attrs": {"level": {"default": 1}},
            "content": "inline*",
            "group": "block",
            "defining": True,
            "parseDOM": [
                {"tag": "h1", "attrs": {"level": 1}},
                {"tag": "h2", "attrs": {"level": 2}},
                {"tag": "h3", "attrs": {"level": 3}},
            ],
        },
        "code_block": {
            "content": "text*",
            "marks": "",
            "group": "block",
            "code": True,
            "defining": True,
            "parseDOM": [{"tag": "pre", "preserveWhitespace": "full"}],
        },
        "text": {"group": "inline"},
        "image": {
            "inline": True,
            "attrs": {"src": {}, "alt": {"default": None}, "title": {"default": None}},
            "group": "inline",
            "draggable": True,
            "parseDOM": [{"tag": "img[src]"}],
        },
        "hard_break": {
            "inline": True,
            "group": "inline",
            "selectable": False,
            "parseDOM": [{"tag": "br"}],
        },
        "ordered_list": {
            "attrs": {"order": {"default": 1}},
            "parseDOM": [{"tag": "ol"}],
            "content": "list_item+",
            "group": "block",
        },
        "bullet_list": {
            "parseDOM": [{"tag": "ul"}],
            "content": "list_item+",
            "group": "block",
        },
        "list_item": {
            "parseDOM": [{"tag": "li"}],
            "defining": True,
            "content": "paragraph block*",
        },
    },
    "marks": {
        "link": {
            "attrs": {"href": {}, "title": {"default": None}},
            "inclusive": False,
            "parseDOM": [{"tag": "a[href]"}],
        },
        "em": {
            "parseDOM": [{"tag": "i"}, {"tag": "em"}, {"style": "font-style=italic"}],
        },
        "strong": {
            "parseDOM": [{"tag": "strong"}, {"tag": "b"}, {"style": "font-weight"}],
        },
        "code": {"parseDOM": [{"tag": "code"}]},
    },
}

schema = Schema(basic_spec)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

OUT_DIR = Path(__file__).parent / "expected"


def save(name: str, data: Any) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    path = OUT_DIR / f"{name}.json"
    with open(path, "w") as f:
        json.dump(data, f, indent=2)
    print(f"  wrote {path}")


def doc_json(*children: Node) -> dict:
    """Build a doc node from children and return its JSON."""
    return schema.node("doc", {}, list(children)).to_json()


def p(text: str = "", *marks) -> Node:
    """Shortcut for a paragraph node."""
    if text:
        return schema.node("paragraph", {}, [schema.text(text, marks)])
    return schema.node("paragraph")


def h(level: int, text: str = "", *marks) -> Node:
    """Shortcut for a heading node."""
    return schema.node("heading", {"level": level}, [schema.text(text, marks)])


def blockquote(*children: Node) -> Node:
    return schema.node("blockquote", {}, list(children))


def strong(text: str) -> Node:
    return schema.text(text, [schema.mark("strong")])


def em(text: str) -> Node:
    return schema.text(text, [schema.mark("em")])


def code(text: str) -> Node:
    return schema.text(text, [schema.mark("code")])


def hr() -> Node:
    return schema.node("horizontal_rule")


def br() -> Node:
    return schema.node("hard_break")


def ol(*items: Node) -> Node:
    return schema.node("ordered_list", {}, list(items))


def ul(*items: Node) -> Node:
    return schema.node("bullet_list", {}, list(items))


def li(*children: Node) -> Node:
    return schema.node("list_item", {}, list(children))


def img(src: str = "x.png") -> Node:
    return schema.node("image", {"src": src})


# ---------------------------------------------------------------------------
# Test: StepMap / Mapping
# ---------------------------------------------------------------------------


def generate_mapping_tests():
    """Port of prosemirror-transform/test/test-mapping.ts"""
    cases = []

    def mk_mapping(specs):
        """Build a Mapping from a list of (ranges, mirrors) pairs."""
        mapping = Mapping()
        mirrors_list = []
        for item in specs:
            if isinstance(item, list):
                mapping.append_map(StepMap(item))
            elif isinstance(item, dict):
                mirrors_list.append(item)
        for m in mirrors_list:
            for k, v in m.items():
                mapping.set_mirror(int(k), v)
        return mapping

    def test_mapping(label, specs, expected_maps):
        mapping = mk_mapping(specs)
        results = []
        for from_pos, bias, expected_to, lossy in expected_maps:
            mapped = mapping.map(from_pos, bias)
            inv = mapping.invert()
            inv_mapped = inv.map(mapped, bias)
            results.append(
                {
                    "from": from_pos,
                    "bias": bias,
                    "to": mapped,
                    "expected_to": expected_to,
                    "lossy": lossy,
                    "inverse_to": inv_mapped,
                }
            )
        cases.append({"label": label, "results": results})

    def test_del(label, specs, pos, side, expected_flags):
        mapping = mk_mapping(specs)
        r = mapping.map_result(pos, side)
        found = ""
        if r.deleted:
            found += "d"
        if r.deleted_before:
            found += "b"
        if r.deleted_after:
            found += "a"
        if r.deleted_across:
            found += "x"
        cases.append(
            {
                "label": label,
                "pos": pos,
                "side": side,
                "flags": found,
                "expected_flags": expected_flags,
            }
        )

    # Single insertion
    test_mapping(
        "single insertion",
        [[2, 0, 4]],
        [(0, 1, 0, False), (2, 1, 6, False), (2, -1, 2, False), (3, 1, 7, False)],
    )

    # Single deletion
    test_mapping(
        "single deletion",
        [[2, 4, 0]],
        [
            (0, 1, 0, False),
            (2, -1, 2, False),
            (3, 1, 2, True),
            (6, 1, 2, False),
            (6, -1, 2, True),
            (7, 1, 3, False),
        ],
    )

    # Single replace
    test_mapping(
        "single replace",
        [[2, 4, 4]],
        [
            (0, 1, 0, False),
            (2, 1, 2, False),
            (4, 1, 6, True),
            (4, -1, 2, True),
            (6, -1, 6, False),
            (8, 1, 8, False),
        ],
    )

    # Deletion flags
    test_del("del before 1", [[0, 2, 0]], 2, -1, "db")
    test_del("del before 2", [[0, 2, 0]], 2, 1, "b")
    test_del("del after 1", [[2, 2, 0]], 2, -1, "a")
    test_del("del after 2", [[2, 2, 0]], 2, 1, "da")
    test_del("del across 1", [[0, 4, 0]], 2, -1, "dbax")
    test_del("del across 2", [[0, 4, 0]], 2, 1, "dbax")

    save("mapping", {"cases": cases})


# ---------------------------------------------------------------------------
# Test: Step merge
# ---------------------------------------------------------------------------


def generate_step_merge_tests():
    """Port of prosemirror-transform/test/test-step.ts"""
    test_doc = schema.node(
        "doc", {}, [schema.node("paragraph", {}, [schema.text("foobar")])]
    )

    def mk_step(from_pos, to_pos, val):
        if val == "+em":
            return AddMarkStep(from_pos, to_pos, schema.mark("em"))
        elif val == "-em":
            return RemoveMarkStep(from_pos, to_pos, schema.mark("em"))
        else:
            if val is None:
                s = Slice.empty
            else:
                s = Slice(Fragment.from_(schema.text(val)), 0, 0)
            return ReplaceStep(from_pos, to_pos, s)

    cases = []

    def yes(label, f1, t1, v1, f2, t2, v2):
        s1 = mk_step(f1, t1, v1)
        s2 = mk_step(f2, t2, v2)
        merged = s1.merge(s2)
        doc1 = s2.apply(s1.apply(test_doc).doc).doc
        doc2 = merged.apply(test_doc).doc if merged else None
        cases.append(
            {
                "label": label,
                "should_merge": True,
                "step1": s1.to_json(),
                "step2": s2.to_json(),
                "result_after_both": doc1.to_json(),
                "result_after_merged": doc2.to_json() if doc2 else None,
            }
        )

    def no(label, f1, t1, v1, f2, t2, v2):
        s1 = mk_step(f1, t1, v1)
        s2 = mk_step(f2, t2, v2)
        merged = s1.merge(s2)
        cases.append(
            {
                "label": label,
                "should_merge": False,
                "step1": s1.to_json(),
                "step2": s2.to_json(),
            }
        )

    yes("merges typing changes", 2, 2, "a", 3, 3, "b")
    yes("merges inverse typing", 2, 2, "a", 2, 2, "b")
    no("doesn't merge separated typing", 2, 2, "a", 4, 4, "b")
    no("doesn't merge inverted separated typing", 3, 3, "a", 2, 2, "b")
    yes("merges adjacent backspaces", 3, 4, None, 2, 3, None)
    yes("merges adjacent deletes", 2, 3, None, 2, 3, None)
    no("doesn't merge separate backspaces", 1, 2, None, 2, 3, None)
    yes("merges backspace and type", 2, 3, None, 2, 2, "x")
    yes("merges longer adjacent inserts", 2, 2, "quux", 6, 6, "baz")
    yes("merges inverted longer inserts", 2, 2, "quux", 2, 2, "baz")
    yes("merges longer deletes", 2, 5, None, 2, 4, None)
    yes("merges inverted longer deletes", 4, 6, None, 2, 4, None)
    yes("merges overwrites", 3, 4, "x", 4, 5, "y")
    yes("merges adding adjacent styles", 1, 2, "+em", 2, 4, "+em")
    yes("merges adding overlapping styles", 1, 3, "+em", 2, 4, "+em")
    no("doesn't merge separate styles", 1, 2, "+em", 3, 4, "+em")
    yes("merges removing adjacent styles", 1, 2, "-em", 2, 4, "-em")
    yes("merges removing overlapping styles", 1, 3, "-em", 2, 4, "-em")
    no("doesn't merge removing separate styles", 1, 2, "-em", 3, 4, "-em")

    save("step_merge", {"test_doc": test_doc.to_json(), "cases": cases})


# ---------------------------------------------------------------------------
# Test: Transform addMark / removeMark
# ---------------------------------------------------------------------------


def generate_transform_mark_tests():
    """Port of mark-related tests from test-trans.ts"""
    cases = []

    def add_mark(label, doc_node, from_pos, to_pos, mark_name):
        tr = Transform(doc_node)
        tr.add_mark(from_pos, to_pos, schema.mark(mark_name))
        cases.append(
            {
                "label": label,
                "type": "addMark",
                "input": doc_node.to_json(),
                "from": from_pos,
                "to": to_pos,
                "mark": mark_name,
                "expected": tr.doc.to_json(),
            }
        )

    def remove_mark(label, doc_node, from_pos, to_pos, mark_name):
        tr = Transform(doc_node)
        tr.remove_mark(from_pos, to_pos, schema.mark(mark_name))
        cases.append(
            {
                "label": label,
                "type": "removeMark",
                "input": doc_node.to_json(),
                "from": from_pos,
                "to": to_pos,
                "mark": mark_name,
                "expected": tr.doc.to_json(),
            }
        )

    # addMark tests
    add_mark(
        "can add a mark",
        schema.node(
            "doc", {}, [schema.node("paragraph", {}, [schema.text("hello there!")])]
        ),
        7,
        12,
        "strong",
    )

    # NOTE: correct positions are 25,26 (not 26,27) to actually mark the final 'i'
    add_mark(
        "can add a mark in a nested node",
        schema.node(
            "doc",
            {},
            [
                blockquote(
                    schema.node(
                        "paragraph", {}, [schema.text("the variable is called i")]
                    )
                )
            ],
        ),
        25,
        26,
        "code",
    )

    # removeMark tests
    remove_mark(
        "can cut a gap",
        schema.node(
            "doc",
            {},
            [
                schema.node(
                    "paragraph", {}, [schema.text("hello world!", [schema.mark("em")])]
                )
            ],
        ),
        7,
        12,
        "em",
    )

    save("transform_marks", {"cases": cases})


# ---------------------------------------------------------------------------
# Test: Transform insert / delete
# ---------------------------------------------------------------------------


def generate_transform_edit_tests():
    """Port of insert/delete tests from test-trans.ts"""
    cases = []

    def insert_test(label, doc_node, pos, content):
        tr = Transform(doc_node)
        tr.insert(pos, content)
        cases.append(
            {
                "label": label,
                "type": "insert",
                "input": doc_node.to_json(),
                "pos": pos,
                "content": content.to_json() if isinstance(content, Fragment) else None,
                "expected": tr.doc.to_json(),
            }
        )

    def delete_test(label, doc_node, from_pos, to_pos):
        tr = Transform(doc_node)
        tr.delete(from_pos, to_pos)
        cases.append(
            {
                "label": label,
                "type": "delete",
                "input": doc_node.to_json(),
                "from": from_pos,
                "to": to_pos,
                "expected": tr.doc.to_json(),
            }
        )

    # insert
    insert_test(
        "can insert a break",
        schema.node("doc", {}, [p("hellothere")]),
        6,
        Fragment.from_([schema.node("hard_break")]),
    )

    insert_test(
        "can insert an empty paragraph at the top",
        schema.node("doc", {}, [p("one"), p("two")]),
        5,
        Fragment.from_([schema.node("paragraph")]),
    )

    # delete
    delete_test(
        "can delete a word",
        schema.node("doc", {}, [p("one"), p("two"), p("three")]),
        5,
        10,
    )

    delete_test("can delete text", schema.node("doc", {}, [p("hello you")]), 5, 7)

    save("transform_edit", {"cases": cases})


# ---------------------------------------------------------------------------
# Test: Transform join / split
# ---------------------------------------------------------------------------


def generate_transform_structure_tests():
    """Port of join/split tests from test-trans.ts (simplified for fixtures)"""
    cases = []

    def split_test(label, doc_node, pos):
        tr = Transform(doc_node)
        tr.split(pos)
        cases.append(
            {
                "label": label,
                "type": "split",
                "input": doc_node.to_json(),
                "pos": pos,
                "expected": tr.doc.to_json(),
            }
        )

    # split
    split_test("can split a textblock", schema.node("doc", {}, [p("foobar")]), 4)

    split_test("can split at the start", schema.node("doc", {}, [p("foobar")]), 1)

    split_test("can split at the end", schema.node("doc", {}, [p("foobar")]), 7)

    save("transform_structure", {"cases": cases})


# ---------------------------------------------------------------------------
# Test: Replace (the smart replace / Fitter)
# ---------------------------------------------------------------------------


def generate_replace_tests():
    """Port of replace tests from test-trans.ts"""
    cases = []

    def replace_test(label, doc_node, from_pos, to_pos, source):
        if source is None:
            slice = Slice.empty
        elif isinstance(source, Node):
            slice = source.slice(0, source.content.size)
        else:
            slice = source
        tr = Transform(doc_node)
        tr.replace(from_pos, to_pos, slice)
        cases.append(
            {
                "label": label,
                "type": "replace",
                "input": doc_node.to_json(),
                "from": from_pos,
                "to": to_pos,
                "slice": slice.to_json() if hasattr(slice, "to_json") else None,
                "expected": tr.doc.to_json(),
            }
        )

    # can delete text
    replace_test(
        "can delete text", schema.node("doc", {}, [p("hello you")]), 5, 7, None
    )

    # can join blocks
    replace_test(
        "can join blocks", schema.node("doc", {}, [p("hello"), p("you")]), 5, 8, None
    )

    # can overwrite text
    replace_test(
        "can overwrite text",
        schema.node("doc", {}, [p("hello you")]),
        5,
        7,
        schema.node("doc", {}, [p("i k")]),
    )

    # can add a textblock
    replace_test(
        "can add a textblock",
        schema.node("doc", {}, [p("helloyou")]),
        6,
        6,
        schema.node("doc", {}, [p("there")]),
    )

    # merges blocks across deleted content
    replace_test(
        "merges blocks across deleted content",
        schema.node("doc", {}, [p("a"), p("b"), p("c")]),
        3,
        6,
        None,
    )

    # can delete the whole document
    replace_test(
        "can delete the whole document",
        schema.node("doc", {}, [h(1, "hi"), p("you")]),
        0,
        7,
        None,
    )

    save("replace", {"cases": cases})


# ---------------------------------------------------------------------------
# Test: Node / Fragment model
# ---------------------------------------------------------------------------


def generate_model_tests():
    """Port of model tests (slice, cut, nodesBetween, textBetween)"""
    cases = []

    # Slice tests
    test_doc = schema.node(
        "doc",
        {},
        [
            p("hello"),
            blockquote(p("world")),
        ],
    )

    def slice_test(
        label, doc_node, from_pos, to_pos, expected_open_start, expected_open_end
    ):
        s = doc_node.slice(from_pos, to_pos)
        cases.append(
            {
                "label": label,
                "type": "slice",
                "input": doc_node.to_json(),
                "from": from_pos,
                "to": to_pos,
                "content": s.content.to_json() if s.content.size else None,
                "open_start": s.open_start,
                "open_end": s.open_end,
                "expected_open_start": expected_open_start,
                "expected_open_end": expected_open_end,
            }
        )

    slice_test("slice half paragraph", test_doc, 2, 4, 0, 0)
    slice_test("slice across blocks", test_doc, 2, 8, 0, 1)

    # textBetween tests
    def text_between_test(label, doc_node, from_pos, to_pos, expected):
        result = doc_node.text_between(from_pos, to_pos)
        cases.append(
            {
                "label": label,
                "type": "text_between",
                "input": doc_node.to_json(),
                "from": from_pos,
                "to": to_pos,
                "result": result,
                "expected": expected,
            }
        )

    text_between_test("text between in paragraph", test_doc, 1, 6, "hello")
    text_between_test("text between in doc", test_doc, 1, 10, "hello")

    # nodeSize
    def size_test(label, doc_node, expected):
        cases.append(
            {
                "label": label,
                "type": "node_size",
                "input": doc_node.to_json(),
                "size": doc_node.node_size,
                "expected": expected,
            }
        )

    size_test("empty doc size", schema.node("doc", {}, [p()]), 3)
    size_test("doc with text", schema.node("doc", {}, [p("hi")]), 5)

    save("model", {"cases": cases})


# ---------------------------------------------------------------------------
# Test: Resolve
# ---------------------------------------------------------------------------


def generate_resolve_tests():
    """Port of prosemirror-model/test/test-resolve.ts"""
    test_doc = schema.node(
        "doc",
        {},
        [
            schema.node("paragraph", {}, [schema.text("ab")]),
            schema.node(
                "blockquote",
                {},
                [
                    schema.node(
                        "paragraph",
                        {},
                        [
                            schema.text("cd", [schema.mark("em")]),
                            schema.text("ef"),
                        ],
                    )
                ],
            ),
        ],
    )

    cases = []
    for pos in range(test_doc.content.size + 1):
        rp = test_doc.resolve(pos)
        cases.append(
            {
                "pos": pos,
                "depth": rp.depth,
                "parent_offset": rp.parent_offset,
                "parent_type": rp.parent.type.name,
            }
        )

    save("resolve", {"doc": test_doc.to_json(), "cases": cases})


# ---------------------------------------------------------------------------
# Test: JSON round-trip for documents
# ---------------------------------------------------------------------------


def generate_roundtrip_tests():
    """Verify that documents survive JSON serialization round-trip."""
    docs = [
        schema.node("doc", {}, [p("hello world")]),
        schema.node("doc", {}, [h(1, "Title"), p("text"), blockquote(p("quote"))]),
        schema.node("doc", {}, [p("one"), p("two"), p("three")]),
        schema.node(
            "doc",
            {},
            [
                schema.node(
                    "paragraph",
                    {},
                    [
                        schema.text("bold", [schema.mark("strong")]),
                        schema.text(" and "),
                        schema.text("italic", [schema.mark("em")]),
                    ],
                )
            ],
        ),
        schema.node(
            "doc",
            {},
            [
                schema.node(
                    "ordered_list",
                    {},
                    [
                        schema.node("list_item", {}, [p("first")]),
                        schema.node("list_item", {}, [p("second")]),
                    ],
                )
            ],
        ),
    ]

    cases = []
    for d in docs:
        json_data = d.to_json()
        restored = Node.from_json(schema, json_data)
        assert d.eq(restored), f"Round-trip failed for {json_data}"
        cases.append(
            {
                "json": json_data,
                "roundtrip_ok": True,
                "node_size": d.node_size,
            }
        )

    save("roundtrip", {"cases": cases})


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    print("Generating test fixtures from prosemirror-py...")
    generate_mapping_tests()
    generate_step_merge_tests()
    generate_transform_mark_tests()
    generate_transform_edit_tests()
    generate_transform_structure_tests()
    generate_replace_tests()
    generate_model_tests()
    generate_resolve_tests()
    generate_roundtrip_tests()
    print("Done.")
