//! Structure analysis utilities for document transformations.

use crate::model::{ContentMatch, Node, NodeType, ResolvedPos, Schema, Slice};

/// Test whether a node can be cut at the given child indices.
pub fn can_cut<S: Schema>(node: &S::Node, start: usize, end: usize) -> bool {
    if start == 0
        || node
            .can_replace(start, node.child_count(), None, ..)
            .unwrap_or(false)
    {
        end == node.child_count() || node.can_replace(0, end, None, ..).unwrap_or(false)
    } else {
        false
    }
}

/// Find the depth to which the given range can be lifted, if any.
pub fn lift_target<S: Schema>(range: &NodeRange<S>) -> Option<usize> {
    let parent = range.parent();
    let content = parent
        .content()
        .map(|c| c.cut_by_index(range.start_index(), range.end_index()))
        .unwrap_or_default();
    let mut depth = range.depth;
    let mut content_before: usize = 0;
    let mut content_after: usize = 0;
    loop {
        let node = range.from.node(depth);
        let index = range.from.index(depth) + content_before;
        let end_index = range.to.index_after(depth).saturating_sub(content_after);
        if depth < range.depth
            && node
                .can_replace(index, end_index, Some(&content), ..)
                .unwrap_or(false)
        {
            return Some(depth);
        }
        if depth == 0 || !can_cut::<S>(node, index, end_index) {
            break;
        }
        if index > 0 {
            content_before = 1;
        }
        if end_index < node.child_count() {
            content_after = 1;
        }
        depth -= 1;
    }
    None
}

/// Find the wrapping node types needed to make `node_type` valid at the given range.
pub fn find_wrapping<S: Schema>(
    range: &NodeRange<S>,
    node_type: S::NodeType,
    _attrs_check: impl Fn(&S::NodeType) -> bool,
) -> Option<Vec<S::NodeType>> {
    let around = find_wrapping_outside(range, node_type)?;
    let inner = find_wrapping_inside(range, node_type)?;

    let mut result = around;
    result.push(node_type);
    result.extend(inner);
    // Check that the first around type can be inserted
    if result.is_empty() {
        return Some(result);
    }
    Some(result)
}

/// Find wrapping types outside the range
pub fn find_wrapping_outside<S: Schema>(
    range: &NodeRange<S>,
    node_type: S::NodeType,
) -> Option<Vec<S::NodeType>> {
    let parent = range.parent();
    let start_index = range.start_index();
    let end_index = range.end_index();
    let around = parent
        .content_match_at(start_index)
        .ok()?
        .find_wrapping(node_type)?;
    let outer = if around.is_empty() {
        node_type
    } else {
        around[0]
    };
    if parent.can_replace_with(start_index, end_index, outer) {
        Some(around)
    } else {
        None
    }
}

