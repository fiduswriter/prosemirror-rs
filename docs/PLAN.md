# Binding deduplication & full prosemirror-model/prosemirror-transform API exposure

This document tracks the in-progress work to (1) deduplicate the Node.js and
Python FFI binding code into a common `src/binding/` layer that future
bindings (e.g. WASM) can also use, and (2) expose every public-API item
documented at <https://prosemirror.net/docs/ref/> in **both** the Node and
Python bindings.

## Status as of this commit

### ✅ Done

- New module `src/binding/` (registered in `src/lib.rs` as `pub mod binding`)
  containing language-neutral wrappers around the dynamic model & transform
  types.  Both files compile cleanly (warnings for missing docs only).
- `src/binding/model.rs` — `BNodeType`, `BMarkType`, `BMark`, `BFragment`,
  `BSlice`, `BNode`, `BResolvedPos`, `BNodeRange`, `BContentMatch`.
- `src/binding/transform.rs` — `BStepMap`, `BMapResult`, `BMapping`, `BStep`,
  `BStepResult`, `BTransform`, plus the free helpers `b_lift_target`,
  `b_find_wrapping`, `b_can_split`, `b_can_join`, `b_join_point`,
  `b_insert_point`, `b_drop_point`.
- **Phase 1 complete**: Added `edge_count`, `edge`, `find_wrapping`,
  `default_type` to `ParsedContentMatch` in `src/dynamic/types.rs`, plus a
  `ParsedContentMatch::from_dynamic` converter.  The following previously-
  blocked methods are now active:
  - `BNodeType::content_match()`
  - `BNode::content_match_at()`
  - `BContentMatch::default_type()`
  - `BContentMatch::find_wrapping(target)`
  - `BContentMatch::edge_count()`
  - `BContentMatch::edge(n)`
- `src/dynamic/schema.rs`: `content_exprs` field promoted to `pub(crate)` to
  support the no-thread-local `find_wrapping` BFS in `ParsedContentMatch`.
- All existing tests still pass:
  - 58 Rust unit tests
  - 444 Node.js upstream tests
  - 462 Python upstream tests

## 🚧 Phase 2 — Wire bindings through the binding layer

Neither `node/src/{model,transform}.rs` nor `python/src/{model,transform}.rs`
has been updated yet.  The plan is that each FFI struct becomes a thin
newtype around the corresponding `B*` struct, with the FFI methods doing
only:

- attribute-macro decoration (`#[napi]`, `#[pyclass]`, `#[pymethods]` …)
- input deserialization (`Value` vs `Bound<PyAny>`)
- error-type translation (`napi::Error` vs `PyValueError`)
- output wrapping (build `Node_`/`PyNode` from `BNode`)

**Example mechanical transformation:**

```rust
// Before (node/src/model.rs):
#[napi]
pub struct Node_ {
    pub(crate) schema: Arc<DynamicSchema>,
    pub(crate) inner: DynamicNode,
}

#[napi]
impl Node_ {
    #[napi(getter)]
    pub fn is_block(&self) -> bool {
        self.schema.with_types(|| self.inner.is_block())
    }
}

// After:
#[napi]
pub struct Node_ {
    pub(crate) inner: prosemirror::binding::model::BNode,
}

#[napi]
impl Node_ {
    #[napi(getter)]
    pub fn is_block(&self) -> bool { self.inner.is_block() }
}
```

Suggested execution order:

1. Land the small core change above so the commented methods come back.
2. Rewrite `node/src/model.rs` to use `BNode`/`BNodeType`/… internally.
3. Rewrite `node/src/transform.rs` to use `BTransform`/`BStep`/….
4. Rewrite `python/src/model.rs` (preserving Python-only extras: `__str__`,
   `__repr__`, `__richcmp__`, `__getattr__`, `PyMarkSet`, the
   `SCHEMA_RAW_SPECS` registry, `strip_callables`, etc.).
5. Rewrite `python/src/transform.rs`.

Run the upstream test suites after each step to catch regressions.

## 🚧 Phase 3 — Add the missing public-API items

Audit from <https://prosemirror.net/docs/ref/>.  Items marked ❌ are absent
from one or both bindings today; the binding layer (when wired up per
Phase 2) already exposes most of them.

### Node (prosemirror-model)

