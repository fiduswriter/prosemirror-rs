use crate::model::{Fragment, Schema};
use displaydoc::Display;
use std::ops::RangeBounds;
use thiserror::Error;

/// Error on content matching
#[derive(Debug, Display, Error)]
pub enum ContentMatchError {
    /// Called contentMatchAt on a node with invalid content
    InvalidContent,
}

/// Instances of this class represent a match state of a node type's content expression, and can be
/// used to find out whether further content matches here, and whether a given position is a valid end of the node.
pub trait ContentMatch<S: Schema>: Copy {
    /// Try to match a fragment. Returns the resulting match when successful.
    fn match_fragment(self, fragment: &Fragment<S>) -> Option<Self> {
        self.match_fragment_range(fragment, ..)
    }

    /// Try to match a part of a fragment. Returns the resulting match when successful.
    fn match_fragment_range<R: RangeBounds<usize>>(
        self,
        fragment: &Fragment<S>,
        range: R,
    ) -> Option<Self>;

    /// True when this match state represents a valid end of the node.
    fn valid_end(self) -> bool;

    /// Match a node type, returning a match after that node if successful.
    fn match_type(self, r#type: S::NodeType) -> Option<Self>;

    /// Find the nodes that need to be inserted before `after` to reach a valid
    /// accepting state. Returns `None` if no valid insertion exists.
    fn fill_before(
        self,
        _after: &Fragment<S>,
        _to_end: bool,
        _start_index: usize,
    ) -> Option<Fragment<S>> {
        None
    }

    /// Find the list of node types needed to wrap content so that `target` becomes valid.
    fn find_wrapping(self, _target: S::NodeType) -> Option<Vec<S::NodeType>> {
        None
    }

    /// Test whether two content match states share any possible next node type.
    fn compatible(self, _other: Self) -> bool {
        false
    }

    /// Whether this match state expects inline content.
    fn inline_content(self) -> bool {
        false
    }

    /// The number of outgoing edges from this DFA state.
    fn edge_count(self) -> usize {
        0
    }

    /// Get the nth outgoing edge from this DFA state, as a (node_type, next_state) pair.
    fn edge(self, _n: usize) -> Option<(S::NodeType, Self)> {
        None
    }
}
