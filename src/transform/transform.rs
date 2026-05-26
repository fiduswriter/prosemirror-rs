//! The `Transform` class — the primary public API for constructing document transformations.

use super::map::{Mappable, Mapping};
use super::mark_step::{AddMarkStep, RemoveMarkStep};
use super::node_mark_step::{AddNodeMarkStep, RemoveNodeMarkStep};
use super::replace_step::{ReplaceAroundStep, ReplaceStep};
use super::structure::NodeRange;
use super::Step;
use crate::model::{ContentMatch, Fragment, Mark, MarkSet, Node, NodeType, Schema, Slice};
use derivative::Derivative;

/// A Transform is a collection of steps that can be applied to a document.
///
/// It maintains the current document state, accumulated steps, document history,
/// and a combined mapping.
#[derive(Derivative)]
#[derivative(Debug(bound = ""))]
pub struct Transform<S: Schema> {
    /// The current document
    pub doc: S::Node,
    /// Accumulated steps
    pub steps: Vec<Step<S>>,
    /// Documents before each step
    pub docs: Vec<S::Node>,
    /// Combined mapping
    pub mapping: Mapping,
}

impl<S: Schema> Transform<S> {
    /// Create a new Transform starting from the given document.
    pub fn new(doc: S::Node) -> Self {
        Transform {
            doc,
            steps: Vec::new(),
            docs: Vec::new(),
            mapping: Mapping::new(),
        }
    }

    /// The document before all steps were applied.
    pub fn before(&self) -> &S::Node {
        self.docs.first().unwrap_or(&self.doc)
    }

