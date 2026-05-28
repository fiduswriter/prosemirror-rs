//! Node mark step types: `AddNodeMarkStep` and `RemoveNodeMarkStep`.

use super::map::{Mappable, StepMap};
use super::step::StepKind;
use super::StepResult;
use crate::model::{Fragment, Mark, Node, Schema, Slice};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Adds a mark to a specific node at a given position.
#[derive(Derivative, Deserialize, Serialize)]
#[derivative(
    Debug(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = ""),
    Clone(bound = "")
)]
#[serde(bound = "", rename_all = "camelCase")]
pub struct AddNodeMarkStep<S: Schema> {
    /// The position of the node
    pub pos: usize,
    /// The mark to add
    pub mark: S::Mark,
}

/// Removes a mark from a specific node at a given position.
#[derive(Derivative, Deserialize, Serialize)]
#[derivative(
    Debug(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = ""),
    Clone(bound = "")
)]
#[serde(bound = "", rename_all = "camelCase")]
pub struct RemoveNodeMarkStep<S: Schema> {
    /// The position of the node
    pub pos: usize,
    /// The mark to remove
    pub mark: S::Mark,
}

impl<S: Schema> StepKind<S> for AddNodeMarkStep<S> {
    fn apply(&self, doc: &S::Node) -> StepResult<S> {
        let rp = doc.resolve(self.pos).map_err(super::StepError::from)?;
        // Get the node before or at the position
        let node = rp
            .node_after()
            .map(|c| c.into_owned())
            .or_else(|| rp.node_before().map(|c| c.into_owned()))
            .ok_or(super::StepError::NoNodeAtPosition)?;

        let new_marks = node.marks().map(Cow::Borrowed).unwrap_or_default();
        let updated = node.mark(self.mark.add_to_set(new_marks).into_owned());
        let is_leaf = updated.is_leaf();
        let slice = Slice::new(
            Fragment::from(vec![updated]),
            0,
            if is_leaf { 0 } else { 1 },
        );
        Ok(doc.replace(self.pos..self.pos + 1, &slice)?)
    }

    fn get_map(&self) -> StepMap {
        StepMap::EMPTY
    }

    fn invert(&self, doc: &S::Node) -> super::Step<S> {
        let rp = doc.resolve(self.pos);
        if let Ok(rp) = rp {
            if let Some(node) = rp
                .node_after()
                .map(|c| c.into_owned())
                .or_else(|| rp.node_before().map(|c| c.into_owned()))
            {
                let marks = node.marks().map(Cow::Borrowed).unwrap_or_default();
                let new_set = self.mark.add_to_set(marks);
                if new_set.len() == node.marks().map(|m| m.len()).unwrap_or(0) {
                    // Mark was already present or replaced one
                    // Check if the mark was actually replaced
                    return super::Step::RemoveNodeMark(RemoveNodeMarkStep {
                        pos: self.pos,
                        mark: self.mark.clone(),
                    });
                }
            }
        }
        super::Step::RemoveNodeMark(RemoveNodeMarkStep {
            pos: self.pos,
            mark: self.mark.clone(),
        })
    }

    fn map<M: Mappable>(&self, mapping: &M) -> Option<super::Step<S>> {
        let pos = mapping.map_result(self.pos, 1);
        if pos.deleted_after() {
            None
        } else {
            Some(super::Step::AddNodeMark(AddNodeMarkStep {
                pos: pos.pos,
                mark: self.mark.clone(),
            }))
        }
    }
}

impl<S: Schema> StepKind<S> for RemoveNodeMarkStep<S> {
    fn apply(&self, doc: &S::Node) -> StepResult<S> {
        let rp = doc.resolve(self.pos).map_err(super::StepError::from)?;
        let node = rp
            .node_after()
            .map(|c| c.into_owned())
            .or_else(|| rp.node_before().map(|c| c.into_owned()))
            .ok_or(super::StepError::NoNodeAtPosition)?;

        let new_marks = node.marks().map(Cow::Borrowed).unwrap_or_default();
        let updated = node.mark(self.mark.remove_from_set(new_marks).into_owned());
        let is_leaf = updated.is_leaf();
        let slice = Slice::new(
            Fragment::from(vec![updated]),
            0,
            if is_leaf { 0 } else { 1 },
        );
        Ok(doc.replace(self.pos..self.pos + 1, &slice)?)
    }

    fn get_map(&self) -> StepMap {
        StepMap::EMPTY
    }

    fn invert(&self, doc: &S::Node) -> super::Step<S> {
        let rp = doc.resolve(self.pos);
        if let Ok(rp) = rp {
            if let Some(node) = rp
                .node_after()
                .map(|c| c.into_owned())
                .or_else(|| rp.node_before().map(|c| c.into_owned()))
            {
                if let Some(marks) = node.marks() {
                    if self.mark.is_in_set(marks) {
                        return super::Step::AddNodeMark(AddNodeMarkStep {
                            pos: self.pos,
                            mark: self.mark.clone(),
                        });
                    }
                }
            }
        }
        super::Step::AddNodeMark(AddNodeMarkStep {
            pos: self.pos,
            mark: self.mark.clone(),
        })
    }

    fn map<M: Mappable>(&self, mapping: &M) -> Option<super::Step<S>> {
        let pos = mapping.map_result(self.pos, 1);
        if pos.deleted_after() {
            None
        } else {
            Some(super::Step::RemoveNodeMark(RemoveNodeMarkStep {
                pos: pos.pos,
                mark: self.mark.clone(),
            }))
        }
    }
}

impl<S: Schema> AddNodeMarkStep<S> {
    /// Get the step map (empty, no position shift)
    pub fn get_map(&self) -> StepMap {
        StepMap::EMPTY
    }

    /// Map this step through a mapping
    pub fn map_step<M: Mappable>(&self, mapping: &M) -> Option<AddNodeMarkStep<S>> {
        let pos = mapping.map_result(self.pos, 1);
        if pos.deleted_after() {
            None
        } else {
            Some(AddNodeMarkStep {
                pos: pos.pos,
                mark: self.mark.clone(),
            })
        }
    }
}

impl<S: Schema> RemoveNodeMarkStep<S> {
    /// Get the step map (empty, no position shift)
    pub fn get_map(&self) -> StepMap {
        StepMap::EMPTY
    }

    /// Map this step through a mapping
    pub fn map_step<M: Mappable>(&self, mapping: &M) -> Option<RemoveNodeMarkStep<S>> {
        let pos = mapping.map_result(self.pos, 1);
        if pos.deleted_after() {
            None
        } else {
            Some(RemoveNodeMarkStep {
                pos: pos.pos,
                mark: self.mark.clone(),
            })
        }
    }
}