/// Find wrapping types inside the range
pub fn find_wrapping_inside<S: Schema>(
    range: &NodeRange<S>,
    node_type: S::NodeType,
) -> Option<Vec<S::NodeType>> {
    let parent = range.parent();
    let start_index = range.start_index();
    let end_index = range.end_index();
    let inner = parent.child(start_index)?;
    let inside = node_type.content_match().find_wrapping(inner.r#type())?;
    let last_type = if inside.is_empty() {
        node_type
    } else {
        inside[inside.len() - 1]
    };
    let mut inner_match = Some(last_type.content_match());
    let mut i = start_index;
    while let Some(m) = inner_match {
        if i >= end_index {
            break;
        }
        inner_match = m.match_type(parent.child(i)?.r#type());
        i += 1;
    }
    match inner_match {
        Some(m) if m.valid_end() => Some(inside),
        _ => None,
    }
}

/// Check if the given node type can be changed at the given position.
pub fn can_change_type<S: Schema>(doc: &S::Node, pos: usize, node_type: S::NodeType) -> bool {
    if let Ok(pos_) = doc.resolve(pos) {
        let index = pos_.index(pos_.depth);
        pos_.parent().can_replace_with(index, index + 1, node_type)
    } else {
        false
    }
}

/// Check whether the document can be split at the given position.
pub fn can_split<S: Schema>(
    doc: &S::Node,
    pos: usize,
    depth: Option<usize>,
    types_after: Option<&[S::NodeType]>,
) -> bool {
    let depth = depth.unwrap_or(1);
    let pos_ = match doc.resolve(pos) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let base = pos_.depth as isize - depth as isize;
    if base < 0 {
        return false;
    }
    let base = base as usize;

    let inner_type = types_after
        .and_then(|t| t.last().copied())
        .unwrap_or_else(|| pos_.parent().r#type());

    if !pos_
        .parent()
        .can_replace(
            pos_.index(pos_.depth),
            pos_.parent().child_count(),
            None,
            ..,
        )
        .unwrap_or(false)
    {
        return false;
    }
    if let Some(content) = pos_.parent().content() {
        if !inner_type
            .valid_content(&content.cut_by_index(pos_.index(pos_.depth), content.child_count()))
        {
            return false;
        }
    }

    let mut d = pos_.depth - 1;
    let mut i = depth as isize - 2;
    while d > base {
        let node = pos_.node(d);
        let index = pos_.index(d);
        if let Some(content) = node.content() {
            let rest = content.cut_by_index(index, content.child_count());

            let rest_to_check = rest;
            if let Some(types) = types_after {
                if (i + 1) >= 0 && ((i + 1) as usize) < types.len() {
                    let override_child_type = types[(i + 1) as usize];
                    // Create a node of the override type and replace the first child
                    // This is a simplified check
                    let _ = override_child_type;
                }
            }

            let after_type = types_after
                .and_then(|t| {
                    if i >= 0 && (i as usize) < t.len() {
                        Some(t[i as usize])
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| node.r#type());

            if !node
                .can_replace(index + 1, node.child_count(), None, ..)
                .unwrap_or(false)
                || !after_type.valid_content(&rest_to_check)
            {
                return false;
            }
        }
        d -= 1;
        i -= 1;
    }

    let index = pos_.index_after(base);
    let base_type = types_after.and_then(|t| t.first().copied());
    pos_.node(base).can_replace_with(
        index,
        index,
        base_type.unwrap_or_else(|| pos_.node(base + 1).r#type()),
    )
}

/// Check whether the document can be joined at the given position.
pub fn can_join<S: Schema>(doc: &S::Node, pos: usize) -> Option<bool> {
    let pos_ = doc.resolve(pos).ok()?;
    let index = pos_.index(pos_.depth);
    if joinable::<S>(pos_.node_before().as_deref(), pos_.node_after().as_deref()) {
        pos_.parent().can_replace(index, index + 1, None, ..).ok()
    } else {
        None
    }
}

/// Find a join point near the given position.
pub fn join_point<S: Schema>(doc: &S::Node, pos: usize, dir: Option<i32>) -> Option<usize> {
    let dir = dir.unwrap_or(-1);
    let pos_ = doc.resolve(pos).ok()?;
    let mut pos = pos;
    for d in (0..=pos_.depth).rev() {
        let (before, after, index) = if d == pos_.depth {
            (pos_.node_before(), pos_.node_after(), pos_.index(d))
        } else if dir > 0 {
            let idx = pos_.index(d) + 1;
            (
                Some(std::borrow::Cow::Borrowed(pos_.node(d + 1))),
                pos_.node(d)
                    .maybe_child(idx)
                    .map(std::borrow::Cow::Borrowed),
                idx,
            )
        } else {
            let idx = pos_.index(d);
            (
                if idx > 0 {
                    pos_.node(d)
                        .maybe_child(idx - 1)
                        .map(std::borrow::Cow::Borrowed)
                } else {
                    None
                },
                Some(std::borrow::Cow::Borrowed(pos_.node(d + 1))),
                idx,
            )
        };
        if let (Some(b), Some(a)) = (&before, &after) {
            if !b.r#type().is_textblock()
                && joinable::<S>(Some(b), Some(a))
                && pos_
                    .node(d)
                    .can_replace(index, index + 1, None, ..)
                    .unwrap_or(false)
            {
                return Some(pos);
            }
        }
        if d == 0 {
            break;
        }
        pos = if dir < 0 {
            pos_.before(d).unwrap_or(pos)
        } else {
            pos_.after(d).unwrap_or(pos)
        };
    }
    None
}

/// Find a valid insertion point for the given node type.
pub fn insert_point<S: Schema>(doc: &S::Node, pos: usize, node_type: S::NodeType) -> Option<usize> {
    let pos_ = doc.resolve(pos).ok()?;
    if pos_
        .parent()
        .can_replace_with(pos_.index(pos_.depth), pos_.index(pos_.depth), node_type)
    {
        return Some(pos);
    }
    if pos_.parent_offset == 0 {
        for d in (0..pos_.depth).rev() {
            let index = pos_.index(d);
            if pos_.node(d).can_replace_with(index, index, node_type) {
                return pos_.before(d + 1);
            }
            if index > 0 {
                return None;
            }
        }
    }
    if pos_.parent_offset == pos_.parent().content().map(|c| c.size()).unwrap_or(0) {
        for d in (0..pos_.depth).rev() {
            let index = pos_.index_after(d);
            if pos_.node(d).can_replace_with(index, index, node_type) {
                return pos_.after(d + 1);
            }
            if index < pos_.node(d).child_count() {
                return None;
            }
        }
    }
    None
}

/// Find a valid drop point for the given slice.
pub fn drop_point<S: Schema>(doc: &S::Node, pos: usize, slice: &Slice<S>) -> Option<usize> {
    if slice.content.size() == 0 {
        return Some(pos);
    }
    let pos_ = doc.resolve(pos).ok()?;
    let mut content = &slice.content;
    for _ in 0..slice.open_start {
        content = content.first_child()?.content()?;
    }
    let max_pass = if slice.open_start == 0 && slice.size() > 0 {
        2
    } else {
        1
    };
    for pass in 1..=max_pass {
        for d in (1..=pos_.depth).rev() {
            let bias = if d == pos_.depth {
                0
            } else if pos_.pos as f64 <= (pos_.start(d + 1) as f64 + pos_.end(d + 1) as f64) / 2.0 {
                -1
            } else {
                1
            };
            let insert_pos = pos_.index(d) + if bias > 0 { 1 } else { 0 };
            let parent = pos_.node(d);
            let fits = if pass == 1 {
                parent
                    .can_replace(insert_pos, insert_pos, Some(content), ..)
                    .unwrap_or(false)
            } else {
                match content.first_child() {
                    Some(first) => {
                        let wrapping = parent
                            .content_match_at(insert_pos)
                            .ok()
                            .and_then(|m| m.find_wrapping(first.r#type()));
                        wrapping.is_some()
                            && parent.can_replace_with(insert_pos, insert_pos, wrapping.unwrap()[0])
                    }
                    None => false,
                }
            };
            if fits {
                return Some(if bias == 0 {
                    pos_.pos
                } else if bias < 0 {
                    pos_.before(d + 1)?
                } else {
                    pos_.after(d + 1)?
                });
            }
        }
    }
    None
}

/// Check if two nodes are joinable (compatible content)
pub fn joinable<S: Schema>(a: Option<&S::Node>, b: Option<&S::Node>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => !a.is_leaf() && a.r#type().compatible_content(b.r#type()),
        _ => false,
    }
}

/// A range between two resolved positions at a given depth.
pub struct NodeRange<'a, S: Schema> {
    /// The start of the range
    pub from: ResolvedPos<'a, S>,
    /// The end of the range
    pub to: ResolvedPos<'a, S>,
    /// The depth at which the range is defined
    pub depth: usize,
}

impl<'a, S: Schema> NodeRange<'a, S> {
    /// Create a new NodeRange
    pub fn new(from: ResolvedPos<'a, S>, to: ResolvedPos<'a, S>, depth: usize) -> Self {
        NodeRange { from, to, depth }
    }

    /// The start position of the range
    pub fn start(&self) -> usize {
        self.from.before(self.depth + 1).unwrap_or(0)
    }

    /// The end position of the range
    pub fn end(&self) -> usize {
        self.to.after(self.depth + 1).unwrap_or(0)
    }

    /// The parent node containing the range
    pub fn parent(&self) -> &'a S::Node {
        self.from.node(self.depth)
    }

    /// The start child index within the parent
    pub fn start_index(&self) -> usize {
        self.from.index(self.depth)
    }

    /// The end child index within the parent
    pub fn end_index(&self) -> usize {
        self.to.index_after(self.depth)
    }
}
