use super::map::{Mappable, StepMap};
use super::{Span, StepError, StepKind, StepResult};
use crate::model::{Node, ResolveErr, Schema, Slice};
use derivative::Derivative;
use serde::{Deserialize, Serialize};

/// Replace some part of the document
#[derive(Derivative, Deserialize, Serialize)]
#[derivative(
    Debug(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = ""),
    Clone(bound = "")
)]
#[serde(bound = "", rename_all = "camelCase")]
pub struct ReplaceStep<S: Schema> {
    /// The affected span
    #[serde(flatten)]
    pub span: Span,
    /// The slice to replace the current content with
    #[serde(default)]
    pub slice: Slice<S>,
    /// Whether this is a structural change
    #[serde(default)]
    pub structure: bool,
}

impl<S: Schema> StepKind<S> for ReplaceStep<S> {
    fn apply(&self, doc: &S::Node) -> StepResult<S> {
        let from = self.span.from;
        let to = self.span.to;
        if self.structure && content_between::<S>(doc, from, to)? {
            Err(StepError::WouldOverwrite)
        } else {
            let node = doc.replace(from..to, &self.slice)?;
            Ok(node)
        }
    }

    fn get_map(&self) -> StepMap {
        StepMap::new(vec![
            self.span.from,
            self.span.to - self.span.from,
            self.slice.size(),
        ])
    }

    fn invert(&self, doc: &S::Node) -> super::Step<S> {
        super::Step::Replace(ReplaceStep {
            span: Span {
                from: self.span.from,
                to: self.span.from + self.slice.size(),
            },
            slice: doc
                .slice(self.span.from..self.span.to, false)
                .unwrap_or_default(),
            structure: false,
        })
    }

    fn map<M: Mappable>(&self, mapping: &M) -> Option<super::Step<S>> {
        let from = mapping.map_result(self.span.from, 1);
        let to = mapping.map_result(self.span.to, -1);
        if from.deleted() && to.deleted() {
            return None;
        }
        Some(super::Step::Replace(ReplaceStep {
            span: Span {
                from: from.pos,
                to: usize::max(from.pos, to.pos),
            },
            slice: self.slice.clone(),
            structure: self.structure,
        }))
    }

