use super::{util, Index, Node, Schema, Text};
use derivative::Derivative;
use displaydoc::Display;
use serde::{Deserialize, Serialize, Serializer};
use std::borrow::Cow;
use std::ops::RangeBounds;
use thiserror::Error;

#[derive(Copy, Clone, Debug, Error, Display, PartialEq, Eq)]
pub enum IndexError {
    /// Index is out of bounds {0}
    OutOfBounds(usize),
}

/// A fragment represents a node's collection of child nodes.
///
/// Like nodes, fragments are persistent data structures, and you should not mutate them or their
/// content. Rather, you create new instances whenever needed. The API tries to make this easy.
#[derive(Derivative, Deserialize, Eq)]
#[derivative(Debug(bound = ""), Clone(bound = ""), PartialEq(bound = ""))]
#[serde(from = "Vec<S::Node>")]
pub struct Fragment<S: Schema> {
    inner: Vec<S::Node>,
    size: usize,
}

impl<S: Schema> Fragment<S> {
    /// An empty fragment
    pub const EMPTY: Self = Fragment {
        inner: Vec::new(),
        size: 0,
    };
    /// Reference to an empty fragment
    pub const EMPTY_REF: &'static Self = &Self::EMPTY;

    /// Slice a fragment by child index (no node splitting).
    pub fn cut_by_index(&self, from: usize, to: usize) -> Self {
        if from == to {
            return Fragment::new();
        }
        if from == 0 && to == self.inner.len() {
            return self.clone();
        }
        Fragment::from(self.inner[from..to].to_vec())
    }

    /// Prepend a single node to this fragment.
    pub fn add_to_start(&self, node: S::Node) -> Self {
        let node_size = node.node_size();
        let mut new_inner = Vec::with_capacity(self.inner.len() + 1);
        new_inner.push(node);
        new_inner.extend_from_slice(&self.inner);
        Fragment {
            size: self.size + node_size,
            inner: new_inner,
        }
    }

    /// Append a single node to this fragment.
    pub fn add_to_end(&self, node: S::Node) -> Self {
        let node_size = node.node_size();
        let mut new_inner = self.inner.clone();
        new_inner.push(node);
        Fragment {
            inner: new_inner,
            size: self.size + node_size,
        }
    }

    /// Iterate over children calling `f(node, offset, index)`.
    pub fn for_each<F: FnMut(&S::Node, usize, usize)>(&self, mut f: F) {
        let mut pos = 0;
        for (i, child) in self.inner.iter().enumerate() {
            f(child, pos, i);
            pos += child.node_size();
        }
    }

    /// Get the node at the given document position within this fragment.
    /// Walks down the tree until finding the node that covers the position.
    pub fn node_at(&self, pos: usize) -> Option<&S::Node> {
        let mut remaining = pos;
        for child in &self.inner {
            let end = child.node_size();
            if end > remaining {
                if child.is_leaf() || remaining == 0 {
                    return Some(child);
                }
                // For non-leaf nodes, descend into content (skip the opening token)
                if let Some(content) = child.content() {
                    return content.node_at(remaining - 1);
                }
                return Some(child);
            }
            remaining -= end;
        }
        None
    }

    /// Create a new empty fragment
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a fragment from an array of nodes, joining adjacent text nodes
    /// with the same marks (matching ProseMirror JS `Fragment.fromArray`).
    pub fn from_array(mut nodes: Vec<S::Node>) -> Self {
        if nodes.is_empty() {
            return Fragment::new();
        }
        let mut result = vec![nodes.remove(0)];
        for node in nodes {
            let last = result.last_mut().unwrap();
            if let Some(n1) = last.text_node() {
                if let Some(n2) = n1.same_markup(&node) {
                    let mid =
                        n1.with_text(Text::from(n1.text.as_str().to_owned() + n2.text.as_str()));
                    *last = S::Node::from(mid);
                    continue;
                }
            }
            result.push(node);
        }
        Fragment::from(result)
    }

    /// The size of the fragment, which is the total of the size of its content nodes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get a slice to all child nodes
    pub fn children(&self) -> &[S::Node] {
        &self.inner[..]
    }

    /// The first child of the fragment wrapped in `Some`, or `None` if it is empty.
    pub fn first_child(&self) -> Option<&S::Node> {
        self.inner.first()
    }

    /// The last child of the fragment wrapped in `Some`, or `None` if it is empty.
    pub fn last_child(&self) -> Option<&S::Node> {
        self.inner.last()
    }

    /// The number of child nodes in this fragment.
    pub fn child_count(&self) -> usize {
        self.inner.len()
    }

