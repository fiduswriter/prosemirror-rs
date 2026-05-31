# Outstanding work: full prosemirror-model/prosemirror-transform API exposure

This document lists the remaining gaps between what the Node.js and Python
bindings currently expose and the full public API documented at
<https://prosemirror.net/docs/ref/>.

## Known limitations (not just missing — need design work)

### `nodesBetween` / `descendants` early-termination

The bindings use a collect-first approach: all nodes are gathered into a `Vec`
before the Python/JS callback is invoked.  This means returning `false` from
the callback to stop traversal has no effect.  Fixing this requires calling the
Python/JS callback inline from within the Rust traversal, which needs careful
handling of the GIL / napi env lifetimes.

### Python `PyNode` / `PyFragment` early-termination

Same limitation as above specifically for Python's `nodes_between` and
`descendants`.

---

## Missing public-API items

### Node (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `hasMarkup(type, attrs?, marks?)` | ❌ | ❌ | needs core impl |
| `canReplace(from, to, content, start?, end?)` | ❌ | ❌ | needs API design |
| `canReplaceWith(from, to, type, attrs?)` | ❌ | ❌ | |
| `childAfter(pos)` / `childBefore(pos)` | ❌ | ❌ | returns `{node, index, offset}` struct |

### Fragment (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `Fragment.from(nodes?)` | partial | ❌ | needs polymorphic input (node, array, fragment, null) |

### NodeRange (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `$from` / `$to` | ✅ | ❌ | Python exposes `start`/`end` positions only, not the resolved-pos objects |

### NodeType (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `spec` | ❌ | ❌ | requires exposing `NodeSpec` as a structured object |
| `attrs` (defaults map) | ❌ | ❌ | |
| `markSet` | ❌ | ❌ | |
| `isInGroup(name)` | ❌ | ❌ | |
| `allowedMarks(marks)` | ❌ | ❌ | |

### MarkType / Mark / Schema (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `MarkType.spec` | ❌ | ❌ | |
| `Mark.sameSet(a, b)` | ❌ | ❌ | static method |
| `Mark.setFrom(marks?)` | ❌ | ❌ | static method |
| `Mark.none` | ❌ | ❌ | static empty-array constant |
| `Schema.spec` | ❌ | ✅ | |
| `Schema.topNodeType` | ❌ | ❌ | |

### Transform (`prosemirror-transform`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `StepMap.forEach(f)` | ❌ | ❌ | |
| `new Mapping(maps?)` | ❌ ctor | partial | |
| `Mapping.appendMapping(m)` | ❌ | ❌ | |
| `Mapping.appendMappingInverted(m)` | ❌ | ❌ | |
| `Mapping.copy()` | ❌ | ❌ | |
| `Transform.clearIncompatible(pos, type, attrs?)` | ❌ | ❌ | needs core impl |

---

## Code-quality / deduplication

### ~~Wire Python structs through the `B*` layer~~ ✅ Done

`PyNode`, `PyNodeType`, `PyMarkType`, `PyMark`, `PyFragment`, `PySlice`,
`PyResolvedPos`, `PyNodeRange`, and `PyContentMatch` now all hold
`inner: BXxx` as their only data field.  Every method body delegates to the
corresponding `B*` method; `python/src/transform.rs` has been updated to use
the new `x.inner.schema` / `x.inner.inner` paths throughout.  All 513 Python
tests and 79 Node.js binding tests continue to pass.
