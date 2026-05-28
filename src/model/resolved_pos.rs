use super::{fragment::IndexError, Fragment, Mark, MarkType, Node, Schema};
use derivative::Derivative;
use displaydoc::Display;
use std::borrow::Cow;
use std::fmt;
use thiserror::Error;

/// Errors at `resolve`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Display, Error)]
pub enum ResolveErr {
    /// Position {pos} out of range
    RangeError {
        /// The position that was out of range
        pos: usize,
    },
    /// Index error
    Index(#[from] IndexError),
}

#[derive(Derivative, new)]
#[derivative(PartialEq(bound = ""), Eq(bound = ""))]
/// A node in the resolution path
pub struct ResolvedNode<'a, S: Schema> {
    /// Reference to the node
    pub node: &'a S::Node,
    /// Index of the in the parent fragment
    pub index: usize,
    /// Offset immediately before the node
    pub before: usize,
}

impl<'a, S: Schema> Clone for ResolvedNode<'a, S> {
    fn clone(&self) -> Self {
        Self {
            node: self.node,
            index: self.index,
            before: self.before,
        }
    }
}

impl<'a, S: Schema> fmt::Debug for ResolvedNode<'a, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedNode")
            .field("node.type", &self.node.r#type())
            .field("index", &self.index)
            .field("before", &self.before)
            .finish()
    }
}

/// You can resolve a position to get more information about it. Objects of this class represent
/// such a resolved position, providing various pieces of context information, and some helper
/// methods.
#[derive(Derivative)]
#[derivative(
    Debug(bound = ""),
    Clone(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = "")
)]
pub struct ResolvedPos<'a, S: Schema> {
    /// The position that was resolved
    pub pos: usize,
    path: Vec<ResolvedNode<'a, S>>,
    /// The offset into the parent node
    pub parent_offset: usize,
    /// The depth of the deepest resolved ancestor
    pub depth: usize,
}

impl<'a, S: Schema> ResolvedPos<'a, S> {
    pub(crate) fn new(pos: usize, path: Vec<ResolvedNode<'a, S>>, parent_offset: usize) -> Self {
        Self {
            depth: path.len() - 1,
            pos,
            path,
            parent_offset,
        }
    }

    /// The parent node that the position points into. Note that even if
    /// a position points into a text node, that node is not considered
    /// the parent—text nodes are ‘flat’ in this model, and have no content.
    pub fn parent(&self) -> &S::Node {
        self.node(self.depth)
    }

    /// The root node in which the position was resolved.
    pub fn doc(&self) -> &S::Node {
        self.node(0)
    }

