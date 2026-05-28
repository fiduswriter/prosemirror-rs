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
    ContentMatch,
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


def _node_type_create_checked(self, attrs=None, content=None, marks=None):
    node = self.create(attrs, content, marks)
    node.check()
    return node


NodeType.create_checked = _node_type_create_checked


def _fragment_nodes_between(self, from_, to, f, node_start=0):
    """Upstream-compatible Fragment.nodes_between."""
    pos = 0
    i = 0
    while i < self.child_count and pos < to:
        child = self.child(i)
        end = pos + child.node_size
        if end > from_:
            if f(child, node_start + pos, self, i) and child.child_count > 0:
                child.content.nodes_between(
                    max(0, from_ - pos - 1),
                    min(child.content.size, to - pos - 1),
                    f,
                    node_start + pos + 1,
                )
        pos = end
        i += 1


Fragment.nodes_between = _fragment_nodes_between


def _node_nodes_between(self, from_, to, f, node_start=0):
    """Upstream-compatible Node.nodes_between."""
    if not self.is_text and self.content.size > 0:
        self.content.nodes_between(from_, to, f, node_start)


Node.nodes_between = _node_nodes_between


# Save the original Rust methods we may delegate to
_original_text_between = Node.text_between
_original_node_str = Node.__str__
_original_fragment_str = Fragment.__str__
_original_text_content = Node.text_content.__get__


def _get_node_spec_callable(node, key):
    """Retrieve a callable schema spec value (e.g. toDebugString, leafText)."""
    schema = node.type.schema
    raw = schema.raw_spec
    if raw is None:
        return None
    nodes = raw.get("nodes")
    if not nodes:
        return None
    spec = nodes.get(node.type.name)
    if not spec:
        return None
    val = spec.get(key)
    if callable(val):
        return val
    return None


def _node_str(self):
    """Upstream-compatible __str__ that respects toDebugString specs."""
    to_debug = _get_node_spec_callable(self, "toDebugString")
    if to_debug is not None:
        return to_debug(self)
    return _original_node_str(self)


Node.__str__ = _node_str


def _fragment_str(self):
    """Upstream-compatible __str__ that respects toDebugString specs."""
    inner = ", ".join(str(self.child(i)) for i in range(self.child_count))
    return f"<{inner}>"


Fragment.__str__ = _fragment_str


def _node_text_content(self):
    """Upstream-compatible text_content that respects leafText specs."""
    if self.is_text:
        return self.text or ""
    leaf_text = _get_node_spec_callable(self, "leafText")
    if leaf_text is not None:
        return leaf_text(self)
    if self.is_leaf:
        return ""
    return _original_text_content(self)


Node.text_content = property(_node_text_content)


def _fragment_text_between(self, from_, to, block_separator="", leaf_text=None):
    """Upstream-compatible Fragment.text_between."""
    text = []
    first = True

    def iteratee(node, pos, _parent, _index):
        nonlocal text, first
        if node.is_text:
            node_text = node.text
        elif node.is_leaf:
            if leaf_text is not None:
                if callable(leaf_text):
                    node_text = leaf_text(node)
                else:
                    node_text = leaf_text
            else:
                schema_leaf = _get_node_spec_callable(node, "leafText")
                if schema_leaf is not None:
                    node_text = schema_leaf(node)
                else:
                    node_text = ""
        else:
            node_text = ""

        if (
            node.is_block
            and ((node.is_leaf and node_text) or node.type.is_textblock)
            and block_separator
        ):
            if first:
                first = False
            else:
                text.append(block_separator)
        text.append(node_text)
        return True

    self.nodes_between(from_, to, iteratee, 0)
    return "".join(text)


Fragment.text_between = _fragment_text_between


def _node_text_between(self, from_, to, block_separator="", leaf_text=None):
    """Upstream-compatible Node.text_between."""
    return self.content.text_between(from_, to, block_separator, leaf_text)


Node.text_between = _node_text_between

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
    "ContentMatch",
]