| Member | Node binding | Python binding | Binding layer ready? |
|---|---|---|---|
| `isAtom` | ❌ | ❌ | ✅ `BNode::is_atom` |
| `isInline` | ✅ | ❌ | ✅ `BNode::is_inline` |
| `isTextblock` | ✅ | ❌ | ✅ `BNode::is_textblock` |
| `inlineContent` | ❌ | ❌ | ✅ `BNode::inline_content` |
| `textBetween(from,to,blockSep?,leafText?)` | ❌ | partial | ✅ `BNode::text_between` |
| `maybeChild(i)` | ❌ | ✅ | ✅ `BNode::maybe_child` |
| `forEach(f)` | ❌ | ❌ | ✅ `BNode::for_each` |
| `nodesBetween(from,to,f,startPos?)` | ❌ | ❌ | ✅ `BNode::nodes_between` |
| `descendants(f)` | ❌ | ❌ | ✅ `BNode::descendants` |
| `rangeHasMark(from,to,type)` | ❌ | ❌ | ✅ `BNode::range_has_mark` |
| `sameMarkup(other)` | ❌ | ❌ | ✅ `BNode::same_markup` |
| `hasMarkup(type,attrs?,marks?)` | ❌ | ❌ | ❌ (needs core impl) |
| `canAppend(other)` | ❌ | ❌ | ✅ `BNode::can_append` |
| `canReplace(...)` | ❌ | ❌ | ❌ (needs API design) |
| `canReplaceWith(...)` | ❌ | ❌ | ❌ |
| `contentMatchAt(i)` | ❌ | ❌ | ✅ `BNode::content_match_at` |
| `childAfter/childBefore(pos)` | ❌ | ❌ | ❌ (returns 3-tuple struct) |
| `children` array | ❌ | ❌ | derivable via `child_count` + loop |
| `Node.fromJSON` static | ✅ | ✅ | ✅ |

### Fragment

| Member | Node binding | Python binding | Binding layer |
|---|---|---|---|
| `firstChild` / `lastChild` | ❌ | ❌ | ✅ |
| `maybeChild(i)` | ❌ | ✅ | ✅ |
| `replaceChild(i,node)` | ❌ | ❌ | ✅ |
| `addToStart(node)` / `addToEnd(node)` | ❌ | ❌ | ✅ |
| `textBetween(...)` | ❌ | ❌ | ✅ |
| `forEach(f)` | ❌ | ❌ | ✅ |
| `nodesBetween(...)` | ❌ | ❌ | ✅ |
| `descendants(f)` | ❌ | ❌ | ❌ (add to BFragment) |
| `Fragment.fromJSON` static | ❌ | ❌ | ✅ `BFragment::from_json` |
| `Fragment.empty` static | ❌ | ✅ | ✅ `BFragment::empty` |
| `Fragment.from(nodes?)` | partial | ❌ | ❌ (needs polymorphic input) |

### Slice

| Member | Node binding | Python binding | Binding layer |
|---|---|---|---|
| `size` | ❌ | ❌ | ✅ `BSlice::size` |
| `Slice.empty` static | ❌ | ❌ | ✅ |
| `Slice.fromJSON` static | ❌ | ❌ | ✅ |
| `Slice.maxOpen(frag,openIsolating?)` | ❌ | ❌ | ✅ |

### ResolvedPos

| Member | Node binding | Python binding | Binding layer |
|---|---|---|---|
| `parent` | ✅ | ❌ | ✅ |
| `doc` | ❌ | ❌ | ✅ `BResolvedPos::doc_node` |
| `textOffset` | ❌ | ❌ | ✅ |
| `index(depth?)` | ❌ | ❌ | ✅ |
| `indexAfter(depth?)` | ❌ | ❌ | ✅ |
| `sharedDepth(pos)` | ❌ | ❌ | ✅ |
| `marksAcross($end)` | ❌ | ❌ | ✅ |
| `sameParent(other)` | ❌ | ❌ | ✅ |
| `max(other)` / `min(other)` | ❌ | ❌ | ✅ |

### NodeRange

| Member | Node | Python | Binding layer |
|---|---|---|---|
| `$from` (`from`) / `$to` (`to`) | ✅ | ❌ | ✅ |
| `parent` | ✅ | ❌ | ✅ |
| `startIndex` / `endIndex` | ✅ | ❌ | ✅ |

### NodeType

