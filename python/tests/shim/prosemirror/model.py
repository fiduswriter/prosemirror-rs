from prosemirror_rs import (
    Schema,
    Node,
    NodeType,
    Fragment,
    Slice,
    ResolvedPos,
    Mark,
    MarkType,
    MarkSet,
)

# Upstream-compatible class attributes
Fragment.empty = Fragment()
Slice.empty = Slice(Fragment.empty, 0, 0)


def _fragment_from(cls, nodes):
    """Upstream-compatible Fragment.from_ constructor."""
    if not nodes:
        return Fragment.empty
    if isinstance(nodes, Fragment):
        return nodes
    if isinstance(nodes, list):
        return Fragment.from_array(nodes)
    if isinstance(nodes, Node):
        return Fragment.from_array([nodes])
    raise TypeError(f"Unexpected nodes type: {type(nodes)}")


Fragment.from_ = classmethod(_fragment_from)


def _mark_same_set(a, b):
    if a == b:
        return True
    if len(a) != len(b):
        return False
    return all(item_a.eq(item_b) for item_a, item_b in zip(a, b, strict=True))


Mark.same_set = staticmethod(_mark_same_set)


def _node_type_create_and_fill(self, attrs=None, content=None, marks=None):
    # Stub: returns None so tests can at least collect.
    # Real implementation needs ContentMatch which is not yet exposed.
    return None


NodeType.create_and_fill = _node_type_create_and_fill

__all__ = [
    "Schema",
    "Node",
    "NodeType",
    "Fragment",
    "Slice",
    "ResolvedPos",
    "Mark",
    "MarkType",
    "MarkSet",
]
