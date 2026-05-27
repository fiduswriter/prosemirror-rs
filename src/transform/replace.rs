//! Smart replace algorithm: `replace_step()` function and the internal `Fitter`.

use crate::model::{ContentMatch, Fragment, Node, NodeType, ResolvedPos, Schema, Slice};
use crate::transform::replace_step::{ReplaceAroundStep, ReplaceStep};
use crate::transform::Span;
use crate::transform::Step;

/// Compute a `ReplaceStep` that fits the given slice into the document range.
/// Returns `None` if the step would be a no-op.
pub fn replace_step<S: Schema>(
    doc: &S::Node,
    from: usize,
    to: usize,
    slice: &Slice<S>,
) -> Option<Step<S>> {
    if from == to && slice.size() == 0 {
        return None;
    }
    let rp_from = doc.resolve(from).ok()?;
    let rp_to = doc.resolve(to).ok()?;
    if fits_trivially(&rp_from, &rp_to, slice) {
        return Some(Step::Replace(ReplaceStep {
            span: Span { from, to },
            slice: slice.clone(),
            structure: false,
        }));
    }
    Fitter::new(rp_from, rp_to, slice.clone()).fit()
}

fn fits_trivially<S: Schema>(
    rp_from: &ResolvedPos<S>,
    rp_to: &ResolvedPos<S>,
    slice: &Slice<S>,
) -> bool {
    if slice.open_start == 0 && slice.open_end == 0 && rp_from.depth == rp_to.depth {
        rp_from
            .parent()
            .can_replace(
                rp_from.index(rp_from.depth),
                rp_to.index_after(rp_to.depth),
                Some(&slice.content),
                ..,
            )
            .unwrap_or(false)
    } else {
        false
    }
}

struct FrontierItem<S: Schema> {
    node_type: S::NodeType,
    match_: S::ContentMatch,
}

struct Fittable {
    slice_depth: usize,
    frontier_depth: usize,
}

struct CloseLevel<S: Schema> {
    depth: usize,
    fit_fragment: Fragment<S>,
    move_pos: usize,
}

#[allow(dead_code)]
struct Fitter<S: Schema> {
    from_pos: usize,
    to_pos: usize,
    from_depth: usize,
    to_depth: usize,
    frontier: Vec<FrontierItem<S>>,
    placed: Fragment<S>,
    unplaced: Slice<S>,
    doc: *const S::Node,
}

