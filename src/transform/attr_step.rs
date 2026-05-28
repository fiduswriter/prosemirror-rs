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

    /// Return the inverse of this step, restoring the original attribute value.
    pub fn invert<S: Schema>(&self, doc: &S::Node) -> super::Step<S> {
        let node = doc.node_at(self.pos).unwrap_or(doc);
        let attrs = node.attrs_json();
        let old_value = attrs
            .get(&self.attr)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        super::Step::Attr(AttrStep {
            pos: self.pos,
            attr: self.attr.clone(),
            value: old_value,
        })
    }

    /// Apply this step to the given document, setting an attribute on the node at `self.pos`.
    pub fn apply<S: Schema>(&self, doc: &S::Node) -> StepResult<S> {
        // Use node_at to find the target node, matching upstream JS behavior.
        let target = doc.node_at(self.pos).ok_or(StepError::NoNodeAtPosition)?;
        let new_target = target.with_attr(&self.attr, self.value.clone());

        // Walk down from the doc to find the path to the target, then rebuild bottom-up.
        // This mirrors the logic of node_at but records the path.
        let mut path: Vec<(usize, S::Node)> = Vec::new();
        let mut node: &S::Node = doc;
        let mut pos = self.pos;
        while let Some(content) = node.content() {
            let idx = match content.find_index(pos, false) {
                Ok(i) => i,
                Err(_) => break,
            };
            let child = match content.maybe_child(idx.index) {
                Some(c) => c,
                None => break,
            };
            if idx.offset == pos || child.is_text() {
                // This child is the target
                path.push((idx.index, node.copy(|_| content.clone())));
                break;
            }
            path.push((idx.index, node.copy(|_| content.clone())));
            pos -= idx.offset + 1;
            node = child;
        }

        if path.is_empty() {
            // Target is the doc itself
            return Ok(new_target);
        }

        // Rebuild bottom-up
        let mut current = new_target;
        for (child_idx, parent_copy) in path.into_iter().rev() {
            let content = parent_copy.content().unwrap();
            let new_content = content.replace_child(child_idx, current);
            current = parent_copy.copy(|_| new_content.into_owned());
        }
        Ok(current)
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

    /// Return the inverse of this step, restoring the original attribute value.
    pub fn invert<S: Schema>(&self, doc: &S::Node) -> super::Step<S> {
        let attrs = doc.attrs_json();
        let old_value = attrs
            .get(&self.attr)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        super::Step::DocAttr(DocAttrStep {
            attr: self.attr.clone(),
            value: old_value,
        })
    }

    /// Apply this step to the given document, setting an attribute on the document root node.
    pub fn apply<S: Schema>(&self, doc: &S::Node) -> StepResult<S> {
        Ok(doc.with_attr(&self.attr, self.value.clone()))
    }
}
