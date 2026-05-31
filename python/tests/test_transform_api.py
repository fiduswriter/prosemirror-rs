"""
Regression tests for the prosemirror-transform API exposed by the Python binding.

Covers StepMap, Mapping, Transform — one smoke assertion per method so any
future refactor that silently breaks a binding is caught immediately.

Run via:  python -m pytest tests/test_transform_api.py  (from the python/ directory)
"""

from __future__ import annotations

import pytest
from prosemirror_rs import (
    Schema,
    Node,
    Fragment,
    NodeType,
    StepMap,
    Mapping,
    Transform,
)

# ---------------------------------------------------------------------------
# Shared schema / doc fixture
# ---------------------------------------------------------------------------

SCHEMA = Schema(
    {
        "nodes": {
            "doc": {"content": "paragraph+"},
            "paragraph": {"content": "text*", "group": "block"},
            "blockquote": {"content": "block+", "group": "block"},
            "text": {"group": "inline"},
        },
        "marks": {
            "em": {},
        },
    }
)


def make_doc(*texts: str) -> Node:
    paras = [SCHEMA.node("paragraph", {}, [SCHEMA.text(t)] if t else []) for t in texts]
    return SCHEMA.node("doc", {}, paras)


# ---------------------------------------------------------------------------
# Mapping.copy
# ---------------------------------------------------------------------------


class TestMappingCopy:
    def test_copy_returns_mapping(self):
        m = Mapping()
        sm = StepMap([1, 1, 0])
        m.append_map(sm)
        copy = m.copy()
        assert isinstance(copy, Mapping)

    def test_copy_is_independent(self):
        m = Mapping()
        sm = StepMap([1, 1, 0])
        m.append_map(sm)
        copy = m.copy()
        # Appending to copy should not affect original length
        m_maps = m.maps
        copy.append_map(StepMap([3, 1, 1]))
        assert len(m.maps) == len(m_maps)
        assert len(copy.maps) == len(m_maps) + 1

    def test_copy_maps_same_positions(self):
        m = Mapping()
        m.append_map(StepMap([2, 2, 3]))
        copy = m.copy()
        assert m.map(5) == copy.map(5)


# ---------------------------------------------------------------------------
# Transform.clear_incompatible
# ---------------------------------------------------------------------------


class TestTransformClearIncompatible:
    def test_clear_incompatible_returns_transform(self):
        doc = make_doc("hello")
        tr = Transform(doc)
        para_type = SCHEMA.nodes["paragraph"]
        result = tr.clear_incompatible(1, para_type, False)
        assert isinstance(result, Transform)

    def test_clear_incompatible_noop_on_valid_content(self):
        doc = make_doc("hello")
        tr = Transform(doc)
        para_type = SCHEMA.nodes["paragraph"]
        tr.clear_incompatible(1, para_type, False)
        # A paragraph with text is already valid — no steps should be added
        assert len(tr.steps) == 0

    def test_clear_incompatible_chaining(self):
        doc = make_doc("hello", "world")
        tr = Transform(doc)
        para_type = SCHEMA.nodes["paragraph"]
        # Should be chainable
        result = tr.clear_incompatible(1, para_type, False).clear_incompatible(
            doc.node_size - 3, para_type, False
        )
        assert isinstance(result, Transform)