#[allow(dead_code)]
impl<S: Schema> Fitter<S> {
    fn new(from_rp: ResolvedPos<S>, to_rp: ResolvedPos<S>, slice: Slice<S>) -> Self {
        let from_pos = from_rp.pos;
        let to_pos = to_rp.pos;
        let doc = from_rp.doc();

        let mut frontier = Vec::new();
        for i in 0..=from_rp.depth {
            let node = from_rp.node(i);
            let match_ = node
                .content_match_at(from_rp.index_after(i))
                .unwrap_or_else(|_| node.r#type().content_match());
            frontier.push(FrontierItem {
                node_type: node.r#type(),
                match_,
            });
        }

        let mut placed = Fragment::new();
        for i in (1..=from_rp.depth).rev() {
            placed = Fragment::from(vec![from_rp.node(i).copy(|_| placed)]);
        }

        Fitter {
            from_pos,
            to_pos,
            from_depth: from_rp.depth,
            to_depth: to_rp.depth,
            frontier,
            placed,
            unplaced: slice,
            doc: doc as *const S::Node,
        }
    }

    fn doc(&self) -> &S::Node {
        unsafe { &*self.doc }
    }

    fn depth(&self) -> usize {
        self.frontier.len() - 1
    }

    #[allow(unused_assignments)]
    fn fit(mut self) -> Option<Step<S>> {
        while self.unplaced.size() > 0 {
            if let Some(fit) = self.find_fittable() {
                self.place_nodes(fit);
            } else if !self.open_more() {
                self.drop_node();
            }
        }

        let move_inline = self.must_move_inline();
        let placed_size = self.placed.size() - self.depth() - self.from_depth;
        let to_depth = self.to_depth;
        let to_pos = self.to_pos;
        let from_depth = self.from_depth;
        let from_pos = self.from_pos;

        // Resolve positions using the raw pointer to avoid borrow conflicts
        let doc = unsafe { &*self.doc };
        let to_rp = doc.resolve(to_pos).ok()?;
        let to_end = to_rp.end(to_depth);
        let to_close = if move_inline > 0 {
            doc.resolve(move_inline).ok()
        } else {
            None
        };
        let close_ref = to_close.as_ref().unwrap_or(&to_rp);
        let close_result = self.close(close_ref)?;

        let mut content = std::mem::replace(&mut self.placed, Fragment::new());
        let mut open_start = from_depth;
        let mut open_end = close_result.depth;
        while open_start > 0 && open_end > 0 && content.child_count() == 1 {
            if let Some(first) = content.first_child() {
                if let Some(first_content) = first.content() {
                    content = first_content.clone();
                    open_start -= 1;
                    open_end -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let slice = Slice::new(content, open_start, open_end);
        if move_inline > 0 {
            return Some(Step::ReplaceAround(ReplaceAroundStep {
                span: Span {
                    from: from_pos,
                    to: move_inline,
                },
                gap_from: to_pos,
                gap_to: to_end,
                slice,
                insert: placed_size,
                structure: false,
            }));
        }
        if slice.size() > 0 || from_pos != to_pos {
            return Some(Step::Replace(ReplaceStep {
                span: Span {
                    from: from_pos,
                    to: close_result.pos,
                },
                slice,
                structure: false,
            }));
        }
        None
    }

    fn find_fittable(&self) -> Option<Fittable> {
        let mut start_depth = self.unplaced.open_start;
        let mut open_end = self.unplaced.open_end;
        let mut cur = &self.unplaced.content;

        for d in 0..start_depth {
            if let Some(first_child) = cur.first_child() {
                if cur.child_count() > 1 {
                    open_end = 0;
                }
                if first_child.r#type().is_atom() && open_end <= d {
                    start_depth = d;
                    break;
                }
                if let Some(content) = first_child.content() {
                    cur = content;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        for pass in 0..2 {
            let slice_start = if pass == 0 {
                start_depth
            } else {
                self.unplaced.open_start
            };
            for slice_depth in (0..=slice_start).rev() {
                let (parent_frag, parent_nt) = if slice_depth > 0 {
                    let parent = content_at(&self.unplaced.content, slice_depth - 1);
                    match parent.first_child() {
                        Some(first) => (
                            first.content().cloned().unwrap_or_default(),
                            Some(first.r#type()),
                        ),
                        None => continue,
                    }
                } else {
                    (self.unplaced.content.clone(), None)
                };
                let first = parent_frag.first_child();

                for frontier_depth in (0..=self.depth()).rev() {
                    let type_ = self.frontier[frontier_depth].node_type;
                    let match_ = self.frontier[frontier_depth].match_;

                    if pass == 0 {
                        let fits = if let Some(f) = first {
                            match_.match_type(f.r#type()).is_some()
                        } else {
                            parent_nt.is_some_and(|p| type_.compatible_content(p))
                        };
                        if fits {
                            return Some(Fittable {
                                slice_depth,
                                frontier_depth,
                            });
                        }
                        if let Some(f) = first {
                            let inject =
                                match_.fill_before(&Fragment::from(vec![f.clone()]), false, 0);
                            if inject.is_some() {
                                return Some(Fittable {
                                    slice_depth,
                                    frontier_depth,
                                });
                            }
                        }
                    } else if pass == 1 {
                        if let Some(f) = first {
                            let wrap = match_.find_wrapping(f.r#type());
                            if let Some(ref w) = wrap {
                                if !w.is_empty() {
                                    return Some(Fittable {
                                        slice_depth,
                                        frontier_depth,
                                    });
                                }
                            }
                        }
                    }

                    if let Some(pnt) = parent_nt {
                        if match_.match_type(pnt).is_some() {
                            break;
                        }
                    }
                }
            }
        }
        None
    }

    fn open_more(&mut self) -> bool {
        let content = self.unplaced.content.clone();
        let open_start = self.unplaced.open_start;
        let open_end = self.unplaced.open_end;
        let inner = content_at(&content, open_start);
        if inner.child_count() == 0 || inner.first_child().is_some_and(|c| c.is_leaf()) {
            return false;
        }
        let new_open_end = if inner.size() + open_start >= content.size() - open_end {
            open_start + 1
        } else {
            0
        };
        self.unplaced = Slice::new(content, open_start + 1, usize::max(open_end, new_open_end));
        true
    }

    fn drop_node(&mut self) {
        let content = self.unplaced.content.clone();
        let open_start = self.unplaced.open_start;
        let open_end = self.unplaced.open_end;
        let inner = content_at(&content, open_start);
        if inner.child_count() <= 1 && open_start > 0 {
            let open_at_end = content.size() - open_start <= open_start + inner.size();
            self.unplaced = Slice::new(
                drop_from_fragment(&content, open_start - 1, 1),
                open_start - 1,
                if open_at_end {
                    open_start - 1
                } else {
                    open_end
                },
            );
        } else {
            self.unplaced = Slice::new(
                drop_from_fragment(&content, open_start, 1),
                open_start,
                open_end,
            );
        }
    }

    fn place_nodes(&mut self, fittable: Fittable) {
        let slice_depth = fittable.slice_depth;
        let frontier_depth = fittable.frontier_depth;

        while self.depth() > frontier_depth {
            self.close_frontier_node();
        }

        let slice = self.unplaced.clone();
        let fragment = slice.content.clone();
        let open_start = slice.open_start.saturating_sub(slice_depth);
        let mut taken = 0;
        let mut add = Vec::new();
        let mut match_ = self.frontier[frontier_depth].match_;
        let _type_ = self.frontier[frontier_depth].node_type;

        let open_end_count = (fragment.size() + slice_depth) as isize
            - (slice.content.size() - slice.open_end) as isize;

        while taken < fragment.child_count() {
            if let Some(next) = fragment.maybe_child(taken) {
                if let Some(matches) = match_.match_type(next.r#type()) {
                    taken += 1;
                    if taken > 1 || open_start == 0 || next.content_size() > 0 {
                        match_ = matches;
                        let oc = if taken == 1 { open_start as isize } else { 0 };
                        let oe = if taken == fragment.child_count() {
                            open_end_count
                        } else {
                            -1
                        };
                        let closed = close_node_start::<S>(next, oc, oe);
                        add.push(closed);
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let to_end = taken == fragment.child_count();
        let actual_open_end = if to_end { open_end_count } else { -1 };

        self.placed = add_to_fragment(&self.placed, frontier_depth, &Fragment::from(add));
        self.frontier[frontier_depth].match_ = match_;

        if to_end && actual_open_end < 0 && self.frontier.len() > 1 {
            self.close_frontier_node();
        }

        let mut cur_fragment = fragment;
        for _ in 0..actual_open_end.max(0) as usize {
            if let Some(node) = cur_fragment.last_child() {
                let nc_match = node
                    .content_match_at(node.child_count())
                    .unwrap_or_else(|_| node.r#type().content_match());
                self.frontier.push(FrontierItem {
                    node_type: node.r#type(),
                    match_: nc_match,
                });
                if let Some(c) = node.content() {
                    cur_fragment = c.clone();
                }
            }
        }

        if !to_end {
            self.unplaced = Slice::new(
                drop_from_fragment(&slice.content, slice_depth, taken),
                slice.open_start,
                slice.open_end,
            );
        } else if slice_depth == 0 {
            self.unplaced = Slice::default();
        } else {
            self.unplaced = Slice::new(
                drop_from_fragment(&slice.content, slice_depth - 1, 1),
                slice_depth - 1,
                if actual_open_end < 0 {
                    slice.open_end
                } else {
                    slice_depth - 1
                },
            );
        }
    }

    fn must_move_inline(&self) -> usize {
        let doc = unsafe { &*self.doc };
        let to_rp = match doc.resolve(self.to_pos) {
            Ok(rp) => rp,
            Err(_) => return 0,
        };
        if !to_rp.parent().r#type().is_textblock() {
            return 0;
        }
        let top = &self.frontier[self.depth()];
        if !top.node_type.is_textblock() {
            return 0;
        }
        let after_fits =
            content_after_fits(&to_rp, self.to_depth, top.node_type, top.match_, false);
        if after_fits.is_none() {
            return 0;
        }
        let close_level = self.find_close_level(&to_rp);
        if self.to_depth == self.depth() {
            if let Some(ref level) = close_level {
                if level.depth == self.depth() {
                    return 0;
                }
            }
        }
        let mut depth = self.to_depth;
        let mut after = to_rp.after(depth).unwrap_or(0);
        while depth > 1 {
            depth -= 1;
            if after != to_rp.end(depth) {
                break;
            }
            after += 1;
        }
        after
    }

    fn find_close_level(&self, to: &ResolvedPos<S>) -> Option<CloseLevel<S>> {
        let max_depth = usize::min(self.depth(), to.depth);
        for i in (0..=max_depth).rev() {
            let match_ = self.frontier[i].match_;
            let type_ = self.frontier[i].node_type;
            let drop_inner = i < to.depth && to.end(i + 1) == to.pos + (to.depth - (i + 1));
            let fit = content_after_fits(to, i, type_, match_, drop_inner);
            if fit.is_none() {
                continue;
            }
            let mut valid = true;
            for d in (0..i).rev() {
                let match2 = self.frontier[d].match_;
                let type2 = self.frontier[d].node_type;
                let matches = content_after_fits(to, d, type2, match2, true);
                match matches {
                    None => {
                        valid = false;
                        break;
                    }
                    Some(ref f) if f.child_count() > 0 => {
                        valid = false;
                        break;
                    }
                    _ => {}
                }
            }
            if valid {
                let move_pos = if drop_inner {
                    to.after(i + 1).unwrap_or(to.pos)
                } else {
                    to.pos
                };
                return Some(CloseLevel {
                    depth: i,
                    fit_fragment: fit.unwrap(),
                    move_pos,
                });
            }
        }
        None
    }

    fn close(&mut self, to: &ResolvedPos<S>) -> Option<CloseResult> {
        let close = self.find_close_level(to)?;
        while self.depth() > close.depth {
            self.close_frontier_node();
        }
        if close.fit_fragment.child_count() > 0 {
            self.placed = add_to_fragment(&self.placed, close.depth, &close.fit_fragment);
        }

        // Determine the effective target position after the close-level move.
        // In JS this is `close.move`, which is either the original `$to` or a
        // re-resolved position when `dropInner` is true.
        let doc = unsafe { &*self.doc };
        let move_to_opt: Option<ResolvedPos<S>>;
        let move_to: &ResolvedPos<S> = if close.move_pos == to.pos {
            to
        } else {
            move_to_opt = doc.resolve(close.move_pos).ok();
            move_to_opt.as_ref()?
        };

        // Open new frontier nodes for each depth level between close.depth+1
        // and move_to.depth.  This is the critical part that JS's Fitter does
        // and was missing from the Rust port — without it, the trailing content
        // (e.g. "you" after inserting a paragraph) has no open slot to merge
        // into and gets incorrectly joined with the last slice node.
        for d in (close.depth + 1)..=move_to.depth {
            let node = move_to.node(d);
            let index = move_to.index(d);
            let fill = node
                .r#type()
                .content_match()
                .fill_before(node.content().unwrap_or(Fragment::EMPTY_REF), true, index)
                .unwrap_or_default();
            let fill_opt = if fill.child_count() > 0 {
                Some(fill)
            } else {
                None
            };
            self.open_frontier_node(node.r#type(), fill_opt);
        }

        Some(CloseResult {
            pos: close.move_pos,
            depth: move_to.depth,
        })
    }

    fn open_frontier_node(&mut self, type_: S::NodeType, content: Option<Fragment<S>>) {
        let d = self.depth();
        if let Some(new_match) = self.frontier[d].match_.match_type(type_) {
            self.frontier[d].match_ = new_match;
        }
        let node = type_.create_node(content.as_ref(), None);
        self.placed = add_to_fragment(&self.placed, d, &Fragment::from(vec![node]));
        self.frontier.push(FrontierItem {
            node_type: type_,
            match_: type_.content_match(),
        });
    }

    fn close_frontier_node(&mut self) {
        let open = self.frontier.pop().unwrap();
        let empty = Fragment::new();
        if let Some(add) = open.match_.fill_before(&empty, true, 0) {
            if add.child_count() > 0 {
                self.placed = add_to_fragment(&self.placed, self.frontier.len(), &add);
            }
        }
    }
}

struct CloseResult {
    pos: usize,
    depth: usize,
}

fn drop_from_fragment<S: Schema>(
    fragment: &Fragment<S>,
    depth: usize,
    count: usize,
) -> Fragment<S> {
    if depth == 0 {
        return fragment.cut_by_index(count, fragment.child_count());
    }
    if let Some(first_child) = fragment.first_child() {
        let new_first = first_child.copy(|c| drop_from_fragment(c, depth - 1, count));
        fragment.replace_child(0, new_first).into_owned()
    } else {
        fragment.clone()
    }
}

fn add_to_fragment<S: Schema>(
    fragment: &Fragment<S>,
    depth: usize,
    content: &Fragment<S>,
) -> Fragment<S> {
    if depth == 0 {
        return fragment.clone().append(content.clone());
    }
    if let Some(last_child) = fragment.last_child() {
        let new_last = last_child.copy(|c| add_to_fragment(c, depth - 1, content));
        let idx = fragment.child_count() - 1;
        fragment.replace_child(idx, new_last).into_owned()
    } else {
        fragment.clone()
    }
}

fn content_at<S: Schema>(fragment: &Fragment<S>, depth: usize) -> Fragment<S> {
    let mut cur = fragment.clone();
    for _ in 0..depth {
        if let Some(first) = cur.first_child() {
            if let Some(c) = first.content() {
                cur = c.clone();
            } else {
                break;
            }
        } else {
            break;
        }
    }
    cur
}

fn close_node_start<S: Schema>(node: &S::Node, open_start: isize, open_end: isize) -> S::Node {
    if open_start <= 0 {
        return node.clone();
    }
    let mut frag = node.content().cloned().unwrap_or_default();
    if open_start > 1 {
        if let Some(first) = frag.first_child() {
            let new_first = close_node_start::<S>(
                first,
                open_start - 1,
                if frag.child_count() == 1 {
                    open_end - 1
                } else {
                    0
                },
            );
            frag = frag.replace_child(0, new_first).into_owned();
        }
    }
    if open_start > 0 {
        if let Some(fill) = node.r#type().content_match().fill_before(&frag, false, 0) {
            frag = fill.append(frag);
            if open_end <= 0 {
                if let Some(matched) = node.r#type().content_match().match_fragment(&frag) {
                    if let Some(tail) = matched.fill_before(&Fragment::new(), true, 0) {
                        frag = frag.append(tail);
                    }
                }
            }
        }
    }
    node.copy(|_| frag)
}

fn content_after_fits<S: Schema>(
    to: &ResolvedPos<S>,
    depth: usize,
    type_: S::NodeType,
    match_: S::ContentMatch,
    open: bool,
) -> Option<Fragment<S>> {
    let node = to.node(depth);
    let index = if open {
        to.index_after(depth)
    } else {
        to.index(depth)
    };
    if index == node.child_count() && !type_.compatible_content(node.r#type()) {
        return None;
    }
    let fit = match_.fill_before(node.content().unwrap_or(Fragment::EMPTY_REF), true, index);
    match fit {
        Some(ref f)
            if !invalid_marks(type_, node.content().unwrap_or(Fragment::EMPTY_REF), index) =>
        {
            Some(f.clone())
        }
        _ => None,
    }
}

fn invalid_marks<S: Schema>(type_: S::NodeType, fragment: &Fragment<S>, start: usize) -> bool {
    for i in start..fragment.child_count() {
        if let Some(child) = fragment.maybe_child(i) {
            if let Some(marks) = child.marks() {
                if !type_.allow_marks(marks) {
                    return true;
                }
            }
        }
    }
    false
}

/// Close an open fragment, filling in missing content as needed.
pub fn close_fragment<S: Schema>(
    fragment: &Fragment<S>,
    depth: usize,
    old_open: usize,
    new_open: usize,
    parent: Option<&S::Node>,
) -> Fragment<S> {
    let mut fragment = fragment.clone();
    if depth < old_open {
        if let Some(first) = fragment.first_child() {
            let new_first =
                first.copy(|c| close_fragment(c, depth + 1, old_open, new_open, Some(first)));
            fragment = fragment.replace_child(0, new_first).into_owned();
        }
    }
    if depth > new_open {
        if let Some(parent) = parent {
            if let Ok(match_) = parent.content_match_at(0) {
                if let Some(fill) = match_.fill_before(&fragment, false, 0) {
                    let start = fill.append(fragment);
                    if let Some(matched) = match_.match_fragment(&start) {
                        if let Some(tail) = matched.fill_before(&Fragment::new(), true, 0) {
                            return start.append(tail);
                        }
                    }
                    return start;
                }
            }
        }
    }
    fragment
}

/// Compute the list of depths fully covered by the given range.
pub fn covered_depths<S: Schema>(from: &ResolvedPos<S>, to: &ResolvedPos<S>) -> Vec<usize> {
    let mut result = Vec::new();
    let min_depth = usize::min(from.depth, to.depth);
    for d in (0..=min_depth).rev() {
        let start = from.start(d);
        if start < from.pos - (from.depth - d) || to.end(d) > to.pos + (to.depth - d) {
            break;
        }
        if start == to.start(d)
            || (d == from.depth
                && d == to.depth
                && from.parent().r#type().inline_content()
                && to.parent().r#type().inline_content()
                && d > 0
                && to.start(d - 1) == start - 1)
        {
            result.push(d);
        }
    }
    result
}
