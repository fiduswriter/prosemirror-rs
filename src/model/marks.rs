use super::{MarkType, Schema};
use derivative::Derivative;
use displaydoc::Display;
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{self, Debug};
use std::{borrow::Cow, convert::TryFrom, hash::Hash};

/// A set of marks
#[derive(Derivative, Deserialize)]
#[derivative(
    Clone(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = ""),
    Default(bound = "")
)]
#[serde(bound = "", try_from = "Vec<S::Mark>")]
pub struct MarkSet<S: Schema> {
    content: Vec<S::Mark>,
}

impl<S: Schema> MarkSet<S> {
    /// Create a new empty MarkSet
    pub fn new() -> Self {
        MarkSet {
            content: Vec::new(),
        }
    }

    /// Create a MarkSet from a vector without reordering or deduping.
    /// Only use this when the caller has already validated the input.
    pub fn from_vec(content: Vec<S::Mark>) -> Self {
        MarkSet { content }
    }

    /// Check whether the set contains this exact mark
    pub fn contains(&self, mark: &S::Mark) -> bool {
        self.content.contains(mark)
    }

    /// Check whether a mark of this type is in the set, returning it if so
    pub fn is_in_set(&self, mark: &S::Mark) -> bool {
        self.content.iter().any(|m| m.r#type() == mark.r#type())
    }

    /// The number of marks in this set
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Whether this set is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Iterate over the marks in this set
    pub fn iter(&self) -> std::slice::Iter<'_, S::Mark> {
        self.content.iter()
    }

    /// Add a mark to the set, preserving rank order and handling exclusions.
    pub fn add(&mut self, mark: &S::Mark) {
        let mut copy = Vec::new();
        let mut placed = false;
        let mut modified = false;
        for other in &self.content {
            if mark == other {
                return;
            }
            if mark.r#type().excludes(other.r#type()) {
                modified = true;
                continue;
            }
            if other.r#type().excludes(mark.r#type()) {
                return;
            }
            if !placed && other.r#type().rank() > mark.r#type().rank() {
                copy.push(mark.clone());
                placed = true;
                modified = true;
            }
            copy.push(other.clone());
        }
        if !placed {
            copy.push(mark.clone());
            modified = true;
        }
        if modified {
            self.content = copy;
        }
    }

    /// Remove an exact mark from the set
    pub fn remove(&mut self, mark: &S::Mark) {
        if let Some(index) = self.content.iter().position(|m| m == mark) {
            self.content.remove(index);
        }
    }
}

impl<'a, S: Schema> IntoIterator for &'a MarkSet<S> {
    type Item = &'a S::Mark;
    type IntoIter = std::slice::Iter<'a, S::Mark>;
    fn into_iter(self) -> Self::IntoIter {
        self.content.iter()
    }
}

impl<S: Schema> Serialize for MarkSet<S> {
    fn serialize<Sr>(&self, serializer: Sr) -> Result<Sr::Ok, Sr::Error>
    where
        Sr: Serializer,
    {
        self.content.serialize(serializer)
    }
}

#[derive(Display)]
pub enum MarkSetError {
    /// Duplicate mark types
    Duplicates,
}

impl<S: Schema> TryFrom<Vec<S::Mark>> for MarkSet<S> {
    type Error = MarkSetError;
    fn try_from(value: Vec<S::Mark>) -> Result<Self, Self::Error> {
        let mut result = MarkSet::new();
        for mark in value {
            if result.contains(&mark) {
                return Err(MarkSetError::Duplicates);
            }
            result.add(&mark);
        }
        Ok(result)
    }
}

impl<S: Schema> fmt::Debug for MarkSet<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.content.fmt(f)
    }
}

/// The methods that
pub trait Mark<S: Schema<Mark = Self>>:
    Serialize + for<'de> Deserialize<'de> + Debug + Clone + PartialEq + Eq + Hash
{
    /// The type of this mark.
    fn r#type(&self) -> S::MarkType;

    /// The attributes of this mark as JSON.
    fn attrs_json(&self) -> serde_json::Value {
        serde_json::Value::Null
    }

    /// Check if a mark of this type is in the given set
    fn is_in_set(&self, set: &MarkSet<S>) -> bool {
        set.content.iter().any(|m| m == self)
    }

    /// Given a set of marks, create a new set which contains this one as well, in the right
    /// position. If this mark is already in the set, the set itself is returned. If any marks that
    /// are set to be exclusive with this mark are present, those are replaced by this one.
    fn add_to_set<'a>(&self, set: Cow<'a, MarkSet<S>>) -> Cow<'a, MarkSet<S>> {
        let mut copy = Vec::new();
        let mut placed = false;
        let mut modified = false;
        for other in set.iter() {
            if self == other {
                return set;
            }
            if self.r#type().excludes(other.r#type()) {
                modified = true;
                continue;
            }
            if other.r#type().excludes(self.r#type()) {
                return set;
            }
            if !placed && other.r#type().rank() > self.r#type().rank() {
                copy.push(self.clone());
                placed = true;
                modified = true;
            }
            copy.push(other.clone());
        }
        if !placed {
            copy.push(self.clone());
            modified = true;
        }
        if modified {
            Cow::Owned(MarkSet { content: copy })
        } else {
            set
        }
    }

    /// Remove this mark from the given set, returning a new set. If this mark is not in the set,
    /// the set itself is returned.
    fn remove_from_set<'a>(&self, set: Cow<'a, MarkSet<S>>) -> Cow<'a, MarkSet<S>> {
        if let Some(index) = set.content.iter().position(|m| m == self) {
            let mut owned_set = set.into_owned();
            owned_set.content.remove(index);
            Cow::Owned(owned_set)
        } else {
            set
        }
    }

    /// Create a set with just this mark
    fn into_set(self) -> MarkSet<S> {
        MarkSet {
            content: vec![self],
        }
    }
}
