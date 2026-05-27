//! Position mapping infrastructure: `StepMap`, `MapResult`, `Mapping`.

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

const DEL_BEFORE: u8 = 1;
const DEL_AFTER: u8 = 2;
const DEL_ACROSS: u8 = 4;
const DEL_SIDE: u8 = 8;

/// The result of mapping a position through a step map, carrying
/// information about whether the position was deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapResult {
    /// The mapped position
    pub pos: usize,
    del_info: u8,
    /// Recovery value for mirror-based position recovery
    pub recover: Option<usize>,
}

impl MapResult {
    /// Whether the position was deleted (from the side indicated by assoc)
    pub fn deleted(&self) -> bool {
        (self.del_info & DEL_SIDE) > 0
    }

    /// Whether the position was deleted before
    pub fn deleted_before(&self) -> bool {
        (self.del_info & (DEL_BEFORE | DEL_ACROSS)) > 0
    }

    /// Whether the position was deleted after
    pub fn deleted_after(&self) -> bool {
        (self.del_info & (DEL_AFTER | DEL_ACROSS)) > 0
    }

    /// Whether the position was deleted across
    pub fn deleted_across(&self) -> bool {
        (self.del_info & DEL_ACROSS) > 0
    }
}

/// A mapping that can map a position through it
pub trait Mappable {
    /// Map a position, returning just the new position
    fn map(&self, pos: usize, assoc: i32) -> usize;

    /// Map a position, returning a `MapResult` with deletion info
    fn map_result(&self, pos: usize, assoc: i32) -> MapResult;
}

/// A flat array of `[start, old_size, new_size, ...]` triples representing
/// insertions and deletions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepMap {
    /// The range triples: `[start, oldSize, newSize, ...]`
    pub ranges: Vec<usize>,
    /// Whether this map is inverted
    #[serde(default)]
    pub inverted: bool,
}

impl StepMap {
    /// An empty step map (no position changes)
    pub const EMPTY: StepMap = StepMap {
        ranges: Vec::new(),
        inverted: false,
    };

    /// Create a new step map with the given ranges.
    ///
    /// Each triple in `ranges` is `[position, old_size, new_size]`.
    /// Positions before `position` are unchanged; positions inside the replaced
    /// span land at one of the two ends depending on the `assoc` bias.
    ///
    /// # Example
    ///
    /// ```
    /// use prosemirror::transform::{Mappable, StepMap};
    ///
    /// // Insert 4 characters at position 2 (old_size = 0, new_size = 4)
    /// let map = StepMap::new(vec![2, 0, 4]);
    ///
    /// assert_eq!(map.map(0,  1),  0); // before the insertion: unchanged
    /// assert_eq!(map.map(2, -1),  2); // at the gap, bias left: stays before new content
    /// assert_eq!(map.map(2,  1),  6); // at the gap, bias right: moves after new content
    /// assert_eq!(map.map(3,  1),  7); // after the insertion: shifted by 4
    ///
    /// // A deletion: remove 3 characters starting at position 2
    /// let del = StepMap::new(vec![2, 3, 0]);
    /// assert_eq!(del.map(0,  1), 0); // before: unchanged
    /// assert_eq!(del.map(5,  1), 2); // inside the deleted range: collapses to start
    /// assert_eq!(del.map(8,  1), 5); // after: shifted by -3
    /// ```
    pub fn new(ranges: Vec<usize>) -> Self {
        StepMap {
            ranges,
            inverted: false,
        }
    }

    /// Create an empty step map
    pub fn empty() -> Self {
        Self::EMPTY
    }

    /// Recover a position from a packed recovery value.
    ///
    /// Recovery values are expected to come from this map's own [`MapResult`]
    /// values or from a mirrored map with the same range shape. Values with an
    /// invalid range index, an impossible offset, malformed ranges, or checked
    /// arithmetic overflow return `None` rather than indexing outside the map's
    /// range triples.
    pub fn recover(&self, value: usize) -> Option<usize> {
        if !self.ranges.len().is_multiple_of(3) {
            return None;
        }

        let index = recover_index(value);
        let range_start = index.checked_mul(3)?;
        let range_end = range_start.checked_add(3)?;
        let range = self.ranges.get(range_start..range_end)?;
        let offset = recover_offset(value);
        if offset > range[1].max(range[2]) {
            return None;
        }

        let mut diff: isize = 0;
        if !self.inverted {
            for range in self.ranges[..range_start].chunks_exact(3) {
                let old_size = isize::try_from(range[1]).ok()?;
                let new_size = isize::try_from(range[2]).ok()?;
                diff = diff.checked_add(new_size.checked_sub(old_size)?)?;
            }
        }
        let start = isize::try_from(range[0]).ok()?;
        let offset = isize::try_from(offset).ok()?;
        let recovered = start.checked_add(diff)?.checked_add(offset)?;
        usize::try_from(recovered).ok()
    }