    fn merge(&self, other: &super::Step<S>) -> Option<super::Step<S>> {
        match other {
            super::Step::Replace(other) if !other.structure && !self.structure => {
                if self.span.from + self.slice.size() == other.span.from
                    && self.slice.open_end == 0
                    && other.slice.open_start == 0
                {
                    let slice = if self.slice.size() + other.slice.size() == 0 {
                        Slice::default()
                    } else {
                        Slice::new(
                            self.slice
                                .content
                                .clone()
                                .append(other.slice.content.clone()),
                            self.slice.open_start,
                            other.slice.open_end,
                        )
                    };
                    Some(super::Step::Replace(ReplaceStep {
                        span: Span {
                            from: self.span.from,
                            to: self.span.to + (other.span.to - other.span.from),
                        },
                        slice,
                        structure: self.structure,
                    }))
                } else if other.span.to == self.span.from
                    && self.slice.open_start == 0
                    && other.slice.open_end == 0
                {
                    let slice = if self.slice.size() + other.slice.size() == 0 {
                        Slice::default()
                    } else {
                        Slice::new(
                            other
                                .slice
                                .content
                                .clone()
                                .append(self.slice.content.clone()),
                            other.slice.open_start,
                            self.slice.open_end,
                        )
                    };
                    Some(super::Step::Replace(ReplaceStep {
                        span: Span {
                            from: other.span.from,
                            to: self.span.to,
                        },
                        slice,
                        structure: self.structure,
                    }))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Replace the document structure while keeping some content
#[derive(Derivative, Deserialize, Serialize)]
#[derivative(
    Debug(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = ""),
    Clone(bound = "")
)]
#[serde(bound = "", rename_all = "camelCase")]
pub struct ReplaceAroundStep<S: Schema> {
    /// The affected part of the document
    #[serde(flatten)]
    pub span: Span,
    /// Start of the gap
    pub gap_from: usize,
    /// End of the gap
    pub gap_to: usize,
    /// The inner slice
    #[serde(default)]
    pub slice: Slice<S>,
    /// ???
    pub insert: usize,
    /// Whether this is a structural change
    #[serde(default)]
    pub structure: bool,
}

impl<S: Schema> StepKind<S> for ReplaceAroundStep<S> {
    fn apply(&self, doc: &S::Node) -> StepResult<S> {
        if self.structure
            && (content_between::<S>(doc, self.span.from, self.gap_from)?
                || content_between::<S>(doc, self.gap_to, self.span.to)?)
        {
            return Err(StepError::GapWouldOverwrite);
        }

        let gap = doc.slice(self.gap_from..self.gap_to, false)?;
        if gap.open_start != 0 || gap.open_end != 0 {
            return Err(StepError::GapNotFlat);
        }

        let inserted = self.slice.insert_at(self.insert, gap.content)?;
        let inserted = inserted.ok_or(StepError::GapNotFit)?;

        let result = doc.replace(self.span.from..self.span.to, &inserted)?;
        Ok(result)
    }

    fn get_map(&self) -> StepMap {
        StepMap::new(vec![
            self.span.from,
            self.gap_from - self.span.from,
            self.insert,
            self.gap_to,
            self.span.to - self.gap_to,
            self.slice.size() - self.insert,
        ])
    }

    fn invert(&self, doc: &S::Node) -> super::Step<S> {
        let gap = self.gap_to - self.gap_from;
        let old_slice = doc
            .slice(self.span.from..self.span.to, false)
            .unwrap_or_default();
        let removed =
            old_slice.remove_between(self.gap_from - self.span.from, self.gap_to - self.span.from);
        super::Step::ReplaceAround(ReplaceAroundStep {
            span: Span {
                from: self.span.from,
                to: self.span.from + self.slice.size() + gap,
            },
            gap_from: self.span.from + self.insert,
            gap_to: self.span.from + self.insert + gap,
            slice: removed,
            insert: self.gap_from - self.span.from,
            structure: self.structure,
        })
    }

    fn map<M: Mappable>(&self, mapping: &M) -> Option<super::Step<S>> {
        let from = mapping.map_result(self.span.from, 1);
        let to = mapping.map_result(self.span.to, -1);
        let gap_from = if self.span.from == self.gap_from {
            from.pos
        } else {
            mapping.map(self.gap_from, -1)
        };
        let gap_to = if self.span.to == self.gap_to {
            to.pos
        } else {
            mapping.map(self.gap_to, 1)
        };
        if (from.deleted() && to.deleted()) || gap_from < from.pos || gap_to > to.pos {
            return None;
        }
        Some(super::Step::ReplaceAround(ReplaceAroundStep {
            span: Span {
                from: from.pos,
                to: to.pos,
            },
            gap_from,
            gap_to,
            slice: self.slice.clone(),
            insert: self.insert,
            structure: self.structure,
        }))
    }
}

pub(super) fn content_between<S: Schema>(
    doc: &S::Node,
    from: usize,
    to: usize,
) -> Result<bool, ResolveErr> {
    let rp_from = doc.resolve(from)?;
    let mut dist = to - from;
    let mut depth = rp_from.depth;
    while dist > 0 && depth > 0 && rp_from.index_after(depth) == rp_from.node(depth).child_count() {
        depth -= 1;
        dist -= 1;
    }
    if dist > 0 {
        let mut next = rp_from.node(depth).maybe_child(rp_from.index_after(depth));
        while dist > 0 {
            match next {
                Some(c) => {
                    if c.is_leaf() {
                        return Ok(true);
                    } else {
                        next = c.first_child();
                        dist -= 1;
                    }
                }
                None => {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
