"""
Regression tests for the prosemirror-model API exposed by the Python binding.

Covers Schema, NodeType, MarkType, Mark, Fragment, Slice, Node, ResolvedPos,
NodeRange and ContentMatch — one "does it work at all" assertion per method so
that any future refactor that silently breaks a binding is caught immediately.

Run via:  python -m pytest tests/test_model_api.py  (from the python/ directory)
"""

from __future__ import annotations

import pytest
import prosemirror_rs as pm
from prosemirror_rs import (
    Schema,
    Node,
    Fragment,
    Slice,
    Mark,
    NodeType,
    MarkType,
    ContentMatch,
    ResolvedPos,
    NodeRange,
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
            "image": {
                "inline": True,
                "attrs": {"src": {}},
                "group": "inline",
                "atom": True,
            },
            "code_block": {"content": "text*", "group": "block", "code": True, "marks": ""},
        },
        "marks": {
            "strong": {},
            "em": {},
            "code": {},
        },
    }
)


def make_doc(text: str = "hello") -> Node:
    return SCHEMA.node("doc", None, [SCHEMA.node("paragraph", None, [SCHEMA.text(text)])])


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


class TestSchema:
    def test_nodes_returns_dict(self):
        nodes = SCHEMA.nodes
        assert isinstance(nodes, dict)
        assert isinstance(nodes["paragraph"], NodeType)

    def test_marks_returns_dict(self):
        marks = SCHEMA.marks
        assert isinstance(marks, dict)
        assert isinstance(marks["strong"], MarkType)


# ---------------------------------------------------------------------------
# NodeType
# ---------------------------------------------------------------------------


class TestNodeType:
    para = SCHEMA.nodes["paragraph"]
    text_type = SCHEMA.nodes["text"]
    code_block = SCHEMA.nodes["code_block"]

    def test_is_text(self):
        assert self.text_type.is_text is True
        assert self.para.is_text is False

    def test_whitespace(self):
        assert self.para.whitespace == "normal"
        assert self.code_block.whitespace == "pre"

    def test_is_code(self):
        assert self.code_block.is_code is True
        assert self.para.is_code is False

    def test_inline_content(self):
        assert self.para.inline_content is True
        assert SCHEMA.nodes["doc"].inline_content is False

    def test_is_block_inline_leaf(self):
        assert self.text_type.is_inline is True
        assert self.para.is_block is True
        assert self.text_type.is_leaf is True

    def test_has_required_attrs(self):
        assert SCHEMA.nodes["image"].has_required_attrs is True
        assert self.para.has_required_attrs is False

    def test_compatible_content(self):
        assert self.para.compatible_content(self.para) is True
        assert self.para.compatible_content(SCHEMA.nodes["blockquote"]) is False

    def test_allows_marks(self):
        strong = SCHEMA.mark("strong")
        em = SCHEMA.mark("em")
        # paragraph allows all marks
        assert self.para.allows_marks([strong, em]) is True
        # code_block has marks: "" → no marks
        assert self.code_block.allows_marks([strong]) is False

    def test_content_match(self):
        cm = self.para.content_match
        assert isinstance(cm, ContentMatch)


# ---------------------------------------------------------------------------
# MarkType
# ---------------------------------------------------------------------------


class TestMarkType:
    strong_type = SCHEMA.marks["strong"]
    em_type = SCHEMA.marks["em"]

    def test_remove_from_set(self):
        s = SCHEMA.mark("strong")
        e = SCHEMA.mark("em")
        mark_set = s.add_to_set(e.add_to_set([]))
        result = self.strong_type.remove_from_set(mark_set)
        assert len(result) == 1
        assert result[0].type.name == "em"

    def test_is_in_set_found(self):
        s = SCHEMA.mark("strong")
        mark_set = s.add_to_set([])
        found = self.strong_type.is_in_set(mark_set)
        assert isinstance(found, Mark)
        assert found.type.name == "strong"

    def test_is_in_set_absent(self):
        mark_set = SCHEMA.mark("em").add_to_set([])
        assert self.strong_type.is_in_set(mark_set) is None

    def test_excludes_self(self):
        assert self.strong_type.excludes(self.strong_type) is True
        assert self.strong_type.excludes(self.em_type) is False