    /// Find the first position at which this fragment and another fragment differ,
    /// or `None` if they are the same.
    pub fn find_diff_start(&self, other: &Fragment<S>, pos: usize) -> Option<usize> {
        let mut i = 0;
        let mut pos = pos;
        loop {
            if i == self.inner.len() || i == other.inner.len() {
                return if self.inner.len() == other.inner.len() {
                    None
                } else {
                    Some(pos)
                };
            }
            let child_a = &self.inner[i];
            let child_b = &other.inner[i];
            if child_a == child_b {
                pos += child_a.node_size();
                i += 1;
                continue;
            }
            if !child_a.same_markup(child_b) {
                return Some(pos);
            }
            if let (Some(a), Some(b)) = (child_a.text_node(), child_b.text_node()) {
                if a.text != b.text {
                    let mut j = 0;
                    let a_str = a.text.as_str();
                    let b_str = b.text.as_str();
                    while j < a_str.len()
                        && j < b_str.len()
                        && a_str.as_bytes()[j] == b_str.as_bytes()[j]
                    {
                        j += 1;
                    }
                    return Some(pos + j);
                }
            }
            if child_a.child_count() > 0 || child_b.child_count() > 0 {
                if let (Some(a_content), Some(b_content)) = (child_a.content(), child_b.content()) {
                    if let Some(inner) = a_content.find_diff_start(b_content, pos + 1) {
                        return Some(inner);
                    }
                }
            }
            pos += child_a.node_size();
            i += 1;
        }
    }

    /// Find the last positions at which this fragment and another fragment differ,
    /// scanning from the end. Returns `None` if they are the same.
    pub fn find_diff_end(
        &self,
        other: &Fragment<S>,
        mut pos_a: usize,
        mut pos_b: usize,
    ) -> Option<(usize, usize)> {
        let mut i_a = self.inner.len();
        let mut i_b = other.inner.len();
        loop {
            if i_a == 0 || i_b == 0 {
                return if i_a == i_b {
                    None
                } else {
                    Some((pos_a, pos_b))
                };
            }
            i_a -= 1;
            i_b -= 1;
            let child_a = &self.inner[i_a];
            let child_b = &other.inner[i_b];
            let size = child_a.node_size();
            if child_a == child_b {
                pos_a -= size;
                pos_b -= size;
                continue;
            }
            if !child_a.same_markup(child_b) {
                return Some((pos_a, pos_b));
            }
            if let (Some(a), Some(b)) = (child_a.text_node(), child_b.text_node()) {
                if a.text != b.text {
                    let a_str = a.text.as_str();
                    let b_str = b.text.as_str();
                    let mut same = 0;
                    let min_size = a_str.len().min(b_str.len());
                    while same < min_size
                        && a_str.as_bytes()[a_str.len() - same - 1]
                            == b_str.as_bytes()[b_str.len() - same - 1]
                    {
                        same += 1;
                        pos_a -= 1;
                        pos_b -= 1;
                    }
                    return Some((pos_a, pos_b));
                }
            }
            if child_a.child_count() > 0 || child_b.child_count() > 0 {
                if let (Some(a_content), Some(b_content)) = (child_a.content(), child_b.content()) {
                    if let Some(inner) = a_content.find_diff_end(b_content, pos_a - 1, pos_b - 1) {
                        return Some(inner);
                    }
                }
            }
            pos_a -= size;
            pos_b -= size;
        }
    }

    /// Create a new fragment containing the combined content of this fragment and the other.
    pub fn append(mut self, mut other: Self) -> Self {
        if let Some(first) = other.first_child() {
            if let Some(last) = self.inner.last_mut() {
                if let Some(n1) = last.text_node() {
                    if let Some(n2) = n1.same_markup(first) {
                        let mid = n1
                            .with_text(Text::from(n1.text.as_str().to_owned() + n2.text.as_str()));
                        *last = S::Node::from(mid);
                        other.inner.remove(0);
                    }
                }

                self.inner.append(&mut other.inner);
                self.size += other.size;
                self
            } else {
                other
            }
        } else {
            self
        }
    }

    /// Cut out the sub-fragment between the two given positions.
    pub fn cut<R: RangeBounds<usize>>(&self, range: R) -> Self {
        let from = util::from(&range);
        let to = util::to(&range, self.size);

        if from == 0 && to == self.size {
            return self.clone();
        }

        let mut result = vec![];
        let mut size = 0;
        if to > from {
            let mut pos = 0;
            let mut i = 0;
            while pos < to {
                let child = &self.inner[i];
                let end = pos + child.node_size();
                if end > from {
                    let new_child = if pos < from || end > to {
                        if let Some(node) = child.text_node() {
                            let len = node.text.len_utf16();
                            let start = from.saturating_sub(pos);
                            let end = usize::min(len, to - pos);
                            child.cut(start..end)
                        } else {
                            let t = pos + 1;
                            let start = from.saturating_sub(t);
                            let end = usize::min(child.content_size(), to - t);
                            child.cut(start..end)
                        }
                        .into_owned()
                    } else {
                        child.clone()
                    };
                    size += new_child.node_size();
                    result.push(new_child);
                }
                pos = end;
                i += 1;
            }
        }
        Fragment {
            inner: result,
            size,
        }
    }

