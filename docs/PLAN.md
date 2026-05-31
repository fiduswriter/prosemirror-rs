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

---

## Missing public-API items

All previously identified missing items have been implemented. No known gaps remain.

---

## Implemented

### Node (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `hasMarkup(type, attrs?, marks?)` | ✅ | ✅ | |
| `canReplace(from, to, content, start?, end?)` | ✅ | ✅ | |
| `canReplaceWith(from, to, type, attrs?)` | ✅ | ✅ | |
| `childAfter(pos)` / `childBefore(pos)` | ✅ | ✅ | returns `{node, index, offset}` dict/object |

### Fragment (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `Fragment.from(nodes?)` | ✅ | ✅ | polymorphic: `null \| Node \| Node[] \| Fragment` |

### NodeRange (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `$from` / `$to` | ✅ | ✅ | Python: `from_` / `to_` getters |

### NodeType (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `spec` | ✅ | ✅ | |
| `attrs` (defaults map) | ✅ | ✅ | |
| `markSet` | ✅ | ✅ | |
| `isInGroup(name)` | ✅ | ✅ | |
| `allowedMarks(marks)` | ✅ | ✅ | |

### MarkType / Mark / Schema (`prosemirror-model`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `MarkType.spec` | ✅ | ✅ | |
| `Mark.sameSet(a, b)` | ✅ | ✅ | static method |
| `Mark.setFrom(marks?)` | ✅ | ✅ | static method |
| `Mark.none` | ✅ | ✅ | Node.js: `markNone()` free fn · Python: `Mark.none` classattr |
| `Schema.topNodeType` | ✅ | ✅ | |

### Transform (`prosemirror-transform`)

| Member | Node.js | Python | Notes |
|---|---|---|---|
| `StepMap.forEach(f)` | ✅ | ✅ | callback `(oldStart, oldEnd, newStart, newEnd)` |
| `new Mapping(maps?)` | ✅ | ✅ | optional `StepMap[]` / list arg |
| `Mapping.appendMapping(m)` | ✅ | ✅ | |
| `Mapping.appendMappingInverted(m)` | ✅ | ✅ | |
| `Mapping.copy()` | ✅ | ✅ | |
| `Transform.clearIncompatible(pos, type, clearNewlines)` | ✅ | ✅ | |

---

## Code-quality / deduplication

### ~~Wire Python structs through the `B*` layer~~ ✅ Done

`PyNode`, `PyNodeType`, `PyMarkType`, `PyMark`, `PyFragment`, `PySlice`,
`PyResolvedPos`, `PyNodeRange`, and `PyContentMatch` now all hold
`inner: BXxx` as their only data field.  Every method body delegates to the
corresponding `B*` method.
