"""Shim for prosemirror.transform — re-exports from prosemirror_rs."""

from prosemirror_rs import (
    Transform,
    Step,
    StepMap,
    Mapping,
    StepResult,
    MapResult,
    NodeRange,
    lift_target as _lift_target,
    can_split as _can_split,
    find_wrapping as _find_wrapping,
    can_join,
    join_point,
    insert_point,
    drop_point,
)


def _unwrap_node_type(obj):
    """Extract a NodeType from NodeType or NodeTypeWithAttrs."""
    if isinstance(obj, NodeTypeWithAttrs):
        return obj.type
    return obj


def can_split(doc, pos, depth=None, types_after=None):
    if types_after:
        types_after = [_unwrap_node_type(t) for t in types_after]
    return _can_split(doc, pos, depth, types_after)


def find_wrapping(range, node_type):
    node_type = _unwrap_node_type(node_type)
    return _find_wrapping(range, node_type)


# lift_target doesn't take node types, so pass through
lift_target = _lift_target


def ReplaceStep(from_, to, slice=None, structure=False):
    return Step.replace(from_, to, slice, structure)


def AddMarkStep(from_, to, mark):
    return Step.add_mark(from_, to, mark)


def RemoveMarkStep(from_, to, mark):
    return Step.remove_mark(from_, to, mark)


class TransformError(Exception):
    pass


class NodeTypeWithAttrs:
    def __init__(self, type_=None, attrs=None, **kwargs):
        # upstream passes keyword argument `type=...`
        self.type = kwargs.get("type", type_)
        self.attrs = attrs or {}


__all__ = [
    "Transform",
    "TransformError",
    "Step",
    "AddMarkStep",
    "RemoveMarkStep",
    "ReplaceStep",
    "StepMap",
    "Mapping",
    "StepResult",
    "MapResult",
    "NodeRange",
    "can_split",
    "find_wrapping",
    "lift_target",
    "NodeTypeWithAttrs",
]