    /// Test whether this map touches a given position at the given recovery index
    pub fn touches(&self, pos: usize, recover: usize) -> bool {
        let mut diff: isize = 0;
        let index = recover_index(recover);
        let old_index = if self.inverted { 2 } else { 1 };
        let new_index = if self.inverted { 1 } else { 2 };
        let mut i = 0;
        while i < self.ranges.len() {
            let start = self.ranges[i] as isize - if self.inverted { diff } else { 0 };
            if start > pos as isize {
                break;
            }
            let old_size = self.ranges[i + old_index] as isize;
            let end = start + old_size;
            if pos as isize <= end && i == index * 3 {
                return true;
            }
            diff += self.ranges[i + new_index] as isize - old_size;
            i += 3;
        }
        false
    }

    /// Iterate over the old/new range pairs, calling `f(old_start, old_end, new_start, new_end)`
    pub fn for_each<F: FnMut(usize, usize, usize, usize)>(&self, mut f: F) {
        let old_index = if self.inverted { 2 } else { 1 };
        let new_index = if self.inverted { 1 } else { 2 };
        let mut i = 0;
        let mut diff: isize = 0;
        while i < self.ranges.len() {
            let start = self.ranges[i] as isize;
            let old_start = start - if self.inverted { diff } else { 0 };
            let new_start = start + if self.inverted { 0 } else { diff };
            let old_size = self.ranges[i + old_index] as isize;
            let new_size = self.ranges[i + new_index] as isize;
            f(
                old_start as usize,
                (old_start + old_size) as usize,
                new_start as usize,
                (new_start + new_size) as usize,
            );
            diff += new_size - old_size;
            i += 3;
        }
    }

    /// Return the inverse of this map
    pub fn invert(&self) -> StepMap {
        StepMap {
            ranges: self.ranges.clone(),
            inverted: !self.inverted,
        }
    }

    /// Create a simple offset map
    pub fn offset(n: isize) -> StepMap {
        if n == 0 {
            return StepMap::EMPTY;
        }
        if n < 0 {
            StepMap::new(vec![0, (-n) as usize, 0])
        } else {
            StepMap::new(vec![0, 0, n as usize])
        }
    }
}

impl Default for StepMap {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Mappable for StepMap {
    fn map(&self, pos: usize, assoc: i32) -> usize {
        self.map_result_impl(pos, assoc, true).0
    }

    fn map_result(&self, pos: usize, assoc: i32) -> MapResult {
        self.map_result_impl(pos, assoc, false).1.unwrap()
    }
}

impl StepMap {
    /// Internal map implementation that returns either a simple position or a full MapResult
    fn map_result_impl(&self, pos: usize, assoc: i32, simple: bool) -> (usize, Option<MapResult>) {
        let mut diff: isize = 0;
        let old_index = if self.inverted { 2 } else { 1 };
        let new_index = if self.inverted { 1 } else { 2 };
        let mut i = 0;
        while i < self.ranges.len() {
            let start = self.ranges[i] as isize - if self.inverted { diff } else { 0 };
            if start > pos as isize {
                break;
            }
            let old_size = self.ranges[i + old_index] as isize;
            let new_size = self.ranges[i + new_index] as isize;
            let end = start + old_size;
            if pos as isize <= end {
                let side = if old_size == 0 {
                    assoc
                } else if pos as isize == start {
                    -1
                } else if pos as isize == end {
                    1
                } else {
                    assoc
                };
                let result = (start + diff + if side < 0 { 0 } else { new_size }) as usize;
                if simple {
                    return (result, None);
                }
                let recover = if pos == (if assoc < 0 { start } else { end }) as usize {
                    None
                } else {
                    Some(make_recover(i / 3, pos as isize - start))
                };
                let del_info = if pos as isize == start {
                    DEL_AFTER
                } else if pos as isize == end {
                    DEL_BEFORE
                } else {
                    DEL_ACROSS
                };
                let del_info = if (assoc < 0 && pos as isize != start)
                    || (assoc >= 0 && pos as isize != end)
                {
                    del_info | DEL_SIDE
                } else {
                    del_info
                };
                return (
                    result,
                    Some(MapResult {
                        pos: result,
                        del_info,
                        recover,
                    }),
                );
            }
            diff += new_size - old_size;
            i += 3;
        }
        let result = (pos as isize + diff) as usize;
        if simple {
            (result, None)
        } else {
            (
                result,
                Some(MapResult {
                    pos: result,
                    del_info: 0,
                    recover: None,
                }),
            )
        }
    }
}

fn make_recover(index: usize, offset: isize) -> usize {
    (index as isize + offset * 65536) as usize
}

fn recover_index(value: usize) -> usize {
    value & 0xFFFF
}

fn recover_offset(value: usize) -> usize {
    (value - (value & 0xFFFF)) / 65536
}

/// A pipeline of [`StepMap`]s with optional mirror-pair tracking for rebasing.
///
/// Maps are applied left-to-right: position `p` is first mapped through
/// `maps[0]`, then through `maps[1]`, and so on.
///
/// # Example
///
/// ```
/// use prosemirror::transform::{Mappable, Mapping, StepMap};
///
/// let mut mapping = Mapping::new();
/// // Step 1: delete 3 characters at position 2
/// mapping.append_map(StepMap::new(vec![2, 3, 0]), None);
/// // Step 2: insert 1 character at position 2 (in the already-updated document)
/// mapping.append_map(StepMap::new(vec![2, 0, 1]), None);
///
/// assert_eq!(mapping.map(0,  1), 0); // before the affected range: unchanged
/// assert_eq!(mapping.map(10, 1), 8); // net shift is −3 + 1 = −2
///
/// // The inverse mapping undoes both steps in reverse order
/// let inv = mapping.invert();
/// assert_eq!(inv.map(8, 1), 10);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Mapping {
    /// The individual step maps in this mapping
    pub maps: Vec<StepMap>,
    mirror: Option<Vec<usize>>,
    from: usize,
    /// The end index (exclusive) of the active range of maps
    pub to: usize,
}

