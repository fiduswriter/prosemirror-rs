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
# StepMap.for_each
# ---------------------------------------------------------------------------


class TestStepMapForEach:
    def test_for_each_visits_each_range(self):
        # StepMap([2, 2, 3]): skip 2, replace 2 old chars with 3 new chars
        sm = StepMap([2, 2, 3])
        calls = []
        sm.for_each(
            lambda old_start, old_end, new_start, new_end: calls.append(
                (old_start, old_end, new_start, new_end)
            )
        )
        assert len(calls) == 1
        old_start, old_end, new_start, new_end = calls[0]
        assert old_start == 2
        assert old_end == 4
        assert new_start == 2
        assert new_end == 5

    def test_for_each_not_called_on_empty_step_map(self):
        sm = StepMap([])
        count = 0
        sm.for_each(lambda *_: None)
        # No ranges → callback never called; use a mutable container
        results = []
        sm.for_each(lambda *args: results.append(args))
        assert results == []

    def test_for_each_multiple_ranges(self):
        sm = StepMap([1, 1, 0, 3, 1, 2])
        results = []
        sm.for_each(lambda *args: results.append(args))
        assert len(results) == 2


# ---------------------------------------------------------------------------
# Mapping constructor with optional maps list
# ---------------------------------------------------------------------------


class TestMappingConstructor:
    def test_new_mapping_no_args(self):
        m = Mapping()
        assert isinstance(m, Mapping)
        assert len(m.maps) == 0

    def test_new_mapping_with_maps(self):
        sm1 = StepMap([1, 1, 0])
        sm2 = StepMap([0, 0, 2])
        m = Mapping([sm1, sm2])
        assert len(m.maps) == 2

    def test_new_mapping_with_maps_maps_positions(self):
        sm = StepMap([2, 2, 3])
        m = Mapping([sm])
        assert isinstance(m.map(6), int)


# ---------------------------------------------------------------------------
# Mapping.append_mapping / append_mapping_inverted
# ---------------------------------------------------------------------------


class TestMappingAppend:
    def test_append_mapping_concatenates_maps(self):
        m1 = Mapping([StepMap([1, 1, 0])])
        m2 = Mapping([StepMap([2, 1, 2])])
        m1.append_mapping(m2)
        assert len(m1.maps) == 2

    def test_append_mapping_maps_through_both(self):
        m1 = Mapping([StepMap([0, 1, 0])])  # delete 1 char at 0
        m2 = Mapping([StepMap([0, 0, 1])])  # insert 1 char at 0
        m1.append_mapping(m2)
        # Net: -1 then +1 → pos 5 unchanged
        assert m1.map(5) == 5

    def test_append_mapping_inverted_extends_maps(self):
        m1 = Mapping([StepMap([1, 1, 0])])
        m2 = Mapping([StepMap([2, 1, 2])])
        orig_len = len(m1.maps)
        m1.append_mapping_inverted(m2)
        assert len(m1.maps) == orig_len + len(m2.maps)


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
        assert len(tr.steps) == 0

    def test_clear_incompatible_chaining(self):
        doc = make_doc("hello", "world")
        tr = Transform(doc)
        para_type = SCHEMA.nodes["paragraph"]
        result = tr.clear_incompatible(1, para_type, False).clear_incompatible(
            doc.node_size - 3, para_type, False
        )
        assert isinstance(result, Transform)
