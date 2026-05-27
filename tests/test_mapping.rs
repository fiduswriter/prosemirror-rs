//! Tests ported from prosemirror-transform/test/test-mapping.ts
//!
//! Verifies StepMap, Mapping, and MapResult behavior.

use prosemirror::transform::{Mappable, Mapping, StepMap};

type MirrorMap = Vec<(usize, usize)>;
type TestMap = (Vec<usize>, Option<MirrorMap>);

fn mk(maps: Vec<TestMap>) -> Mapping {
    let mut mapping = Mapping::new();
    for (ranges, mirrors) in maps {
        mapping.append_map(StepMap::new(ranges), None);
        if let Some(m) = mirrors {
            for (from, to) in m {
                mapping.set_mirror(from, to);
            }
        }
    }
    mapping
}

fn test_mapping(mapping: &Mapping, cases: &[(usize, usize, i32, bool)]) {
    let inverted = mapping.invert();
    for &(from, to, bias, lossy) in cases {
        assert_eq!(
            mapping.map(from, bias),
            to,
            "map({from}, {bias}) should be {to}"
        );
        if !lossy {
            assert_eq!(
                inverted.map(to, bias),
                from,
                "inverse map({to}, {bias}) should be {from}"
            );
        }
    }
}

fn test_del(mapping: &Mapping, pos: usize, side: i32, flags: &str) {
    let r = mapping.map_result(pos, side);
    let mut found = String::new();
    if r.deleted() {
        found.push('d');
    }
    if r.deleted_before() {
        found.push('b');
    }
    if r.deleted_after() {
        found.push('a');
    }
    if r.deleted_across() {
        found.push('x');
    }
    assert_eq!(found, flags, "deletion flags at pos={pos} side={side}");
}

#[test]
fn map_through_single_insertion() {
    let m = mk(vec![(vec![2, 0, 4], None)]);
    test_mapping(
        &m,
        &[
            (0, 0, 1, false),
            (2, 6, 1, false),
            (2, 2, -1, false),
            (3, 7, 1, false),
        ],
    );
}

#[test]
fn map_through_single_deletion() {
    let m = mk(vec![(vec![2, 4, 0], None)]);
    test_mapping(
        &m,
        &[
            (0, 0, 1, false),
            (2, 2, -1, false),
            (3, 2, 1, true), // deleted
            (6, 2, 1, false),
            (6, 2, -1, true), // deleted
            (7, 3, 1, false),
        ],
    );
}

#[test]
fn map_through_single_replace() {
    let m = mk(vec![(vec![2, 4, 4], None)]);
    test_mapping(
        &m,
        &[
            (0, 0, 1, false),
            (2, 2, 1, false),
            (4, 6, 1, true),  // deleted
            (4, 2, -1, true), // deleted
            (6, 6, -1, false),
            (8, 8, 1, false),
        ],
    );
}

#[test]
fn delete_flags_before() {
    let m = mk(vec![(vec![0, 2, 0], None)]);
    test_del(&m, 2, -1, "db");
    test_del(&m, 2, 1, "b");

    let m2 = mk(vec![(vec![0, 2, 2], None)]);
    test_del(&m2, 2, -1, "db");

    let m3 = mk(vec![(vec![0, 1, 0], None), (vec![0, 1, 0], None)]);
    test_del(&m3, 2, -1, "db");
}

#[test]
fn delete_flags_after() {
    let m = mk(vec![(vec![2, 2, 0], None)]);
    test_del(&m, 2, -1, "a");
    test_del(&m, 2, 1, "da");

    let m2 = mk(vec![(vec![2, 2, 2], None)]);
    test_del(&m2, 2, 1, "da");
}

#[test]
fn delete_flags_across() {
    let m = mk(vec![(vec![0, 4, 0], None)]);
    test_del(&m, 2, -1, "dbax");
    test_del(&m, 2, 1, "dbax");
}