# ---------------------------------------------------------------------------
# Mark
# ---------------------------------------------------------------------------


class TestMark:
    def test_to_json(self):
        m = SCHEMA.mark("strong")
        j = m.to_json()
        assert j["type"] == "strong"

    def test_add_remove_is_in_set(self):
        s = SCHEMA.mark("strong")
        e = SCHEMA.mark("em")
        mark_set = s.add_to_set([])
        assert len(mark_set) == 1
        mark_set = e.add_to_set(mark_set)
        assert len(mark_set) == 2
        assert s.is_in_set(mark_set) is True
        mark_set = s.remove_from_set(mark_set)
        assert len(mark_set) == 1
        assert s.is_in_set(mark_set) is False


# ---------------------------------------------------------------------------
# Fragment
# ---------------------------------------------------------------------------


class TestFragment:
    # Use different marks to prevent ProseMirror from merging adjacent text nodes.
    strong = SCHEMA.mark("strong")
    em = SCHEMA.mark("em")

    def make_frag_marked(self, texts_marks):
        """Build a fragment from [(text, marks), ...] to keep nodes distinct."""
        return Fragment.from_array([SCHEMA.text(t, m) for t, m in texts_marks])

    def test_first_last_child(self):
        # Different marks so nodes are not merged
        frag = self.make_frag_marked([("foo", [self.strong]), ("bar", [self.em])])
        assert frag.first_child.text == "foo"
        assert frag.last_child.text == "bar"

    def test_first_child_empty(self):
        frag = Fragment.from_array([])
        assert frag.first_child is None
        assert frag.last_child is None

    def test_maybe_child(self):
        frag = self.make_frag_marked([("a", [self.strong]), ("b", [self.em])])
        assert frag.maybe_child(0).text == "a"
        assert frag.maybe_child(99) is None

    def test_replace_child(self):
        frag = self.make_frag_marked([("foo", [self.strong]), ("bar", [self.em])])
        new_node = SCHEMA.text("baz", [self.strong])
        result = frag.replace_child(0, new_node)
        assert result.child(0).text == "baz"
        assert result.child(1).text == "bar"

    def test_add_to_start_end(self):
        frag = Fragment.from_array([SCHEMA.text("b", [self.em])])
        a = SCHEMA.text("a", [self.strong])
        c = SCHEMA.text("c", [self.strong])
        assert frag.add_to_start(a).child(0).text == "a"
        assert frag.add_to_end(c).child(1).text == "c"

    def test_text_between(self):
        # Single plain text node — no merging issue
        frag = Fragment.from_array([SCHEMA.text("hello world")])
        assert frag.text_between(0, frag.size) == "hello world"

    def test_for_each(self):
        frag = self.make_frag_marked([("a", [self.strong]), ("b", [self.em]), ("c", [self.strong])])
        texts = []
        frag.for_each(lambda node, _offset, _index: texts.append(node.text))
        assert texts == ["a", "b", "c"]

    def test_nodes_between_return_false_skips_children(self):
        """Fragment.nodesBetween — returning False skips children."""
        nested_doc = SCHEMA.node(
            "doc",
            None,
            [
                SCHEMA.node(
                    "blockquote",
                    None,
                    [SCHEMA.node("paragraph", None, [SCHEMA.text("hello")])],
                )
            ],
        )
        frag = nested_doc.content
        visited = []

        def callback(node, _pos, _parent, _index):
            visited.append(node.type.name)
            if node.type.name == "blockquote":
                return False
            return True

        frag.nodes_between(0, frag.size, callback)
        assert "blockquote" in visited
        assert "paragraph" not in visited


# ---------------------------------------------------------------------------
# Slice
# ---------------------------------------------------------------------------


class TestSlice:
    def test_size(self):
        doc = make_doc("hello")
        s = doc.slice(1, 6)
        assert isinstance(s.size, int)
        assert s.size > 0