| Member | Node | Python | Binding layer |
|---|---|---|---|
| `spec` | ❌ | ❌ | ❌ (needs NodeSpec exposure) |
| `attrs` (defaults map) | ❌ | ❌ | ❌ |
| `markSet` | ❌ | ❌ | ❌ |
| `isText` | ❌ | ❌ | ✅ `BNodeType::is_text` |
| `isInline` | ✅ | ✅ | ✅ |
| `whitespace` | ❌ | ❌ | ✅ |
| `isCode` | ❌ | ❌ | ✅ |
| `isInGroup(name)` | ❌ | ❌ | ❌ |
| `contentMatch` | ❌ | ❌ | ✅ `BNodeType::content_match` |
| `hasRequiredAttrs` | ❌ | ❌ | ✅ |
| `compatibleContent(other)` | ❌ | ❌ | ✅ |
| `createChecked(...)` | ✅ | ❌ | ✅ |
| `allowsMarks(marks)` | ❌ | ❌ | ✅ `BNodeType::allows_marks` |
| `allowedMarks(marks)` | ❌ | ❌ | ❌ |

### MarkType

| Member | Node | Python | Binding layer |
|---|---|---|---|
| `spec` | ❌ | ❌ | ❌ |
| `removeFromSet(set)` | ❌ | ❌ | ✅ |
| `isInSet(set)` | ❌ | ❌ | ✅ |
| `excludes(other)` | ❌ | ❌ | ✅ |

### Mark

| Member | Node | Python | Binding layer |
|---|---|---|---|
| `toJSON()` | ❌ | ❌ | ✅ `BMark::to_json` |
| `Mark.fromJSON` (via schema) | ✅ | ✅ | n/a |
| `Mark.sameSet(a,b)` | ❌ | ❌ | ❌ (add) |
| `Mark.setFrom(marks?)` | ❌ | ❌ | ❌ |
| `Mark.none` | ❌ | ❌ | ❌ |

### ContentMatch

All four (`defaultType`, `findWrapping`, `edgeCount`, `edge`) are now
implemented in the binding layer (`BContentMatch`).  The Node and Python
FFI wrappers still need to be wired up (Phase 2).

### Schema

| Member | Node | Python | Binding layer |
|---|---|---|---|
| `spec` | ❌ | ✅ | ❌ |
| `topNodeType` | ❌ | ❌ | ❌ |
| `linebreakReplacement` | ❌ | ❌ | ❌ |

### prosemirror-transform

| Member | Node | Python | Binding layer |
|---|---|---|---|
| `StepMap.forEach(f)` | ❌ | ❌ | ✅ `BStepMap::for_each` |
| `StepMap.offset(n)` (static) | ❌ | ❌ | ✅ |
| `StepMap.empty` (static) | ❌ | ❌ | ✅ |
| `MapResult.recover` field | ❌ | ❌ | ✅ |
| `new Mapping(maps?,…)` | ❌ ctor | partial | ❌ (add) |
| `Mapping.from` / `to` properties | ❌/❌ | ❌/❌ | partial (`to_end`) |
| `Mapping.appendMapping(m)` | ❌ | ❌ | ✅ |
| `Mapping.appendMappingInverted(m)` | ❌ | ❌ | ✅ |
| `Mapping.copy()` | ❌ | ❌ | ❌ (add) |
| `Step.replaceAround(...)` (static) | ✅ | ❌ | ✅ `BStep::make_replace_around` |
| `Step.addNodeMark/removeNodeMark/attr/docAttr` static | ✅ | ❌ | ✅ |
| `Transform.clearIncompatible(...)` | ❌ | ❌ | ❌ (needs core impl) |

## 🚧 Phase 4 — Tests for every new method

Once Phases 1–3 land, add regression tests:

- **Node**: extend `node/tests/test-editor.cjs` (or a new file) with a
  suite that constructs a schema/doc and exercises each newly-exposed
  getter/method.
- **Python**: extend `python/tests/` with a parallel test module.
- Both sets should test the same scenarios on the same fixtures so that
  cross-binding parity stays enforced.

## Suggested commit boundaries

1. **Done** — skeleton binding layer + plan doc.
2. **Done** — Core: added `ParsedContentMatch::{edge,edge_count,find_wrapping,default_type}`
   and `from_dynamic` converter; uncommented the four previously-blocked
   binding methods.
3. Node binding rewrite (`node/src/model.rs` first, then `transform.rs`).
4. Python binding rewrite (same order).
5. Add missing-method tests per binding.
6. README updates if any developer-facing API changes need flagging.
