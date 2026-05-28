use super::{
    replace, util, ContentMatch, ContentMatchError, Fragment, Mark, MarkSet, ReplaceError,
    ResolveErr, ResolvedPos, Schema, Slice, TextNode,
};
use displaydoc::Display;
use serde::{Deserialize, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt::Debug;
use std::ops::RangeBounds;
use thiserror::Error;

#[derive(Debug, Clone, Error, Display, Eq, PartialEq)]
/// Error type raised by `Node::slice` when given an invalid replacement.
pub enum SliceError {
    /// The given span was invalid
    Resolve(#[from] ResolveErr),
    /// Unknown
    Unknown,
}

/// This is the type that encodes a kind of node
pub trait NodeType<S: Schema>: Copy + Clone + Debug + PartialEq + Eq {
    /// Whether two node types have compatible content
    fn compatible_content(self, other: Self) -> bool;
    /// Whether the given fragment is valid content for this node type
    fn valid_content(self, fragment: &Fragment<S>) -> bool;

    /// Check whether the given mark type is allowed in this node.
    fn allows_mark_type(self, mark_type: S::MarkType) -> bool;

    /// Get the content match for this node type
    fn content_match(self) -> S::ContentMatch;

    /// Check whether the given marks is allowed in this node.
    fn allow_marks(self, marks: &MarkSet<S>) -> bool;

    /// True if this is an inline type.
    fn is_inline(self) -> bool {
        !self.is_block()
    }
    /// True if this is a block type
    fn is_block(self) -> bool;

    /// The name of this node type (e.g. "paragraph", "heading")
    fn name(self) -> &'static str {
        ""
    }

    /// Whether this is an atom (leaf) type
    fn is_atom(self) -> bool {
        false
    }

    /// Whether this type is isolating (prevents merging across boundaries)
    fn is_isolating(self) -> bool {
        false
    }

    /// Whether this type is defining (for replaceRange behavior)
    fn is_defining(self) -> bool {
        false
    }

    /// Whether this type is defining as context (for replaceRange behavior)
    fn is_defining_as_context(self) -> bool {
        false
    }

    /// Whether this type is defining for content (for replaceRange behavior)
    fn is_defining_for_content(self) -> bool {
        false
    }

    /// Whether this node type has required attributes (no defaults)
    fn has_required_attrs(self) -> bool {
        false
    }

    /// Whitespace handling mode for this node type (e.g. "pre")
    fn whitespace(self) -> Option<String> {
        None
    }

    /// Create a node of this type, filling in missing required child nodes
    /// if necessary. Returns None if the content cannot be made valid.
    fn create_and_fill(
        self,
        attrs: serde_json::Value,
        content: Option<&Fragment<S>>,
        marks: Option<&MarkSet<S>>,
    ) -> Option<S::Node> {
        let node = self.create(attrs, content, marks);
        if let Some(c) = content {
            if c.size() > 0 {
                let before = self.content_match().fill_before(c, false, 0)?;
                let new_content = before.append(c.clone());
                let matched = self.content_match().match_fragment(&new_content)?;
                let after_fill = matched.fill_before(&Fragment::new(), true, 0)?;
                return Some(self.create(
                    node.attrs_json(),
                    Some(&new_content.append(after_fill)),
                    marks,
                ));
            }
        }
        let matched = self
            .content_match()
            .match_fragment(content.unwrap_or(&Fragment::new()))?;
        let after_fill = matched.fill_before(&Fragment::new(), true, 0)?;
        let final_content = content
            .map(|c| c.clone().append(after_fill.clone()))
            .unwrap_or(after_fill);
        Some(self.create(node.attrs_json(), Some(&final_content), marks))
    }

    /// Whether this is a textblock type (a block that contains inline content)
    fn is_textblock(self) -> bool {
        false
    }

    /// Whether this type expects inline content
    fn inline_content(self) -> bool {
        false
    }

    /// Create a new node of this type with the given content and marks.
    fn create_node(self, content: Option<&Fragment<S>>, marks: Option<&MarkSet<S>>) -> S::Node;

    /// Create a new node of this type with JSON attributes, content, and marks.
    /// This mirrors the JS `NodeType.create(attrs, content, marks)` API.
    fn create(
        self,
        _attrs: serde_json::Value,
        content: Option<&Fragment<S>>,
        marks: Option<&MarkSet<S>>,
    ) -> S::Node {
        // Default: ignore attrs, delegate to create_node
        self.create_node(content, marks)
    }

    /// Check whether a single node of the given type can replace a range at the given indices.
    fn can_replace_with(self, from: usize, to: usize, node_type: S::NodeType) -> bool {
        self.content_match()
            .match_fragment_range(&Fragment::EMPTY, 0..from)
            .and_then(|m| m.match_type(node_type))
            .and_then(|m| m.match_fragment_range(&Fragment::EMPTY, to..))
            .is_some_and(|m| m.valid_end())
    }

    /// Check whether the marks are allowed for this node type
    fn allows_marks(self, marks: &MarkSet<S>) -> bool {
        self.allow_marks(marks)
    }
}

/// This class represents a node in the tree that makes up a ProseMirror document. So a document is
/// an instance of Node, with children that are also instances of Node.
pub trait Node<S: Schema<Node = Self> + 'static>:
    Serialize + for<'de> Deserialize<'de> + Clone + Debug + PartialEq + Eq + Sized + From<TextNode<S>>
{
    /// The size of this node, as defined by the integer-based indexing scheme. For text nodes,
    /// this is the amount of characters. For other leaf nodes, it is one. For non-leaf nodes, it
    /// is the size of the content plus two (the start and end token).
    fn node_size(&self) -> usize {
        match self.content() {
            Some(c) => c.size() + 2,
            None => {
                if let Some(node) = self.text_node() {
                    node.text.len_utf16
                } else {
                    1
                }
            }
        }
    }

    /// The number of children that the node has.
    fn child_count(&self) -> usize {
        self.content().map_or(0, Fragment::child_count)
    }

    /// Get the child node at the given index. Raises an error when the index is out of range.
    fn child(&self, index: usize) -> Option<&Self> {
        self.content().map(|c| c.child(index))
    }

    /// Get the child node at the given index, if it exists.
    fn maybe_child(&self, index: usize) -> Option<&Self> {
        self.content().and_then(|c| c.maybe_child(index))
    }

    /// Create a copy of this node, with the given set of marks instead of the node's own marks.
    fn mark(&self, marks: MarkSet<S>) -> Self;

    /// Create a copy of this node with only the content between the given positions.
    fn cut<R: RangeBounds<usize>>(&self, range: R) -> Cow<'_, Self> {
        let from = util::from(&range);

        if let Some(TextNode { text, marks }) = self.text_node() {
            let len = text.len_utf16;
            let to = util::to(&range, len);

            if from == 0 && to == len {
                return Cow::Borrowed(self);
            }
            let (_, rest) = util::split_at_utf16(&text.content, from);
            let (rest, _) = util::split_at_utf16(rest, to - from);

            Cow::Owned(Self::new_text_node(TextNode {
                text: Text::from(rest.to_owned()),
                marks: marks.clone(),
            }))
        } else {
            let content_size = self.content_size();
            let to = util::to(&range, content_size);

            if from == 0 && to == content_size {
                Cow::Borrowed(self)
            } else {
                Cow::Owned(self.copy(|c| c.cut(from..to)))
            }
        }
    }

    /// Cut out the part of the document between the given positions, and return it as a `Slice` object.
    fn slice<R: RangeBounds<usize> + Debug>(
        &self,
        range: R,
        include_parents: bool,
    ) -> Result<Slice<S>, SliceError> {
        let from = util::from(&range);
        let to = util::to(&range, self.node_size());

        if from == to {
            return Ok(Slice::default());
        }

        let rp_from = self.resolve(from)?;
        let rp_to = self.resolve(to)?;

        let depth = if include_parents {
            0
        } else {
            rp_from.shared_depth(to)
        };

        let (start, node) = (rp_from.start(depth), rp_from.node(depth));
        let content = if let Some(c) = node.content() {
            c.cut(rp_from.pos - start..rp_to.pos - start)
        } else {
            Fragment::new()
        };
        Ok(Slice::new(
            content,
            rp_from.depth - depth,
            rp_to.depth - depth,
        ))
    }

    /// Replace the part of the document between the given positions with the given slice. The
    /// slice must 'fit', meaning its open sides must be able to connect to the surrounding content,
    /// and its content nodes must be valid children for the node they are placed into. If any of
    /// this is violated, an error of type
    /// [`ReplaceError`](#model.ReplaceError) is thrown.
    fn replace<R: RangeBounds<usize> + Debug>(
        &self,
        range: R,
        slice: &Slice<S>,
    ) -> Result<Self, ReplaceError<S>> {
        let from = util::from(&range);
        let to = util::to(&range, self.node_size());
        // FIXME: this max value is my guess, that needs to be tested out

        assert!(to >= from, "replace: {} >= {}", to, from);

        let rp_from = self.resolve(from)?;
        let rp_to = self.resolve(to)?;

        let node = replace(&rp_from, &rp_to, slice)?;
        Ok(node)
    }

    /// Resolve the given position in the document, returning a struct with information about its
    /// context.
    fn resolve(&self, pos: usize) -> Result<ResolvedPos<'_, S>, ResolveErr> {
        ResolvedPos::resolve(self, pos)
    }

    /// Create a new node with the same markup as this node, containing the given content (or
    /// empty, if no content is given).
    fn copy<F>(&self, map: F) -> Self
    where
        F: FnOnce(&Fragment<S>) -> Fragment<S>;

    /// Concatenates all the text nodes found in this fragment and its children.
    fn text_content(&self) -> String {
        if let Some(node) = self.text_node() {
            node.text.content.clone()
        } else {
            let mut buf = String::new();
            if let Some(c) = self.content() {
                c.text_between(&mut buf, true, 0, c.size(), Some(""), None);
            }
            buf
        }
    }

    /// Returns this node's first child wrapped in `Some`, or `Node` if there are no children.
    fn first_child(&self) -> Option<&S::Node> {
        self.content().and_then(Fragment::first_child)
    }

    /// Represents `.content.size` in JS
    fn content_size(&self) -> usize {
        self.content().map(Fragment::size).unwrap_or(0)
    }

    /// Get the text and marks if this is a text node
    fn text_node(&self) -> Option<&TextNode<S>>;

    /// Create a new text node
    fn new_text_node(node: TextNode<S>) -> Self;

    /// Creates a new text node
    fn text<A: Into<String>>(text: A) -> Self;

    /// A container holding the node's children.
    fn content(&self) -> Option<&Fragment<S>>;

    /// Get the marks on this node
    fn marks(&self) -> Option<&MarkSet<S>>;

    /// Get the type of the node
    fn r#type(&self) -> S::NodeType;

    /// Get this node's attributes as a JSON value.
    /// Text nodes and nodes without attrs return `serde_json::Value::Null`.
    fn attrs_json(&self) -> serde_json::Value {
        serde_json::Value::Null
    }

    /// Create a new node with a single attribute set to the given value.
    /// Other attributes are preserved.
    fn with_attr(&self, _attr: &str, _value: serde_json::Value) -> Self {
        // Default: nodes without attributes just return self unchanged.
        self.copy(|c| c.clone())
    }

    /// Get the node at the given document position.
    /// Returns the node that covers the position (for non-text content positions,
    /// returns the child node; for text positions, returns the text node).
    fn node_at(&self, mut pos: usize) -> Option<&Self> {
        let mut node: &Self = self;
        loop {
            let content = node.content()?;
            let idx = content.find_index(pos, false).ok()?;
            node = content.maybe_child(idx.index)?;
            if idx.offset == pos || node.is_text() {
                return Some(node);
            }
            pos -= idx.offset + 1;
        }
    }

    /// True when this is a block (non-inline node)
    fn is_block(&self) -> bool;

    /// True when this is an inline node (a text node or a node that can appear among text).
    fn is_inline(&self) -> bool {
        self.r#type().is_inline()
    }

    /// True when this is a text node.
    fn is_text(&self) -> bool {
        self.text_node().is_some()
    }

    /// True when this is a leaf node.
    fn is_leaf(&self) -> bool {
        self.content().is_none()
    }

    /// Get the content match in this node at the given index.
    fn content_match_at(&self, index: usize) -> Result<S::ContentMatch, ContentMatchError> {
        self.r#type()
            .content_match()
            .match_fragment_range(self.content().unwrap_or(Fragment::EMPTY_REF), 0..index)
            .ok_or(ContentMatchError::InvalidContent)
    }

    /// Test whether a single node of the given type can replace the given range.
    fn can_replace_with(&self, from: usize, to: usize, node_type: S::NodeType) -> bool {
        self.content_match_at(from)
            .ok()
            .and_then(|m| m.match_type(node_type))
            .and_then(|m| {
                m.match_fragment_range(self.content().unwrap_or(Fragment::EMPTY_REF), to..)
            })
            .is_some_and(|m| m.valid_end())
    }

    /// Test whether replacing the range between `from` and `to` (by
    /// child index) with the given replacement fragment (which defaults
    /// to the empty fragment) would leave the node's content valid. You
    /// can optionally pass `start` and `end` indices into the
    /// replacement fragment.
    fn can_replace<R: RangeBounds<usize>>(
        &self,
        from: usize,
        to: usize,
        replacement: Option<&Fragment<S>>,
        range: R,
    ) -> Result<bool, ContentMatchError> {
        let replacement = replacement.unwrap_or(Fragment::EMPTY_REF);
        let start = util::from(&range);
        let end = util::to(&range, replacement.child_count());

        let one = self
            .content_match_at(from)?
            .match_fragment_range(replacement, start..end);
        let two = one.and_then(|o| {
            o.match_fragment_range(self.content().unwrap_or(Fragment::EMPTY_REF), to..)
        });

        if matches!(two, Some(m) if m.valid_end()) {
            for i in start..end {
                if replacement
                    .child(i)
                    .marks()
                    .filter(|m| !self.r#type().allow_marks(m))
                    .is_some()
                {
                    return Ok(false);
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the last child of this node, or None if it has no children.
    fn last_child(&self) -> Option<&S::Node> {
        self.content().and_then(Fragment::last_child)
    }

    /// Get the text content between two positions. When `block_separator` is
    /// given, it will be inserted whenever a new block node is started. When
    /// `leaf_text` is given, it'll be inserted for leaf nodes.
    fn text_between(
        &self,
        from: usize,
        to: usize,
        block_separator: Option<&str>,
        leaf_text: Option<&str>,
    ) -> String {
        if let Some(text_node) = self.text_node() {
            let len = text_node.text.len_utf16();
            let from = from.min(len);
            let to = to.min(len);

            if from >= to {
                return String::new();
            }

            let (_, rest) = util::split_at_utf16(text_node.text.as_str(), from);
            return util::split_at_utf16(rest, to - from).0.to_owned();
        }

        let mut result = String::new();
        if let Some(c) = self.content() {
            c.text_between(&mut result, true, from, to, block_separator, leaf_text);
        }
        result
    }

    /// Invoke a callback for all descendant nodes between `from` and `to`
    fn nodes_between<F: FnMut(&S::Node, usize) -> bool>(
        &self,
        from: usize,
        to: usize,
        f: &mut F,
        offset: usize,
    ) {
        if let Some(c) = self.content() {
            c.nodes_between(from, to, f, offset);
        }
    }

    /// Invoke a callback for all descendant nodes
    fn descendants<F: FnMut(&S::Node, usize) -> bool>(&self, f: &mut F) {
        let size = self.content_size();
        self.nodes_between(0, size, f, 0);
    }

    /// Test whether this node's content matches another node's content
    fn eq(&self, other: &S::Node) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        if self.r#type() != other.r#type()
            || self.attrs_json() != other.attrs_json()
            || self.marks() != other.marks()
        {
            return false;
        }
        match (self.text_node(), other.text_node()) {
            (Some(a), Some(b)) => a.text == b.text,
            (Some(_), None) | (None, Some(_)) => false,
            (None, None) => match (self.content(), other.content()) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                _ => false,
            },
        }
    }

    /// Test whether this node has the same markup (type, attrs, marks) as another
    fn same_markup(&self, other: &S::Node) -> bool {
        self.r#type() == other.r#type()
            && self.attrs_json() == other.attrs_json()
            && self.marks() == other.marks()
    }

    /// Test whether a range of this node has the given mark type
    fn range_has_mark(&self, from: usize, to: usize, mark_type: S::MarkType) -> bool {
        let mut found = false;
        self.nodes_between(
            from,
            to,
            &mut |node, _pos| {
                if let Some(marks) = node.marks() {
                    for m in marks {
                        if m.r#type() == mark_type {
                            found = true;
                            return false;
                        }
                    }
                }
                true
            },
            0,
        );
        found
    }

    /// Test whether another node's content could be appended to this node
    fn can_append(&self, other: &S::Node) -> bool {
        if other.child_count() == 0 {
            return true;
        }
        if let (Some(mc), Some(oc)) = (self.content(), other.content()) {
            self.r#type()
                .content_match()
                .match_fragment_range(mc, ..)
                .and_then(|m| m.match_fragment_range(oc, ..))
                .is_some_and(|m| m.valid_end())
        } else {
            false
        }
    }

    /// Validate the content of this node against the schema. Returns Ok(()) or an error.
    fn check(&self) -> Result<(), String> {
        if let Some(c) = self.content() {
            if !self.r#type().valid_content(c) {
                return Err(format!("Invalid content for node {}", self.r#type().name()));
            }
            for child in c.children() {
                child.check()?;
            }
        }
        if let Some(marks) = self.marks() {
            if !self.r#type().allow_marks(marks) {
                return Err(format!("Invalid marks for node {}", self.r#type().name()));
            }
        }
        Ok(())
    }

    /// Iterate over the node's children, calling `f` with each child node,
    /// its offset, and its index.
    fn for_each<F: FnMut(&S::Node, usize, usize)>(&self, f: &mut F) {
        if let Some(c) = self.content() {
            c.for_each(f);
        }
    }

    /// True when this is an atom node (a leaf that should not be edited directly)
    fn is_atom(&self) -> bool {
        self.r#type().is_atom()
    }

    /// True when this is a textblock (a block node with inline content)
    fn is_textblock(&self) -> bool {
        self.r#type().is_textblock()
    }

    /// Whether this node has inline content
    fn inline_content(&self) -> bool {
        self.r#type().inline_content()
    }
}

/// A string that stores its length in utf-16
#[derive(Default, Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub struct Text {
    len_utf16: usize,
    content: String,
}

impl Text {
    /// Return the contained string
    pub fn as_str(&self) -> &str {
        &self.content
    }

    /// The length of this string if it were encoded in utf-16
    pub fn len_utf16(&self) -> usize {
        self.len_utf16
    }

    /// Join two texts together
    pub fn join(&self, other: &Self) -> Self {
        let left = &self.content;
        let right = &other.content;
        let mut content = String::with_capacity(left.len() + right.len());
        content.push_str(left);
        content.push_str(right);
        let len_utf16 = self.len_utf16 + other.len_utf16;
        Text { len_utf16, content }
    }
}

impl From<String> for Text {
    fn from(src: String) -> Text {
        Text {
            len_utf16: src.encode_utf16().count(),
            content: src,
        }
    }
}

impl Serialize for Text {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.content.serialize(serializer)
    }
}