#[test]
fn mapping_invert_roundtrip() {
    let mapping = mk(vec![
        (vec![0, 0, 5], None),  // insert 5 at pos 0
        (vec![10, 3, 0], None), // delete 3 at pos 10 after the insertion
    ]);

    test_mapping(
        &mapping,
        &[
            (0, 5, 1, false),
            (4, 9, 1, false),
            (5, 10, 1, true),
            (9, 11, 1, false),
            (15, 17, 1, false),
        ],
    );
}

#[test]
fn mapping_get_mirror_is_bidirectional() {
    let mut mapping = Mapping::new();
    mapping.append_map(StepMap::new(vec![2, 4, 0]), None);
    mapping.append_map(StepMap::new(vec![2, 0, 4]), Some(0));

    assert_eq!(mapping.get_mirror(0), Some(1));
    assert_eq!(mapping.get_mirror(1), Some(0));
    assert_eq!(mapping.get_mirror(2), None);
}

#[test]
fn mirrored_mapping_recovers_deleted_positions() {
    let delete = StepMap::new(vec![2, 4, 0]);
    let mut mapping = Mapping::new();
    mapping.append_map(delete.clone(), None);
    mapping.append_map(delete.invert(), Some(0));

    assert_eq!(mapping.map(4, 1), 4);
    assert_eq!(mapping.map(4, -1), 4);
}

#[test]
fn stepmap_recover_restores_positions_inside_replaced_ranges() {
    let map = StepMap::new(vec![5, 3, 1]);
    let result = map.map_result(6, 1);
    let recover = result
        .recover
        .expect("position inside replaced range should have recovery token");

    assert_eq!(result.pos, 6);
    assert_eq!(map.recover(recover), Some(6));
}

#[test]
fn stepmap_recover_rejects_invalid_recovery_tokens() {
    let map = StepMap::new(vec![5, 3, 1]);

    // The low 16 bits encode the range index. This map only has range index 0,
    // so index 1 must be rejected instead of indexing past the single range.
    assert_eq!(map.recover(1), None);

    // The high bits encode the offset inside the replaced span. This replaced
    // span is only three units wide, so offset 4 is invalid.
    assert_eq!(map.recover(4 * 65_536), None);

    // Empty or malformed raw range vectors cannot recover any token.
    assert_eq!(StepMap::empty().recover(0), None);
    assert_eq!(StepMap::new(vec![5, 3]).recover(0), None);
}

#[test]
fn mapping_ignores_invalid_mirror_recovery_tokens() {
    let mut mapping = Mapping::new();
    mapping.append_map(StepMap::new(vec![2, 4, 0]), None);
    mapping.append_map(StepMap::empty(), Some(0));

    // The mirror pair points at a map that cannot recover the deletion token.
    // Mapping should fall back to normal mapping instead of panicking.
    assert_eq!(mapping.map(4, 1), 2);
}

#[test]
fn stepmap_offset() {
    let m = StepMap::offset(5);
    assert_eq!(m.map(0, 1), 5);
    assert_eq!(m.map(3, 1), 8);
    assert_eq!(m.map(10, 1), 15);

    let m2 = StepMap::offset(-3);
    assert_eq!(m2.map(5, 1), 2);
    assert_eq!(m2.map(10, 1), 7);
}

#[test]
fn stepmap_empty() {
    let m = StepMap::empty();
    assert_eq!(m.map(5, 1), 5);
    assert_eq!(m.map(0, 1), 0);
    assert_eq!(m.map(100, -1), 100);
}

#[test]
fn stepmap_for_each() {
    let m = StepMap::new(vec![2, 3, 1]); // replace 3 chars at pos 2 with 1 char
    let mut ranges = Vec::new();
    m.for_each(|old_from, old_to, new_from, new_to| {
        ranges.push((old_from, old_to, new_from, new_to));
    });
    assert_eq!(ranges, vec![(2, 5, 2, 3)]);
}

#[test]
fn stepmap_invert() {
    let m = StepMap::new(vec![2, 3, 1]);
    let inv = m.invert();
    // Original: replace pos 2..5 with 1 char
    // Inverted: should map back
    assert_eq!(m.map(0, 1), 0);
    assert_eq!(inv.map(0, 1), 0);
}
