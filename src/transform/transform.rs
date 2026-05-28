//! The `Transform` class — the primary public API for constructing document transformations.

use super::map::{Mappable, Mapping};
use super::mark_step::{AddMarkStep, RemoveMarkStep};
use super::node_mark_step::{AddNodeMarkStep, RemoveNodeMarkStep};
use super::replace::{close_fragment, covered_depths};
use super::replace_step::{ReplaceAroundStep, ReplaceStep};
use super::structure::{insert_point, NodeRange};
use super::Step;
use crate::model::{ContentMatch, Fragment, Mark, MarkSet, Node, NodeType, Schema, Slice};
use derivative::Derivative;

/// Check whether a node type is defining for content (used by replaceRange).
fn defines_content<S: Schema>(node_type: S::NodeType) -> bool {
    node_type.is_defining() || node_type.is_defining_for_content()
}

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
                                        span: crate::transform::Span {
                                            from: start,
                                            to: end,
                                        },
                                        mark: m.clone(),
                                    }));
                                }
                            }
                        }

                        added.push(Step::AddMark(AddMarkStep {
                            span: crate::transform::Span {
                                from: start,
                                to: end,
                            },
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
    pub fn remove_mark(&mut self, from: usize, to: usize, mark: Option<S::Mark>) -> &mut Self {
        let mut matched: Vec<(S::Mark, usize, usize, usize)> = Vec::new();
        let mut step = 0usize;
        self.doc.nodes_between(
            from,
            to,
            &mut |node, pos| {
                if !node.is_inline() {
                    return true;
                }
                step += 1;
                let node_marks = node.marks().cloned().unwrap_or_default();
                let to_remove: Vec<S::Mark> = match &mark {
                    None => node_marks.iter().cloned().collect(),
                    Some(mark) => {
                        if mark.is_in_set(&node_marks) {
                            vec![mark.clone()]
                        } else {
                            vec![]
                        }
                    }
                };
                if !to_remove.is_empty() {
                    let end = (pos + node.node_size()).min(to);
                    for style in to_remove {
                        if let Some(found) =
                            matched.iter_mut().find(|m| m.3 == step - 1 && m.0 == style)
                        {
                            found.2 = end;
                            found.3 = step;
                        } else {
                            matched.push((style, from.max(pos), end, step));
                        }
                    }
                }
                false
            },
            0,
        );
        for item in matched {
            let _ = self.maybe_step(Step::RemoveMark(RemoveMarkStep {
                span: crate::transform::Span {
                    from: item.1,
                    to: item.2,
                },
                mark: item.0,
            }));
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
    pub fn replace_with(&mut self, from: usize, to: usize, content: Fragment<S>) -> &mut Self {
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

    /// Replace a range with a slice, using depth-based heuristics to try to
    /// make the change fit the document structure.
    pub fn replace_range(&mut self, from: usize, to: usize, slice: Slice<S>) -> &mut Self {
        if slice.size() == 0 {
            return self.delete_range(from, to);
        }

        let doc = self.doc.clone();
        let from_rp = match doc.resolve(from) {
            Ok(rp) => rp,
            Err(_) => return self,
        };
        let to_rp = match doc.resolve(to) {
            Ok(rp) => rp,
            Err(_) => return self,
        };

        // Trivial case: simple replace fits directly
        if slice.open_start == 0
            && slice.open_end == 0
            && from_rp.start(from_rp.depth) == to_rp.start(to_rp.depth)
        {
            if let Ok(can) = from_rp.parent().can_replace(
                from_rp.index(from_rp.depth),
                to_rp.index(to_rp.depth),
                Some(&slice.content),
                ..,
            ) {
                if can {
                    let _ = self.maybe_step(Step::Replace(ReplaceStep {
                        span: crate::transform::Span { from, to },
                        slice,
                        structure: false,
                    }));
                    return self;
                }
            }
        }

        let mut target_depths: Vec<isize> = covered_depths(&from_rp, &to_rp)
            .into_iter()
            .map(|d| d as isize)
            .collect();
        // Can't replace the whole document, so remove 0 if it's present
        if target_depths.last() == Some(&0) {
            target_depths.pop();
        }

        let mut preferred_target = -(from_rp.depth as isize + 1);
        target_depths.insert(0, preferred_target);

        let mut pos = from_rp.pos.saturating_sub(1);
        for d in (1..=from_rp.depth).rev() {
            let node_type = from_rp.node(d).r#type();
            if node_type.is_defining()
                || node_type.is_defining_as_context()
                || node_type.is_isolating()
            {
                break;
            }
            if target_depths.contains(&(d as isize)) {
                preferred_target = d as isize;
            } else if from_rp.before(d) == Some(pos) {
                target_depths.insert(1, -(d as isize));
            }
            pos = pos.saturating_sub(1);
        }

        let preferred_target_index = target_depths
            .iter()
            .position(|&d| d == preferred_target)
            .unwrap_or(0);

        // Collect left edge nodes from the slice
        let mut left_nodes: Vec<S::Node> = Vec::new();
        let mut current_content = &slice.content;
        for i in 0..=slice.open_start {
            if let Some(node) = current_content.first_child() {
                left_nodes.push(node.clone());
                if i < slice.open_start {
                    current_content = node.content().unwrap_or(Fragment::EMPTY_REF);
                }
            } else {
                break;
            }
        }

        let mut preferred_depth = slice.open_start;
        for d in (0..preferred_depth).rev() {
            let left_node = &left_nodes[d];
            let def = defines_content::<S>(left_node.r#type());
            let abs_preferred = preferred_target.unsigned_abs();
            let compare_node = from_rp.node(abs_preferred.saturating_sub(1));
            if def && !left_node.same_markup(compare_node) {
                preferred_depth = d;
            } else if def || !left_node.r#type().is_textblock() {
                break;
            }
        }

        for j in (0..=slice.open_start).rev() {
            let open_depth = (j + preferred_depth + 1) % (slice.open_start + 1);
            let insert = match left_nodes.get(open_depth) {
                Some(n) => n,
                None => continue,
            };
            for i in 0..target_depths.len() {
                let idx = (i + preferred_target_index) % target_depths.len();
                let mut target_depth = target_depths[idx];
                let expand = target_depth >= 0;
                if !expand {
                    target_depth = -target_depth;
                }
                let target_depth = target_depth as usize;
                let parent = from_rp.node(target_depth.saturating_sub(1));
                let index = from_rp.index(target_depth.saturating_sub(1));
                let marks_ok = parent
                    .r#type()
                    .allow_marks(insert.marks().unwrap_or(&MarkSet::new()));
                if parent.can_replace_with(index, index, insert.r#type()) && marks_ok {
                    let from_pos = from_rp.before(target_depth).unwrap_or(from);
                    let to_pos = if expand {
                        to_rp.after(target_depth).unwrap_or(to)
                    } else {
                        to
                    };
                    let closed =
                        close_fragment(&slice.content, 0, slice.open_start, open_depth, None);
                    self.replace(
                        from_pos,
                        Some(to_pos),
                        Some(Slice::new(closed, open_depth, slice.open_end)),
                    );
                    return self;
                }
            }
        }

        // Fallback: try expanding the range
        let start_steps = self.steps.len();
        let mut from = from;
        let mut to = to;
        for i in (0..target_depths.len()).rev() {
            self.replace(from, Some(to), Some(slice.clone()));
            if self.steps.len() > start_steps {
                break;
            }
            let depth = target_depths[i];
            if depth < 0 {
                continue;
            }
            let depth = depth as usize;
            from = from_rp.before(depth).unwrap_or(from);
            to = to_rp.after(depth).unwrap_or(to);
        }

        self
    }

    /// Replace a range with a single node.
    pub fn replace_range_with(&mut self, from: usize, to: usize, node: S::Node) -> &mut Self {
        if !node.is_inline() && from == to {
            if let Ok(resolved_pos) = self.doc.resolve(from) {
                if resolved_pos
                    .parent()
                    .content()
                    .map(|c| c.size())
                    .unwrap_or(0)
                    > 0
                {
                    if let Some(point) = insert_point::<S>(&self.doc, from, node.r#type()) {
                        let from = point;
                        let to = point;
                        return self.replace_range(
                            from,
                            to,
                            Slice::new(Fragment::from(vec![node]), 0, 0),
                        );
                    }
                }
            }
        }
        self.replace_range(from, to, Slice::new(Fragment::from(vec![node]), 0, 0))
    }

    /// Delete a range, expanding to cover full nodes when possible.
    pub fn delete_range(&mut self, from: usize, to: usize) -> &mut Self {
        let doc = self.doc.clone();
        let mut from_rp = match doc.resolve(from) {
            Ok(rp) => rp,
            Err(_) => return self,
        };
        let mut to_rp = match doc.resolve(to) {
            Ok(rp) => rp,
            Err(_) => return self,
        };

        // When the deleted range spans from the start of one textblock to
        // the start of another one, move out of the start of both blocks.
        if from_rp.parent().is_textblock()
            && to_rp.parent().is_textblock()
            && from_rp.start(from_rp.depth) != to_rp.start(to_rp.depth)
            && from_rp.parent_offset == 0
            && to_rp.parent_offset == 0
        {
            let shared = from_rp.shared_depth(to);
            let mut isolated = false;
            for d in (shared + 1..=from_rp.depth).rev() {
                if from_rp.node(d).r#type().is_isolating() {
                    isolated = true;
                }
            }
            for d in (shared + 1..=to_rp.depth).rev() {
                if to_rp.node(d).r#type().is_isolating() {
                    isolated = true;
                }
            }
            if !isolated {
                let mut from = from;
                let mut to = to;
                for d in (1..=from_rp.depth).rev() {
                    if from == from_rp.start(d) {
                        if let Some(before) = from_rp.before(d) {
                            from = before;
                        }
                    }
                }
                for d in (1..=to_rp.depth).rev() {
                    if to == to_rp.start(d) {
                        if let Some(before) = to_rp.before(d) {
                            to = before;
                        }
                    }
                }
                from_rp = match self.doc.resolve(from) {
                    Ok(rp) => rp,
                    Err(_) => return self,
                };
                to_rp = match self.doc.resolve(to) {
                    Ok(rp) => rp,
                    Err(_) => return self,
                };
            }
        }

        let covered = covered_depths(&from_rp, &to_rp);
        for (i, &depth) in covered.iter().enumerate() {
            let last = i == covered.len() - 1;
            if (last && depth == 0) || from_rp.node(depth).r#type().content_match().valid_end() {
                self.delete(from_rp.start(depth), to_rp.end(depth));
                return self;
            }
            if depth > 0 {
                let can_replace = from_rp.node(depth - 1).can_replace(
                    from_rp.index(depth - 1),
                    to_rp.index_after(depth - 1),
                    None,
                    ..,
                );
                if last || can_replace.unwrap_or(false) {
                    self.delete(
                        from_rp.before(depth).unwrap_or(from),
                        to_rp.after(depth).unwrap_or(to),
                    );
                    return self;
                }
            }
        }
        for d in 1..=usize::min(from_rp.depth, to_rp.depth) {
            if from - from_rp.start(d) == from_rp.depth - d
                && to > from_rp.end(d)
                && to_rp.end(d) - to != to_rp.depth - d
                && from_rp.start(d - 1) == to_rp.start(d - 1)
            {
                let can_replace = from_rp.node(d - 1).can_replace(
                    from_rp.index(d - 1),
                    to_rp.index(d - 1),
                    None,
                    ..,
                );
                if can_replace.unwrap_or(false) {
                    self.delete(from_rp.before(d).unwrap_or(from), to);
                    return self;
                }
            }
        }
        self.delete(from, to)
    }

    /// Insert content at a position.
    pub fn insert(&mut self, pos: usize, content: Fragment<S>) -> &mut Self {
        self.replace_with(pos, pos, content)
    }

    /// Add a node mark step.
    pub fn add_node_mark(&mut self, pos: usize, mark: S::Mark) -> &mut Self {
        if let Some(node) = self.doc.node_at(pos) {
            if let Some(marks) = node.marks() {
                if marks.contains(&mark) {
                    return self;
                }
            }
        }
        let _ = self.maybe_step(Step::AddNodeMark(AddNodeMarkStep { pos, mark }));
        self
    }

    /// Remove a node mark step.
    pub fn remove_node_mark(&mut self, pos: usize, mark: S::Mark) -> &mut Self {
        if let Some(node) = self.doc.node_at(pos) {
            if let Some(marks) = node.marks() {
                if !marks.contains(&mark) {
                    return self;
                }
            } else {
                return self;
            }
        } else {
            return self;
        }
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
    pub fn set_doc_attribute(&mut self, attr: &str, value: serde_json::Value) -> &mut Self {
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
        let before_size = before.size();
        let combined = before.append(after);
        let insert_offset = before_size - open_start;
        let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
            span: crate::transform::Span {
                from: start,
                to: end,
            },
            gap_from: gap_start,
            gap_to: gap_end,
            slice: Slice::new(combined, open_start, open_end),
            insert: insert_offset,
            structure: true,
        }));
        self
    }

    /// Wrap content in node(s).
    pub fn wrap(
        &mut self,
        range: &NodeRange<S>,
        wrappers: &[super::structure::Wrapper<S>],
    ) -> &mut Self {
        let mut content = Fragment::new();
        for i in (0..wrappers.len()).rev() {
            if content.size() > 0 {
                match wrappers[i]
                    .node_type
                    .content_match()
                    .match_fragment(&content)
                {
                    Some(match_) if match_.valid_end() => {}
                    _ => return self,
                }
            }
            content = Fragment::from(vec![wrappers[i].node_type.create(
                wrappers[i].attrs.clone(),
                Some(&content),
                None,
            )]);
        }
        let start = range.start();
        let end = range.end();
        let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
            span: crate::transform::Span {
                from: start,
                to: end,
            },
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
    ///     tr.split(4, None, None).unwrap(); // split after "foo"
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
    ) -> Result<&mut Self, StepError> {
        let depth = depth.unwrap_or(1);
        let pos_ = self
            .doc
            .resolve(pos)
            .map_err(|e| StepError::ApplyFailed(format!("{e:?}")))?;
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
        self.step(Step::Replace(ReplaceStep {
            span: crate::transform::Span { from: pos, to: pos },
            slice: Slice::new(combined, depth, depth),
            structure: true,
        }))?;
        Ok(self)
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

    /// Remove content that is not valid in the given parent type.
    fn clear_incompatible(
        &mut self,
        pos: usize,
        parent_type: S::NodeType,
        mut match_: Option<S::ContentMatch>,
        clear_newlines: bool,
    ) {
        let match_ = match_.get_or_insert_with(|| parent_type.content_match());
        let child_count = match self.doc.node_at(pos) {
            Some(n) => n.child_count(),
            None => return,
        };
        let mut repl_spans = Vec::new();
        let mut remove_marks = Vec::new();
        let mut cur = pos + 1;
        for i in 0..child_count {
            let (end, allowed, marks, text_info) = {
                let node = self.doc.node_at(pos).unwrap();
                let child = match node.child(i) {
                    Some(c) => c,
                    None => continue,
                };
                let end = cur + child.node_size();
                let allowed = match_.match_type(child.r#type());
                let marks = child.marks().cloned();
                let text_info = child.text_node().map(|t| t.text.as_str().to_string());
                (end, allowed, marks, text_info)
            };
            if let Some(allowed) = allowed {
                *match_ = allowed;
                if let Some(ref marks) = marks {
                    for mark in marks.iter() {
                        if !parent_type.allows_mark_type(mark.r#type()) {
                            remove_marks.push((cur, end, mark.clone()));
                        }
                    }
                }
                if clear_newlines {
                    if let Some(text_str) = text_info {
                        if parent_type.whitespace().as_deref() != Some("pre") {
                            let mut offset = 0isize;
                            for m in text_str.match_indices(['\n', '\r']) {
                                let start = ((cur + m.0) as isize + offset) as usize;
                                let newline_len = m.1.len();
                                repl_spans.push((start, start + newline_len, false));
                                offset += 1 - newline_len as isize;
                            }
                        }
                    }
                }
            } else {
                repl_spans.push((cur, end, true));
            }
            cur = end;
        }
        for (from, to, mark) in remove_marks {
            self.maybe_step(Step::RemoveMark(RemoveMarkStep {
                span: crate::transform::Span { from, to },
                mark,
            }));
        }
        if !match_.valid_end() {
            if let Some(fill) = match_.fill_before(&Fragment::new(), true, 0) {
                self.replace(cur, Some(cur), Some(Slice::new(fill, 0, 0)));
            }
        }
        for (from, to, is_delete) in repl_spans.into_iter().rev() {
            let slice = if is_delete {
                Slice::default()
            } else {
                let space_node = S::Node::text(" ");
                Slice::new(Fragment::from(vec![space_node]), 0, 0)
            };
            self.maybe_step(Step::Replace(ReplaceStep {
                span: crate::transform::Span { from, to },
                slice,
                structure: false,
            }));
        }
    }

    /// Change the type of textblocks in the given range.
    pub fn set_block_type(
        &mut self,
        from: usize,
        to: usize,
        node_type: S::NodeType,
        attrs: Option<serde_json::Value>,
    ) -> &mut Self {
        let map_from = self.steps.len();
        if let Some(content) = self.doc.content() {
            let mut positions = Vec::new();
            content.nodes_between(
                from,
                to,
                &mut |node, pos| {
                    if node.is_textblock() {
                        positions.push((pos, node.node_size()));
                        return false;
                    }
                    true
                },
                0,
            );
            for (pos, size) in positions {
                let mapped_pos = self.mapping.slice(map_from, None).map(pos, 1);
                if !super::structure::can_change_type::<S>(&self.doc, mapped_pos, node_type) {
                    continue;
                }
                self.clear_incompatible(mapped_pos, node_type, None, true);
                let mapping = self.mapping.slice(map_from, None);
                let start_m = mapping.map(pos, 1);
                let end_m = mapping.map(pos + size, 1);
                let attrs_here = attrs.clone().unwrap_or(serde_json::Value::Null);
                let marks = self.doc.node_at(start_m).and_then(|n| n.marks().cloned());
                let new_node = node_type.create(attrs_here, None, marks.as_ref());
                let _ = self.maybe_step(Step::ReplaceAround(ReplaceAroundStep {
                    span: crate::transform::Span {
                        from: start_m,
                        to: end_m,
                    },
                    gap_from: start_m + 1,
                    gap_to: end_m - 1,
                    slice: Slice::new(Fragment::from(vec![new_node]), 0, 0),
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
        attrs: Option<serde_json::Value>,
        marks: Option<MarkSet<S>>,
    ) -> &mut Self {
        let node = match self.doc.resolve(pos) {
            Ok(rp) => rp.node_after().map(|c| c.into_owned()),
            Err(_) => return self,
        };
        if let Some(node) = node {
            let type_ = node_type.unwrap_or_else(|| node.r#type());
            let marks = marks.or_else(|| node.marks().cloned());
            let attrs = attrs.unwrap_or(serde_json::Value::Null);
            let new_node = type_.create(attrs, None, marks.as_ref());
            if node.is_leaf() {
                return self.replace_with(
                    pos,
                    pos + node.node_size(),
                    Fragment::from(vec![new_node]),
                );
            }
            if !type_.valid_content(node.content().unwrap_or(&Fragment::new())) {
                return self;
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