    /// The ancestor node at the given level. `p.node(p.depth)` is the same as `p.parent()`.
    pub fn node(&self, depth: usize) -> &'a S::Node {
        self.path[depth].node
    }

    /// The index into the ancestor at the given level. If this points at the 3rd node in the
    /// 2nd paragraph on the top level, for example, `p.index(0)` is 1 and `p.index(1)` is 2.
    pub fn index(&self, depth: usize) -> usize {
        self.path[depth].index
    }

    /// The index pointing after this position into the ancestor at the given level.
    pub fn index_after(&self, depth: usize) -> usize {
        let index = self.index(depth);
        if depth == self.depth && self.text_offset() == 0 {
            index
        } else {
            index + 1
        }
    }

    /// The (absolute) position at the start of the node at the given level.
    pub fn start(&self, depth: usize) -> usize {
        if depth == 0 {
            0
        } else {
            self.path[depth - 1].before + 1
        }
    }

    /// The (absolute) position at the end of the node at the given level.
    pub fn end(&self, depth: usize) -> usize {
        self.start(depth) + self.node(depth).content().map(Fragment::size).unwrap_or(0)
    }

    /// The (absolute) position directly before the wrapping node at the given level, or, when
    /// depth is `self.depth + 1`, the original position.
    pub fn before(&self, depth: usize) -> Option<usize> {
        if depth == 0 {
            None
        } else if depth == self.depth + 1 {
            Some(self.pos)
        } else {
            Some(self.path[depth - 1].before)
        }
    }

    /// The (absolute) position directly after the wrapping node at the given level, or the
    /// original position when depth is `self.depth + 1`.
    pub fn after(&self, depth: usize) -> Option<usize> {
        if depth == 0 {
            None
        } else if depth == self.depth + 1 {
            Some(self.pos)
        } else {
            Some(self.path[depth - 1].before + self.path[depth].node.node_size())
        }
    }

    /// When this position points into a text node, this returns the
    /// distance between the position and the start of the text node.
    /// Will be zero for positions that point between nodes.
    pub fn text_offset(&self) -> usize {
        self.pos - self.path.last().unwrap().before
    }

    /// Get the node directly before the position, if any. If the position points into a text node,
    /// only the part of that node before the position is returned.
    pub fn node_before(&self) -> Option<Cow<'_, S::Node>> {
        let index = self.index(self.depth);
        let d_off = self.pos - self.path.last().unwrap().before;
        if d_off > 0 {
            let parent = self.parent();
            let child = parent.child(index).unwrap();
            let cut = child.cut(0..d_off);
            Some(cut)
        } else if index == 0 {
            None
        } else {
            Some(Cow::Borrowed(self.parent().child(index - 1).unwrap()))
        }
    }

    /// Get the node directly after the position, if any. If the position points into a text node,
    /// only the part of that node after the position is returned.
    pub fn node_after(&self) -> Option<Cow<'_, S::Node>> {
        let parent = self.parent();
        let index = self.index(self.depth);
        if index == parent.child_count() {
            return None;
        }
        let d_off = self.pos - self.path.last().unwrap().before;
        let child = parent.child(index).unwrap();
        if d_off > 0 {
            Some(child.cut(d_off..))
        } else {
            Some(Cow::Borrowed(child))
        }
    }

    /// The depth up to which this position and the given (non-resolved)
    /// position share the same parent nodes.
    pub fn shared_depth(&self, pos: usize) -> usize {
        for depth in (1..=self.depth).rev() {
            if self.start(depth) <= pos && self.end(depth) >= pos {
                return depth;
            }
        }
        0
    }

    /// Returns the range around this position that can be lifted.
    #[allow(clippy::type_complexity)]
    pub fn block_range(
        &self,
        other: Option<&ResolvedPos<'a, S>>,
        pred: Option<&dyn Fn(&S::Node) -> bool>,
    ) -> Option<NodeRange<'a, S>> {
        let other = other.unwrap_or(self);
        if other.pos < self.pos {
            return other.block_range(Some(self), pred);
        }
        let start_d =
            self.depth
                .saturating_sub(if self.parent().inline_content() || self.pos == other.pos {
                    1
                } else {
                    0
                });
        for d in (0..=start_d).rev() {
            if other.pos <= self.end(d) && pred.as_ref().is_none_or(|p| p(self.node(d))) {
                return Some(NodeRange::new(self.clone(), other.clone(), d));
            }
        }
        None
    }

    /// Resolve a position in the given document.
    pub fn resolve(doc: &'a S::Node, pos: usize) -> Result<Self, ResolveErr> {
        if pos > doc.content().unwrap().size() {
            return Err(ResolveErr::RangeError { pos });
        }
        let mut path = vec![];
        let mut start = 0;
        let mut parent_offset = pos;
        let mut node = doc;

        loop {
            let Index { index, offset } = node
                .content()
                .unwrap_or(&Fragment::default())
                .find_index(parent_offset, false)?;
            let rem = parent_offset - offset;
            path.push(ResolvedNode {
                node,
                index,
                before: start + offset,
            });
            if rem == 0 {
                break;
            }
            node = node.child(index).unwrap();
            if node.is_text() {
                break;
            }
            parent_offset = rem - 1;
            start += offset + 1;
        }
        Ok(ResolvedPos::new(pos, path, parent_offset))
    }

    /// Returns the active marks at the resolved position.
    /// Filters out non-inclusive marks at the start/end of their spans.
    pub fn marks(&self) -> Vec<S::Mark> {
        let parent = self.parent();
        let index = self.index(self.depth);
        if parent.content().is_none_or(|c| c.size() == 0) {
            return Vec::new();
        }
        if self.text_offset() > 0 {
            return parent
                .child(index)
                .and_then(|c| c.marks())
                .map(|m| m.iter().cloned().collect())
                .unwrap_or_default();
        }
        let main = if index > 0 {
            parent.maybe_child(index - 1)
        } else {
            None
        };
        let other = parent.maybe_child(index);
        let main = match (main, other) {
            (Some(m), _) => m,
            (None, Some(o)) => o,
            _ => return Vec::new(),
        };
        let other_marks = other.and_then(|o| o.marks());
        let mut marks: Vec<S::Mark> = main
            .marks()
            .map(|m| m.iter().cloned().collect())
            .unwrap_or_default();
        let mut i = 0;
        while i < marks.len() {
            if !marks[i].r#type().inclusive()
                && other_marks.is_none_or(|om| !marks[i].is_in_set(om))
            {
                marks.remove(i);
            } else {
                i += 1;
            }
        }
        marks
    }

    /// Returns marks that span across the range from this position to `end`.
    pub fn marks_across(&self, _end: &ResolvedPos<S>) -> Option<Vec<S::Mark>> {
        let after = self.parent().maybe_child(self.index(self.depth));
        if after.is_none() || !after.unwrap().is_inline() {
            return None;
        }
        let marks: Vec<S::Mark> = after
            .unwrap()
            .marks()
            .map(|m| m.iter().cloned().collect())
            .unwrap_or_default();
        Some(marks)
    }

    /// Test whether two resolved positions have the same direct parent node.
    pub fn same_parent(&self, other: &ResolvedPos<S>) -> bool {
        self.pos - self.parent_offset == other.pos - other.parent_offset
    }

    /// Return the later of two resolved positions.
    pub fn max<'b>(&'b self, other: &'b ResolvedPos<'a, S>) -> &'b ResolvedPos<'a, S> {
        if other.pos > self.pos {
            other
        } else {
            self
        }
    }

    /// Return the earlier of two resolved positions.
    pub fn min<'b>(&'b self, other: &'b ResolvedPos<'a, S>) -> &'b ResolvedPos<'a, S> {
        if other.pos < self.pos {
            other
        } else {
            self
        }
    }

    /// Return the position at a given child index within the parent at the given depth.
    pub fn pos_at_index(&self, index: usize, depth: Option<usize>) -> usize {
        let depth = depth.unwrap_or(self.depth);
        let node = self.node(depth);
        let mut pos = if depth == 0 {
            0
        } else {
            self.path[depth - 1].before + 1
        };
        for i in 0..index {
            pos += node.child(i).map(|c| c.node_size()).unwrap_or(0);
        }
        pos
    }
}

/// A range between two resolved positions at a given depth.
#[derive(Derivative)]
#[derivative(Debug(bound = ""), Clone(bound = ""))]
pub struct NodeRange<'a, S: Schema> {
    /// The start resolved position
    pub from: ResolvedPos<'a, S>,
    /// The end resolved position
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Index {
    pub index: usize,
    pub offset: usize,
}

impl Index {
    #[allow(unused)]
    pub fn new(index: usize, offset: usize) -> Index {
        Index { index, offset }
    }
}
