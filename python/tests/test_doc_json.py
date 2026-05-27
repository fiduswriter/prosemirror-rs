#!/usr/bin/env python3
"""
Tests for Editor.doc_json() — "mini" JSON output.

Verifies that:
  1. doc_json() (no arguments) includes all attributes.
  2. doc_json(True) omits attributes that match schema defaults.
  3. doc_json(True) preserves non-default attributes.
  4. doc_json(True) with a mix of default/non-default attributes only shows
     the non-default ones.
  5. Backwards compatibility: doc_json() == doc_json(False).
"""

import json
import sys

from prosemirror_rs import Editor

# Schema with attributes that have defaults
SCHEMA = json.dumps(
    {
        "nodes": {
            "doc": {"content": "paragraph+"},
            "paragraph": {
                "content": "text*",
                "group": "block",
                "attrs": {"align": {"default": "left"}, "indent": {"default": 0}},
            },
            "text": {"group": "inline"},
        },
        "marks": {"strong": {"attrs": {"level": {"default": 1}}}, "em": {}},
    }
)


def parse(raw: str) -> dict:
    """Convenience: parse a JSON string into a Python dict."""
    return json.loads(raw)


def test_default_all_attrs_include() -> None:
    """
    doc_json() (no arguments) includes every attribute, even when
    they match the schema default.
    """
    doc_json_str = json.dumps(
        {
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "attrs": {"align": "left", "indent": 0},
                    "content": [{"type": "text", "text": "hello"}],
                }
            ],
        }
    )
    editor = Editor(SCHEMA, doc_json_str)

    raw = editor.doc_json()
    obj = parse(raw)

    assert obj["type"] == "doc"
    para = obj["content"][0]
    assert para["type"] == "paragraph"
    assert para["attrs"] == {"align": "left", "indent": 0}, (
        f"Expected all attrs present, got {para['attrs']!r}"
    )


def test_mini_omits_default_attrs() -> None:
    """
    doc_json(True) omits attributes whose value matches the schema
    default from both nodes and marks.
    """
    doc_json_str = json.dumps(
        {
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "attrs": {"align": "left", "indent": 0},
                    "content": [
                        {
                            "type": "text",
                            "text": "hello",
                            "marks": [{"type": "strong", "attrs": {"level": 1}}],
                        }
                    ],
                }
            ],
        }
    )
    editor = Editor(SCHEMA, doc_json_str)

    raw = editor.doc_json(True)
    obj = parse(raw)

    para = obj["content"][0]
    # attrs omitted entirely because both match defaults
    assert "attrs" not in para, f"Expected no attrs on paragraph, got {para.get('attrs')!r}"

    # Mark attrs also omitted
    text_node = para["content"][0]
    assert "marks" in text_node
    strong_mark = text_node["marks"][0]
    assert strong_mark["type"] == "strong"
    assert "attrs" not in strong_mark, (
        f"Expected no attrs on strong mark, got {strong_mark.get('attrs')!r}"
    )


def test_mini_preserves_non_default_attrs() -> None:
    """
    doc_json(True) preserves attributes whose value differs from the
    schema default.
    """
    doc_json_str = json.dumps(
        {
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "attrs": {"align": "right", "indent": 3},
                    "content": [
                        {
                            "type": "text",
                            "text": "world",
                            "marks": [{"type": "strong", "attrs": {"level": 2}}],
                        }
                    ],
                }
            ],
        }
    )
    editor = Editor(SCHEMA, doc_json_str)

    raw = editor.doc_json(True)
    obj = parse(raw)

    para = obj["content"][0]
    assert para["attrs"] == {"align": "right", "indent": 3}, (
        f"Expected non-default attrs preserved, got {para.get('attrs')!r}"
    )

    text_node = para["content"][0]
    strong_mark = text_node["marks"][0]
    assert strong_mark["attrs"] == {"level": 2}, (
        f"Expected non-default mark attrs preserved, got {strong_mark.get('attrs')!r}"
    )


def test_mini_mixed_attrs() -> None:
    """
    doc_json(True) with a mix of default and non-default attributes:
    only the non-default attributes appear.
    """
    doc_json_str = json.dumps(
        {
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    # align is default ("left"), indent is non-default (2)
                    "attrs": {"align": "left", "indent": 2},
                    "content": [{"type": "text", "text": "mixed"}],
                }
            ],
        }
    )
    editor = Editor(SCHEMA, doc_json_str)

    raw = editor.doc_json(True)
    obj = parse(raw)

    para = obj["content"][0]
    # Only the non-default attr should survive
    assert para["attrs"] == {"indent": 2}, (
        f"Expected only non-default attr (indent=2), got {para.get('attrs')!r}"
    )


def test_backwards_compatibility() -> None:
    """
    doc_json() without arguments should produce the same output as
    doc_json(False).
    """
    doc_json_str = json.dumps(
        {
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "attrs": {"align": "right", "indent": 0},
                    "content": [{"type": "text", "text": "compat"}],
                }
            ],
        }
    )
    editor = Editor(SCHEMA, doc_json_str)

    raw_default = editor.doc_json()
    raw_explicit = editor.doc_json(False)

    assert raw_default == raw_explicit, (
        f"doc_json() != doc_json(False)\n  default: {raw_default}\n  explicit: {raw_explicit}"
    )


# ── runner ────────────────────────────────────────────────────────────────────


def main() -> int:
    tests = [
        ("all attrs included (default)", test_default_all_attrs_include),
        ("mini omits default attrs", test_mini_omits_default_attrs),
        ("mini preserves non-default attrs", test_mini_preserves_non_default_attrs),
        ("mini mixed attrs", test_mini_mixed_attrs),
        ("backwards compat: doc_json() == False", test_backwards_compatibility),
    ]

    failures = 0
    for label, fn in tests:
        try:
            fn()
            print(f"  PASS  {label}")
        except Exception as exc:
            print(f"  FAIL  {label}")
            # Print the traceback manually to stay self-contained
            import traceback

            traceback.print_exc()
            failures += 1

    total = len(tests)
    if failures:
        print(f"\nRESULT: {total - failures}/{total} passed — {failures} failure(s)")
        return 1
    else:
        print(f"\nRESULT: {total}/{total} passed — all good!")
        return 0


if __name__ == "__main__":
    sys.exit(main())