# ---------------------------------------------------------------------------
# Node
# ---------------------------------------------------------------------------


class TestNode:
    def test_is_atom(self):
        img = SCHEMA.node("image", {"src": "x.png"})
        assert img.is_atom is True
        assert make_doc().is_atom is False

    def test_inline_content(self):
        para = SCHEMA.node("paragraph", None, [SCHEMA.text("hi")])
        assert para.inline_content is True
        assert make_doc().inline_content is False

    def test_is_inline_is_textblock(self):
        para = SCHEMA.node("paragraph", None, [SCHEMA.text("hi")])
        assert para.is_textblock is True
        assert para.is_inline is False

    def test_maybe_child(self):
        doc = make_doc("hello")
        assert doc.maybe_child(0).type.name == "paragraph"
        assert doc.maybe_child(99) is None

    def test_text_between_with_separator(self):
        doc = SCHEMA.node(
            "doc",
            None,
            [
                SCHEMA.node("paragraph", None, [SCHEMA.text("foo")]),
                SCHEMA.node("paragraph", None, [SCHEMA.text("bar")]),
            ],
        )
        assert doc.text_between(0, doc.content.size, "\n") == "foo\nbar"

    def test_for_each(self):
        doc = make_doc("hello")
        names = []
        doc.for_each(lambda node, _offset, _index: names.append(node.type.name))
        assert names == ["paragraph"]

    def test_range_has_mark(self):
        strong = SCHEMA.mark("strong")
        para = SCHEMA.node("paragraph", None, [SCHEMA.text("hi", [strong])])
        doc = SCHEMA.node("doc", None, [para])
        assert doc.range_has_mark(0, doc.content.size, SCHEMA.marks["strong"]) is True
        assert doc.range_has_mark(0, doc.content.size, SCHEMA.marks["em"]) is False

    def test_same_markup(self):
        a = SCHEMA.node("paragraph")
        b = SCHEMA.node("paragraph")
        assert a.same_markup(b) is True

    def test_can_append(self):
        p1 = SCHEMA.node("paragraph", None, [SCHEMA.text("a")])
        p2 = SCHEMA.node("paragraph", None, [SCHEMA.text("b")])
        assert p1.can_append(p2) is True

    def test_content_match_at(self):
        para = SCHEMA.node("paragraph", None, [SCHEMA.text("hello")])
        cm = para.content_match_at(0)
        assert isinstance(cm, ContentMatch)

    def test_nodes_between(self):
        doc = make_doc("hello")
        seen = []
        doc.nodes_between(
            0,
            doc.content.size,
            lambda node, pos, parent, index: seen.append(
                {
                    "name": node.type.name,
                    "parent_name": parent.type.name if parent else None,
                }
            ),
        )
        assert any(e["name"] == "paragraph" for e in seen)
        # paragraph's parent should be the doc
        para = next(e for e in seen if e["name"] == "paragraph")
        assert para["parent_name"] == "doc"

    def test_descendants(self):
        doc = make_doc("hello")
        seen = []
        doc.descendants(lambda node, pos, parent, index: seen.append(node.type.name))
        assert "paragraph" in seen

    # -- nodesBetween / descendants early-termination --------------------

    @staticmethod
    def _nested_doc():
        """doc > blockquote > paragraph > text — nested enough for skip tests."""
        return SCHEMA.node(
            "doc",
            None,
            [
                SCHEMA.node(
                    "blockquote",
                    None,
                    [SCHEMA.node("paragraph", None, [SCHEMA.text("hello")])],
                )
            ],
        )

    def test_nodes_between_return_false_skips_children(self):
        doc = self._nested_doc()
        visited = []

        def callback(node, _pos, _parent, _index):
            visited.append(node.type.name)
            if node.type.name == "blockquote":
                return False  # don't descend
            return True

        doc.nodes_between(0, doc.content.size, callback)
        assert "blockquote" in visited
        assert "paragraph" not in visited
        assert "text" not in visited

    def test_nodes_between_return_true_recurses_normally(self):
        doc = self._nested_doc()
        visited = []

        def callback(node, _pos, _parent, _index):
            visited.append(node.type.name)
            return True

        doc.nodes_between(0, doc.content.size, callback)
        assert "blockquote" in visited
        assert "paragraph" in visited
        assert "text" in visited

    def test_descendants_return_false_skips_children(self):
        doc = self._nested_doc()
        visited = []

        def callback(node, _pos, _parent, _index):
            visited.append(node.type.name)
            if node.type.name == "blockquote":
                return False
            return True

        doc.descendants(callback)
        assert "blockquote" in visited
        assert "paragraph" not in visited
        assert "text" not in visited