    /// Whether any steps have been applied.
    pub fn doc_changed(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Apply a step, raising on failure.
    pub fn step(&mut self, step: Step<S>) -> Result<&mut Self, StepError> {
        let result = self.maybe_step(step);
        if let Some(msg) = result {
            return Err(StepError::ApplyFailed(msg));
        }
        Ok(self)
    }

    /// Apply a step, returning an error message on failure.
    pub fn maybe_step(&mut self, step: Step<S>) -> Option<String> {
        match step.apply(&self.doc) {
            Ok(new_doc) => {
                self.add_step(step, new_doc);
                None
            }
            Err(e) => Some(format!("{:?}", e)),
        }
    }

    fn add_step(&mut self, step: Step<S>, doc: S::Node) {
        self.docs.push(std::mem::replace(&mut self.doc, doc));
        self.mapping.append_map(step.get_map(), None);
        self.steps.push(step);
    }

    /// Return the total changed range, or None if no changes.
    pub fn changed_range(&self) -> Option<(usize, usize)> {
        if self.mapping.maps.is_empty() {
            return None;
        }
        let mut from: usize = 1_000_000_000;
        let mut to: isize = -1_000_000_000;
        for (i, map) in self.mapping.maps.iter().enumerate() {
            if i > 0 {
                from = map.map(from, 1);
                to = map.map(to as usize, -1) as isize;
            }
            map.for_each(|_old_from, _old_to, new_from, new_to| {
                from = usize::min(from, new_from);
                to = isize::max(to, new_to as isize);
            });
        }
        if from == 1_000_000_000 {
            None
        } else {
            Some((from, to as usize))
        }
    }

    /// Add a mark to the inline content in the given range.
    pub fn add_mark(&mut self, from: usize, to: usize, mark: S::Mark) -> &mut Self {
        let mut removed = Vec::new();
        let mut added = Vec::new();

        if let Some(content) = self.doc.content() {
            content.nodes_between(
                from,
                to,
                &mut |node, pos| {
                    if !node.is_inline() {
                        return true;
                    }
                    let marks = node.marks();
                    let node_marks = marks.map(Cow::Borrowed).unwrap_or_default();

                    if !mark.is_in_set(&node_marks) {
                        let start = usize::max(pos, from);
                        let end = usize::min(pos + node.node_size(), to);
                        let new_set = mark.add_to_set(node_marks);

                        if let Some(marks) = marks {
                            for m in marks {
                                if !m.is_in_set(&new_set) {
                                    removed.push(Step::RemoveMark(RemoveMarkStep {
                                        span: crate::transform::Span { from: start, to: end },
                                        mark: m.clone(),
                                    }));
                                }
                            }
                        }

                        added.push(Step::AddMark(AddMarkStep {
                            span: crate::transform::Span { from: start, to: end },
                            mark: mark.clone(),
                        }));
                    }
                    true
                },
                0,
            );
        }

        for item in removed {
            let _ = self.maybe_step(item);
        }
        for item in added {
            let _ = self.maybe_step(item);
        }
        self
    }

    /// Remove mark(s) from the inline content in the given range.
    pub fn remove_mark(
        &mut self,
        from: usize,
        to: usize,
        mark: Option<S::Mark>,
    ) -> &mut Self {
        // Simplified: remove all matching marks
        if let Some(mark) = mark {
            let step = Step::RemoveMark(RemoveMarkStep {
                span: crate::transform::Span { from, to },
                mark,
            });
            let _ = self.maybe_step(step);
        }
        self
    }

    /// Low-level replace.
    pub fn replace(
        &mut self,
        from: usize,
        to: Option<usize>,
        slice: Option<Slice<S>>,
    ) -> &mut Self {
        let to = to.unwrap_or(from);
        let slice = slice.unwrap_or_default();
        if from == to && slice.size() == 0 {
            return self;
        }
        // Use the smart Fitter so cross-depth replacements (e.g. inserting a block
        // node into inline content) are handled correctly, matching the JS behaviour.
        if let Some(step) = crate::transform::replace::replace_step(&self.doc, from, to, &slice) {
            let _ = self.maybe_step(step);
        }
        self
    }

    /// Replace range with specific content.
    pub fn replace_with(
        &mut self,
        from: usize,
        to: usize,
        content: Fragment<S>,
    ) -> &mut Self {
        self.replace(from, Some(to), Some(Slice::new(content, 0, 0)))
    }

    /// Delete the content between positions `from` and `to`.
    ///
    /// This is a convenience wrapper around [`Transform::replace`] with an empty slice.
    /// Positions follow ProseMirror's token-stream model: the opening tag of the first
    /// child is at position 0, the first character of its text content is at position 1, etc.
    ///
    /// # Example
    ///
    /// ```
    /// use prosemirror::dynamic::{DynamicNode, DynamicSchema};
    /// use prosemirror::dynamic::types::Dyn;
    /// use prosemirror::model::Node;
    /// use prosemirror::transform::Transform;
    ///
    /// let schema = DynamicSchema::from_json(&serde_json::json!({
    ///     "nodes": {
    ///         "doc":       { "content": "paragraph+" },
    ///         "paragraph": { "content": "text*", "group": "block" },
    ///         "text":      { "group": "inline" }
    ///     },
    ///     "marks": {}
    /// })).unwrap();
    ///
    /// // Document: doc[ paragraph[ "hello world" ] ]
    /// // Positions: 0=before-p  1='h' 2='e' ... 6=' ' 7='w' ... 11='d'  12=after-p
    /// let after = schema.with_types(|| {
    ///     let doc: DynamicNode = schema.node_from_json(&serde_json::json!({
    ///         "type": "doc",
    ///         "content": [{"type": "paragraph",
    ///                       "content": [{"type": "text", "text": "hello world"}]}]
    ///     })).unwrap();
    ///     let mut tr: Transform<Dyn> = Transform::new(doc);
    ///     tr.delete(7, 12); // remove "world" (positions 7 through 12)
    ///     tr.doc.text_content()
    /// });
    ///
    /// assert_eq!(after, "hello ");
    /// ```
    pub fn delete(&mut self, from: usize, to: usize) -> &mut Self {
        self.replace(from, Some(to), None)
    }

    /// Insert content at a position.
    pub fn insert(&mut self, pos: usize, content: Fragment<S>) -> &mut Self {
        self.replace_with(pos, pos, content)
    }

    /// Add a node mark step.
    pub fn add_node_mark(&mut self, pos: usize, mark: S::Mark) -> &mut Self {
        let _ = self.maybe_step(Step::AddNodeMark(AddNodeMarkStep { pos, mark }));
        self
    }

    /// Remove a node mark step.
    pub fn remove_node_mark(&mut self, pos: usize, mark: S::Mark) -> &mut Self {
        let _ = self.maybe_step(Step::RemoveNodeMark(RemoveNodeMarkStep { pos, mark }));
        self
    }

    /// Set a node attribute.
    pub fn set_node_attribute(
        &mut self,
        pos: usize,
        attr: &str,
        value: serde_json::Value,
    ) -> &mut Self {
        let _ = self.maybe_step(Step::Attr(super::AttrStep {
            pos,
            attr: attr.to_string(),
            value,
        }));
        self
    }

    /// Set a document attribute.
    pub fn set_doc_attribute(
        &mut self,
        attr: &str,
        value: serde_json::Value,
    ) -> &mut Self {
        let _ = self.maybe_step(Step::DocAttr(super::DocAttrStep {
            attr: attr.to_string(),
            value,
        }));
        self
    }

    /// Lift content to the given target depth.
    pub fn lift(&mut self, range: &NodeRange<S>, target: usize) -> &mut Self {
        let from = &range.from;
        let to = &range.to;
        let depth = range.depth;

        let gap_start = from.before(depth + 1).unwrap_or(0);
        let gap_end = to.after(depth + 1).unwrap_or(0);
        let mut start = gap_start;
        let mut end = gap_end;

        let mut before = Fragment::new();
        let mut open_start = 0;
        let mut d = depth;
        let mut splitting = false;
        while d > target {
            if splitting || from.index(d) > 0 {
                splitting = true;
                before = Fragment::from(vec![from.node(d).copy(|_| before)]);
                open_start += 1;
            } else {
                start -= 1;
            }
            d -= 1;
        }
        let mut after = Fragment::new();
        let mut open_end = 0;
        d = depth;
        splitting = false;
        while d > target {
            let after_pos = to.after(d + 1).unwrap_or(0);
            let end_d = to.end(d);
            if splitting || after_pos < end_d {
                splitting = true;
                after = Fragment::from(vec![to.node(d).copy(|_| after)]);
                open_end += 1;
            } else {
                end += 1;
            }
            d -= 1;
        }
        let combined = before.append(after);
        let insert_offset = combined.size() - open_start;
        let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
            span: crate::transform::Span { from: start, to: end },
            gap_from: gap_start,
            gap_to: gap_end,
            slice: Slice::new(combined, open_start, open_end),
            insert: insert_offset,
            structure: true,
        }));
        self
    }

    /// Wrap content in node(s).
    pub fn wrap(&mut self, range: &NodeRange<S>, wrappers: &[(S::NodeType, Option<MarkSet<S>>)]) -> &mut Self {
        let mut content = Fragment::new();
        for i in (0..wrappers.len()).rev() {
            if content.size() > 0 {
                match wrappers[i].0.content_match().match_fragment(&content) {
                    Some(match_) if match_.valid_end() => {}
                    _ => return self,
                }
            }
            content = Fragment::from(vec![wrappers[i].0.create_node(Some(&content), wrappers[i].1.as_ref())]);
        }
        let start = range.start();
        let end = range.end();
        let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
            span: crate::transform::Span { from: start, to: end },
            gap_from: start,
            gap_to: end,
            slice: Slice::new(content, 0, 0),
            insert: wrappers.len(),
            structure: true,
        }));
        self
    }

    /// Split the node at the given position, producing two sibling nodes.
    ///
    /// `depth` controls how many ancestor levels are split (defaults to 1, i.e. just the
    /// immediate parent). `types_after` can override the node types of the newly created
    /// right-hand siblings.
    ///
    /// # Example
    ///
    /// ```
    /// use prosemirror::dynamic::{DynamicNode, DynamicSchema};
    /// use prosemirror::dynamic::types::Dyn;
    /// use prosemirror::model::Node;
    /// use prosemirror::transform::Transform;
    ///
    /// let schema = DynamicSchema::from_json(&serde_json::json!({
    ///     "nodes": {
    ///         "doc":       { "content": "paragraph+" },
    ///         "paragraph": { "content": "text*", "group": "block" },
    ///         "text":      { "group": "inline" }
    ///     },
    ///     "marks": {}
    /// })).unwrap();
    ///
    /// // "foobar" in a paragraph — position 4 falls between 'o' and 'b'
    /// let (first, second) = schema.with_types(|| {
    ///     let doc: DynamicNode = schema.node_from_json(&serde_json::json!({
    ///         "type": "doc",
    ///         "content": [{"type": "paragraph",
    ///                       "content": [{"type": "text", "text": "foobar"}]}]
    ///     })).unwrap();
    ///     let mut tr: Transform<Dyn> = Transform::new(doc);
    ///     tr.split(4, None, None); // split after "foo"
    ///     let a = tr.doc.child(0).unwrap().text_content();
    ///     let b = tr.doc.child(1).unwrap().text_content();
    ///     (a, b)
    /// });
    ///
    /// assert_eq!(first,  "foo");
    /// assert_eq!(second, "bar");
    /// ```
    pub fn split(
        &mut self,
        pos: usize,
        depth: Option<usize>,
        types_after: Option<&[S::NodeType]>,
    ) -> &mut Self {
        let depth = depth.unwrap_or(1);
        let pos_ = match self.doc.resolve(pos) {
            Ok(p) => p,
            Err(_) => return self,
        };
        let mut before = Fragment::new();
        let mut after = Fragment::new();
        let mut d = pos_.depth;
        let e = pos_.depth - depth;
        let mut i = depth as isize - 1;
        while d > e {
            before = Fragment::from(vec![pos_.node(d).copy(|_| before)]);
            let type_after = types_after.and_then(|t| {
                if i >= 0 && (i as usize) < t.len() {
                    Some(t[i as usize])
                } else {
                    None
                }
            });
            after = Fragment::from(vec![match type_after {
                Some(t) => t.create_node(Some(&after), None),
                None => pos_.node(d).copy(|_| after),
            }]);
            d -= 1;
            i -= 1;
        }
        let combined = before.append(after);
        let _ = self.maybe_step(Step::Replace(ReplaceStep {
            span: crate::transform::Span { from: pos, to: pos },
            slice: Slice::new(combined, depth, depth),
            structure: true,
        }));
        self
    }

    /// Join nodes at the given position.
    pub fn join(&mut self, pos: usize, depth: Option<usize>) -> &mut Self {
        let depth = depth.unwrap_or(1);
        let _ = self.maybe_step(Step::Replace(ReplaceStep {
            span: crate::transform::Span {
                from: pos - depth,
                to: pos + depth,
            },
            slice: Slice::default(),
            structure: true,
        }));
        self
    }

    /// Change the type of textblocks in the given range.
    pub fn set_block_type(
        &mut self,
        from: usize,
        to: usize,
        node_type: S::NodeType,
    ) -> &mut Self {
        let map_from = self.steps.len();
        // Walk through nodes and change block types
        if let Some(content) = self.doc.content() {
            let mut positions = Vec::new();
            content.nodes_between(
                from,
                to,
                &mut |node, pos| {
                    if node.is_block() && !node.is_text() && !node.is_leaf() {
                        // This is a textblock candidate
                        let mapped_pos = self.mapping.slice(map_from, None).map(pos, 1);
                        positions.push((node.r#type(), mapped_pos, node.node_size()));
                    }
                    true
                },
                0,
            );
            for (_, pos, size) in positions {
                let mapped_end = self.mapping.slice(map_from, None).map(pos + size, 1);
                let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
                    span: crate::transform::Span {
                        from: pos + 1,
                        to: mapped_end - 1,
                    },
                    gap_from: pos + 1,
                    gap_to: mapped_end - 1,
                    slice: Slice::new(Fragment::from(vec![node_type.create_node(None, None)]), 0, 0),
                    insert: 1,
                    structure: true,
                }));
            }
        }
        self
    }

    /// Change the markup of a node at the given position.
    pub fn set_node_markup(
        &mut self,
        pos: usize,
        node_type: Option<S::NodeType>,
        marks: Option<MarkSet<S>>,
    ) -> &mut Self {
        let node = match self.doc.resolve(pos) {
            Ok(rp) => rp.node_after().map(|c| c.into_owned()),
            Err(_) => return self,
        };
        if let Some(node) = node {
            let type_ = node_type.unwrap_or_else(|| node.r#type());
            let new_node = type_.create_node(None, marks.as_ref());
            if node.is_leaf() {
                return self.replace_with(pos, pos + node.node_size(), Fragment::from(vec![new_node]));
            }
            let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
                span: crate::transform::Span {
                    from: pos,
                    to: pos + node.node_size(),
                },
                gap_from: pos + 1,
                gap_to: pos + node.node_size() - 1,
                slice: Slice::new(Fragment::from(vec![new_node]), 0, 0),
                insert: 1,
                structure: true,
            }));
        }
        self
    }
}

/// Error type for transform operations
#[derive(Debug)]
pub enum StepError {
    /// Step application failed
    ApplyFailed(String),
}

use std::borrow::Cow;
