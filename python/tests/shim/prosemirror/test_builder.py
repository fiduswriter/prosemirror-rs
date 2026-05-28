import re
from typing import Any

from prosemirror.model import Node, Schema

# Build a test schema that matches the upstream prosemirror-test-builder schema.
_test_schema = Schema(
    {
        "nodes": {
            "doc": {"content": "block+", "attrs": {"meta": {"default": None}}},
            "paragraph": {"content": "inline*", "group": "block"},
            "blockquote": {"content": "block+", "group": "block", "defining": True},
            "horizontal_rule": {"group": "block"},
            "heading": {
                "attrs": {"level": {"default": 1}},
                "content": "inline*",
                "group": "block",
                "defining": True,
            },
            "code_block": {"content": "text*", "marks": "", "group": "block", "code": True},
            "text": {"group": "inline"},
            "image": {
                "inline": True,
                "attrs": {
                    "src": {"validate": "string"},
                    "alt": {"default": None},
                    "title": {"default": None},
                },
                "group": "inline",
            },
            "hard_break": {"inline": True, "group": "inline"},
            "ordered_list": {
                "content": "list_item+",
                "group": "block",
                "attrs": {"order": {"default": 1}},
            },
            "bullet_list": {"content": "list_item+", "group": "block"},
            "list_item": {"content": "paragraph block*", "defining": True},
        },
        "marks": {
            "link": {"attrs": {"href": {}, "title": {"default": None}}, "inclusive": False},
            "em": {},
            "strong": {},
            "code": {"code": True},
        },
    }
)


NO_TAG = object()


def flatten(schema: Schema, children: list, f):
    result, pos, tag = [], 0, NO_TAG

    for child in children:
        if isinstance(child, str):
            at = 0
            out_str = ""
            for m in re.finditer(r"<(\w+)>", child):
                out_str += child[at : m.start()]
                pos += m.start() - at
                at = m.start() + len(m[0])
                if tag == NO_TAG:
                    tag = {}
                tag[m[1]] = pos
            out_str += child[at:]
            pos += len(child) - at
            if out_str:
                result.append(f(schema.text(out_str)))
        else:
            child_tag = (
                getattr(child, "tag", NO_TAG)
                if isinstance(child, Node)
                else child.get("tag", NO_TAG)
                if isinstance(child, dict)
                else NO_TAG
            )
            if child_tag and child_tag != NO_TAG:
                if tag == NO_TAG:
                    tag = {}
                is_flat = getattr(child, "flat", None) or (
                    isinstance(child, dict) and "flat" in child
                )
                is_text = getattr(child, "is_text", False)
                for tid in child_tag:
                    tag[tid] = child_tag[tid] + (0 if is_flat or is_text else 1) + pos
            flat = getattr(child, "flat", None) or (
                child.get("flat") if isinstance(child, dict) else None
            )
            if flat:
                for item in flat:
                    node = f(item)
                    pos += node.node_size
                    result.append(node)
            else:
                node = f(child)
                pos += node.node_size
                result.append(node)
    return result, tag


def _take_attrs(attrs, args):
    if not args:
        return attrs, args
    a0 = args[0]
    if a0 and (
        isinstance(a0, str | Node)
        or getattr(a0, "flat", None)
        or (isinstance(a0, dict) and "flat" in a0)
    ):
        return attrs, args
    args = args[1:]
    if not attrs:
        return a0, args
    if not a0:
        return attrs, args
    result = {**attrs, **a0}
    return result, args


def block(type_, attrs=None):
    def result(*args):
        my_attrs, args = _take_attrs(attrs, args)
        nodes, tag = flatten(_test_schema, args, lambda x: x)
        node = type_.create(my_attrs, nodes)
        if tag != NO_TAG:
            node.tag = tag
        return node

    if type_.is_leaf:
        try:
            result.flat = [type_.create(attrs)]
        except ValueError:
            pass

    return result


def mark(type_, attrs=None):
    def result(*args):
        my_attrs, args = _take_attrs(attrs, args)
        mk = type_.create(my_attrs)

        def f(n):
            new_marks = mk.add_to_set(n.marks)
            return n.mark(new_marks) if len(new_marks) > len(n.marks) else n

        nodes, tag = flatten(_test_schema, args, f)
        return {"flat": nodes, "tag": tag}

    return result


def builders(schema, names):
    result = {"schema": schema}
    for name in schema.nodes:
        result[name] = block(schema.nodes[name], {})
    for name in schema.marks:
        result[name] = mark(schema.marks[name], {})

    if names:
        for name, value in names.items():
            type_name = value.get("nodeType") or value.get("markType") or name
            attrs = {k: v for k, v in value.items() if k not in ("nodeType", "markType")}
            type_ = schema.nodes.get(type_name)
            if type_:
                result[name] = block(type_, attrs)
            else:
                type_ = schema.marks.get(type_name)
                if type_:
                    result[name] = mark(type_, attrs)
    return result


out = builders(
    _test_schema,
    {
        "doc": {"nodeType": "doc"},
        "docMetaOne": {"nodeType": "doc", "meta": 1},
        "docMetaTwo": {"nodeType": "doc", "meta": 2},
        "p": {"nodeType": "paragraph"},
        "pre": {"nodeType": "code_block"},
        "h1": {"nodeType": "heading", "level": 1},
        "h2": {"nodeType": "heading", "level": 2},
        "h3": {"nodeType": "heading", "level": 3},
        "li": {"nodeType": "list_item"},
        "ul": {"nodeType": "bullet_list"},
        "ol": {"nodeType": "ordered_list"},
        "br": {"nodeType": "hard_break"},
        "img": {"nodeType": "image", "src": "img.png"},
        "hr": {"nodeType": "horizontal_rule"},
        "a": {"markType": "link", "href": "foo"},
    },
)


test_schema = _test_schema


def eq(a: Node, b: Node) -> bool:
    return a.eq(b)