impl Mapping {
    /// Create a new empty mapping
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a sub-mapping view
    pub fn slice(&self, from: usize, to: Option<usize>) -> Mapping {
        Mapping {
            maps: self.maps.clone(),
            mirror: self.mirror.clone(),
            from,
            to: to.unwrap_or(self.maps.len()),
        }
    }

    /// Push a new step map, optionally recording a mirror pair
    pub fn append_map(&mut self, map: StepMap, mirrors: Option<usize>) {
        self.maps.push(map);
        self.to = self.maps.len();
        if let Some(m) = mirrors {
            self.set_mirror(self.maps.len() - 1, m);
        }
    }

    /// Append another mapping
    pub fn append_mapping(&mut self, mapping: &Mapping) {
        let mut i = 0;
        let start_size = self.maps.len();
        while i < mapping.maps.len() {
            let mirr = mapping.get_mirror(i);
            self.append_map(
                mapping.maps[i].clone(),
                mirr.filter(|&m| m < i).map(|m| start_size + m),
            );
            i += 1;
        }
    }

    /// Find the mirror of step `n`
    pub fn get_mirror(&self, n: usize) -> Option<usize> {
        if let Some(ref mirror) = self.mirror {
            let mut i = 0;
            while i + 1 < mirror.len() {
                if mirror[i] == n {
                    return Some(mirror[i + 1]);
                }
                if mirror[i + 1] == n {
                    return Some(mirror[i]);
                }
                i += 2;
            }
        }
        None
    }

    /// Record a mirror pair
    pub fn set_mirror(&mut self, n: usize, m: usize) {
        self.mirror
            .get_or_insert_with(Vec::new)
            .extend_from_slice(&[n, m]);
    }

    /// Append the inverse of another mapping in reverse order
    pub fn append_mapping_inverted(&mut self, mapping: &Mapping) {
        let mut i = mapping.maps.len();
        let total_size = self.maps.len() + mapping.maps.len();
        while i > 0 {
            i -= 1;
            let mirr = mapping.get_mirror(i);
            self.append_map(
                mapping.maps[i].invert(),
                mirr.filter(|&m| m > i).map(|m| total_size - m - 1),
            );
        }
    }

    /// Return the inverse of this mapping
    pub fn invert(&self) -> Mapping {
        let mut inverse = Mapping::new();
        inverse.append_mapping_inverted(self);
        inverse
    }
}

impl Mappable for Mapping {
    fn map(&self, pos: usize, assoc: i32) -> usize {
        if self.mirror.is_some() {
            return self.map_impl(pos, assoc, true).0;
        }
        let mut pos = pos;
        for i in self.from..self.to {
            pos = self.maps[i].map(pos, assoc);
        }
        pos
    }

    fn map_result(&self, pos: usize, assoc: i32) -> MapResult {
        self.map_impl(pos, assoc, false).1.unwrap()
    }
}

impl Mapping {
    fn map_impl(&self, pos: usize, assoc: i32, simple: bool) -> (usize, Option<MapResult>) {
        let mut del_info: u8 = 0;
        let mut pos = pos;
        let mut i = self.from;
        while i < self.to {
            let map = &self.maps[i];
            let result = map.map_result(pos, assoc);
            if let Some(recover) = result.recover {
                if let Some(corr) = self.get_mirror(i) {
                    if corr > i && corr < self.to {
                        if let Some(recovered) = self.maps[corr].recover(recover) {
                            i = corr;
                            pos = recovered;
                            i += 1;
                            continue;
                        }
                    }
                }
            }
            del_info |= result.del_info;
            pos = result.pos;
            i += 1;
        }
        if simple {
            (pos, None)
        } else {
            (
                pos,
                Some(MapResult {
                    pos,
                    del_info,
                    recover: None,
                }),
            )
        }
    }
}
