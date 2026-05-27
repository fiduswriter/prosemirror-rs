//! Attribute step types: `AttrStep` and `DocAttrStep`.

use super::map::{Mappable, StepMap};
use crate::model::{Node, Schema};
use crate::transform::{StepError, StepResult};
use serde::{Deserialize, Serialize};

/// Changes a single named attribute on the node at a given position.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttrStep {
    /// The position of the node whose attribute should be changed
    pub pos: usize,
    /// The attribute name
    pub attr: String,
    /// The new attribute value
    pub value: serde_json::Value,
}

impl AttrStep {
    /// Get the step map for this step (empty — no position shift)
    pub fn get_map(&self) -> StepMap {
        StepMap::EMPTY
    }

    /// Map this step through a mapping. Returns None if the position was deleted.
    pub fn map_step<M: Mappable>(&self, mapping: &M) -> Option<AttrStep> {
        let pos = mapping.map_result(self.pos, 1);
        if pos.deleted_after() {
            None
        } else {
            Some(AttrStep {
                pos: pos.pos,
                attr: self.attr.clone(),
                value: self.value.clone(),
            })
        }
    }

    /// Apply this step to the given document, setting an attribute on the node at `self.pos`.
    pub fn apply<S: Schema>(&self, doc: &S::Node) -> StepResult<S> {
        if self.pos == 0 {
            // Position 0 means the document root itself
            return Ok(doc.with_attr(&self.attr, self.value.clone()));
        }
        // Resolve the position to find which node we're pointing at.
        // If pos points to the start of a node, that node is the target.
        // Otherwise, if pos is inside a node, we target that node.
        let resolved = doc.resolve(self.pos)?;

        // The target node is the node at the deepest resolved level.
        // We need to find it in its parent's children and replace it.
        let depth = resolved.depth;
        if depth == 0 {
            // Direct child of doc — use index to find it
            let index = resolved.index(0);
            let child = doc.child(index).ok_or(StepError::NoNodeAtPosition)?;
            let new_child = child.with_attr(&self.attr, self.value.clone());
            let new_content = doc.content().unwrap().replace_child(index, new_child);
            let new_doc = doc.copy(|_| new_content.into_owned());
            return Ok(new_doc);
        }

        // Walk from the deepest level up, rebuilding each level
        let target = resolved.node(depth);
        let new_target = target.with_attr(&self.attr, self.value.clone());

        // Rebuild parent chain bottom-up
        let mut current_node = new_target;
        for d in (1..=depth).rev() {
            let parent = resolved.node(d - 1);
            let child_idx = resolved.index(d);
            let content = parent.content().unwrap();
            let new_content = content.replace_child(child_idx, current_node);
            current_node = parent.copy(|_| new_content.into_owned());
        }
        Ok(current_node)
    }
}

/// Changes a single named attribute on the document root node.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocAttrStep {
    /// The attribute name
    pub attr: String,
    /// The new attribute value
    pub value: serde_json::Value,
}

impl DocAttrStep {
    /// Get the step map for this step (empty — no position shift)
    pub fn get_map(&self) -> StepMap {
        StepMap::EMPTY
    }

    /// Map this step through a mapping. Always returns self (doc-level step).
    pub fn map_step<M: Mappable>(&self, _mapping: &M) -> Option<DocAttrStep> {
        Some(DocAttrStep {
            attr: self.attr.clone(),
            value: self.value.clone(),
        })
    }

    /// Apply this step to the given document, setting an attribute on the document root node.
    pub fn apply<S: Schema>(&self, doc: &S::Node) -> StepResult<S> {
        Ok(doc.with_attr(&self.attr, self.value.clone()))
    }
}