    /// Invoke a callback for all descendant nodes between the given two positions (relative to
    /// start of this fragment). Doesn't descend into a node when the callback returns `false`.
    pub fn nodes_between<F: FnMut(&S::Node, usize, Option<&S::Node>, usize) -> bool>(
        &self,
        from: usize,
        to: usize,
        f: &mut F,
        node_start: usize,
        parent: Option<&S::Node>,
    ) {
        let mut pos = 0;
        for (index, child) in self.inner.iter().enumerate() {
            let end = pos + child.node_size();
            if pos >= to {
                break;
            }
            if end > from && f(child, node_start + pos, parent, index) {
                if let Some(content) = child.content() {
                    let start = pos + 1;
                    content.nodes_between(
                        from.saturating_sub(start),
                        content.size().min(to.saturating_sub(start)),
                        f,
                        node_start + start,
                        Some(child),
                    )
                }
            }
            pos = end;
        }
    }

    /// Get all text between positions from and to. When `block_separator` is given, it will be
    /// inserted whenever a new block node is started. When `leaf_text` is given, it'll be inserted
    /// for every non-text leaf node encountered.
    pub fn text_between(
        &self,
        text: &mut String,
        mut separated: bool,
        from: usize,
        to: usize,
        block_separator: Option<&str>,
        leaf_text: Option<&str>,
    ) {
        self.nodes_between(
            from,
            to,
            &mut move |node, pos, _parent, _index| {
                if let Some(txt_node) = node.text_node() {
                    let txt = &txt_node.text;
                    let (rest, skip) = if from > pos {
                        let skip = from - pos;
                        (util::split_at_utf16(txt.as_str(), skip).1, skip)
                    } else {
                        (txt.as_str(), 0)
                    };

                    let end = to - pos;
                    let slice = util::split_at_utf16(rest, end - skip).0;

                    text.push_str(slice);
                    separated = block_separator.is_none();
                } else if node.is_leaf() {
                    if let Some(leaf_text) = leaf_text {
                        text.push_str(leaf_text);
                    }
                    separated = block_separator.is_none();
                } else if !separated && node.is_block() {
                    text.push_str(block_separator.unwrap_or(""));
                    separated = true
                }
                true
            },
            0,
            None,
        )
    }
    /// Create a new fragment in which the node at the given index is replaced by the given node.
    pub fn replace_child(&self, index: usize, node: S::Node) -> Cow<'_, Self> {
        let (before, rest) = self.inner.split_at(index);
        let (current, after) = rest.split_first().unwrap();

        if *current == node {
            Cow::Borrowed(self)
        } else {
            let size = self.size + node.node_size() - current.node_size();
            let mut copy = Vec::with_capacity(self.inner.capacity());
            copy.extend_from_slice(before);
            copy.push(node);
            copy.extend_from_slice(after);
            Cow::Owned(Fragment { inner: copy, size })
        }
    }

    /// Get the child node at the given index. Panics when the index is out of range.
    pub fn child(&self, index: usize) -> &S::Node {
        &self.inner[index]
    }

    /// Get the child node at the given index, if it exists.
    pub fn maybe_child(&self, index: usize) -> Option<&S::Node> {
        self.inner.get(index)
    }

    pub(crate) fn find_index(&self, pos: usize, round: bool) -> Result<Index, IndexError> {
        let len = self.inner.len();
        match pos {
            0 => Ok(Index {
                index: 0,
                offset: pos,
            }),
            p if p == self.size => Ok(Index {
                index: len,
                offset: pos,
            }),
            p if p > self.size => Err(IndexError::OutOfBounds(p)),
            p => {
                let mut cur_pos = 0;
                for (i, cur) in self.inner.iter().enumerate() {
                    let end = cur_pos + cur.node_size();
                    if end >= p {
                        if (end == p) || round {
                            return Ok(Index {
                                index: i + 1,
                                offset: end,
                            });
                        } else {
                            return Ok(Index {
                                index: i,
                                offset: cur_pos,
                            });
                        }
                    }
                    cur_pos = end;
                }
                panic!("Invariant failed: self.size must be the sum of all node sizes")
            }
        }
    }
}

impl<S: Schema> Default for Fragment<S> {
    fn default() -> Self {
        Self {
            inner: Vec::new(),
            size: 0,
        }
    }
}

impl<S: Schema> Serialize for Fragment<S> {
    fn serialize<Sr>(&self, serializer: Sr) -> Result<Sr::Ok, Sr::Error>
    where
        Sr: Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<S: Schema> From<Vec<S::Node>> for Fragment<S> {
    fn from(src: Vec<S::Node>) -> Fragment<S> {
        let size = src.iter().map(|x| x.node_size()).sum::<usize>();
        Fragment { inner: src, size }
    }
}

impl<S: Schema> From<Fragment<S>> for Vec<S::Node> {
    fn from(src: Fragment<S>) -> Vec<S::Node> {
        src.inner
    }
}

impl<S, A, B> From<(A, B)> for Fragment<S>
where
    S: Schema,
    A: Into<S::Node>,
    B: Into<S::Node>,
{
    fn from((a, b): (A, B)) -> Self {
        Self::from(vec![a.into(), b.into()])
    }
}

impl<N, S: 'static, A> From<(A,)> for Fragment<S>
where
    N: Node<S>,
    S: Schema<Node = N>,
    A: Into<N>,
{
    fn from((a,): (A,)) -> Self {
        Self::from(vec![a.into()])
    }
}