# ---------------------------------------------------------------------------
# ResolvedPos
# ---------------------------------------------------------------------------


class TestResolvedPos:
    doc = make_doc("hello")
    rp = doc.resolve(1)

    def test_doc(self):
        assert self.rp.doc.type.name == "doc"

    def test_parent(self):
        assert self.rp.parent.type.name == "paragraph"

    def test_text_offset(self):
        assert isinstance(self.rp.text_offset, int)

    def test_index_index_after(self):
        assert isinstance(self.rp.index(), int)
        assert isinstance(self.rp.index_after(), int)

    def test_shared_depth(self):
        assert self.rp.shared_depth(1) == self.rp.depth

    def test_marks_across(self):
        rp2 = self.doc.resolve(3)
        result = self.rp.marks_across(rp2)
        assert result is None or isinstance(result, list)

    def test_same_parent(self):
        rp2 = self.doc.resolve(3)
        assert self.rp.same_parent(rp2) is True

    def test_max_min(self):
        rp2 = self.doc.resolve(4)
        assert self.rp.max(rp2).pos == max(self.rp.pos, rp2.pos)
        assert self.rp.min(rp2).pos == min(self.rp.pos, rp2.pos)


# ---------------------------------------------------------------------------
# NodeRange
# ---------------------------------------------------------------------------


class TestNodeRange:
    def test_fields(self):
        doc = make_doc("hello")
        from_ = doc.resolve(1)
        to = doc.resolve(3)
        range_ = from_.block_range(to)
        assert isinstance(range_, NodeRange)
        assert isinstance(range_.from_, ResolvedPos)
        assert isinstance(range_.to, ResolvedPos)
        assert isinstance(range_.from_pos, int)
        assert isinstance(range_.to_pos, int)
        assert isinstance(range_.parent, Node)
        assert isinstance(range_.start_index, int)
        assert isinstance(range_.end_index, int)
        assert isinstance(range_.depth, int)


# ---------------------------------------------------------------------------
# ContentMatch
# ---------------------------------------------------------------------------


class TestContentMatch:
    para_type = SCHEMA.nodes["paragraph"]
    cm = SCHEMA.nodes["paragraph"].content_match

    def test_valid_end(self):
        # paragraph content is "text*" → valid end
        assert self.cm.valid_end is True

    def test_edge_count_edge_type_edge_match(self):
        count = self.cm.edge_count
        assert isinstance(count, int)
        if count > 0:
            et = self.cm.edge_type(0)
            assert et is None or isinstance(et, NodeType)
            em = self.cm.edge_match(0)
            assert em is None or isinstance(em, ContentMatch)

    def test_default_type(self):
        dt = self.cm.default_type
        assert dt is None or isinstance(dt, NodeType)

    def test_find_wrapping(self):
        doc_cm = SCHEMA.nodes["doc"].content_match
        result = doc_cm.find_wrapping(SCHEMA.nodes["paragraph"])
        # paragraph is a direct block child → should return [] or a short list
        assert result is None or isinstance(result, list)

    def test_match_type_fragment_fill_before(self):
        matched = self.cm.match_type(SCHEMA.nodes["text"])
        assert matched is None or isinstance(matched, ContentMatch)

        frag = Fragment.from_array([SCHEMA.text("hello")])
        after_frag = self.cm.match_fragment(frag)
        assert after_frag is None or isinstance(after_frag, ContentMatch)

        fill = self.cm.fill_before(frag, True)
        assert fill is None or isinstance(fill, Fragment)
