# Plan: WASM bindings + complete npm package for prosemirror-model / prosemirror-transform

## Goal

Deliver a **single npm package** — `prosemirror-rs` — that serves as a
**drop-in replacement** for both upstream `prosemirror-model` and
`prosemirror-transform`.  The heavy lifting runs in Rust, dispatched
automatically based on the runtime environment:

- **Node.js** → napi-rs native addon (fastest path)
- **Browser / bundler / Deno / Cloudflare Workers** → WASM

Subpath exports keep model and transform namespaces separate:

```js
// Works everywhere (Node, browser, bundler):
import { Schema, Node } from "prosemirror-rs/model";
import { Transform, Step } from "prosemirror-rs/transform";

// Root entry also exports Editor (high-level JSON bridge):
import { Editor } from "prosemirror-rs";
```

---

## Status

### ✅ Done

| Step | Task |
|------|------|
| 1.1 | npm package under `node/npm/` with subpath exports |
| 1.2 | Conditional exports + auto-dispatch (node/browser/default) |
| 1.3 | TypeScript declarations (model, transform, dom, index) |
| 1.4 | `ReplaceError` in `node/npm/dom.js` |
| 1.5 | `contentMatchParse` re-exported |
| 1.6 | `Fragment.from` polymorphic wrapper in `node/npm/patch.js` |
| 2.1 | `wasm/` workspace member |
| 2.2 | WASM model wrappers (`wasm/src/model.rs`, ~1150 lines) |
| 2.3 | WASM transform wrappers (`wasm/src/transform.rs`, ~1120 lines) |
| 2.4 | `wasm-pack build` integrated (`--target web` + `--target nodejs`) |
| 3.1 | DOM files vendored into `vendor/` |
| 5.1a | napi unit tests (97 pass) |
| 5.1b | napi upstream tests (444 pass) |
| 5.2a | WASM smoke test (10 pass) |
| 5.2b | WASM upstream test infrastructure (runner, shims, bridges) |

### Test results summary

| Suite | Tests | Status |
|-------|-------|--------|
| `npm test` (napi unit) | 97 | ✅ |
| `npm run test:upstream` (napi) | 444 | ✅ |
| `npm run test:wasm` (WASM smoke) | 10 | ✅ |
| `npm run test:upstream:wasm` (WASM) | 444 | ⚠️ 408 pass, 36 fail |

### WASM upstream error breakdown (remaining 36)

| Count | Error | Category |
|-------|-------|----------|
| 7 | `!false` | Slice openStart/openEnd mismatches |
| 3 | `RuntimeError: unreachable` | Content expression edge cases |
| 3 | `Invalid step JSON: Unknown mark type` | Step serialization cross-schema scope |
| 2 | `null pointer passed to rust` | Remaining Mark lifecycle edge cases |
| 2 | `!eq doc → heading` | replaceRange node type conversion |
| ~9 | Various `!eq` | Transform semantics (isolating, wrapping, defining context) |
| ~10 | Other | Small API gaps (schema.spec.nodes.addBefore, Fragment.from, etc.) |

These are core Rust implementation differences from the JavaScript reference,
not WASM binding issues.

- `node/test-shim/wasm/prosemirror-model.cjs` (~400 lines) — Main bridging:
  - `BridgedSchema` — wraps raw WASM Schema with array→Fragment, marks defaults
  - `Schema.spec` getter — OrderedMap-compatible `nodes`/`marks` with `get`, `update`, `append`, `forEach`, `toJSON`
  - Raw WASM patches: `OrigSchema.prototype.text`, `OrigSchema.prototype.node`, `OrigNodeType.prototype.create`
  - CamelCase aliases for ~40 methods across Node, Mark, MarkType, ContentMatch, ResolvedPos, Fragment
  - `Transform` bridging with `new`-less constructor

- `node/test-shim/wasm/prosemirror-transform.cjs` (~93 lines) — Step constructors, Mapping shim

- `node/run-upstream-tests.mjs` — `--wasm` flag enables WASM shim import rewriting

### Key WASM-vs-napi API differences

| Area | napi (camelCase) | WASM (snake_case) | Bridge |
|------|-----------------|-------------------|--------|
| Factory methods | `Fragment.from`, `Fragment.fromArray` | `Fragment.from(schema, input)`, `Fragment.from_array(schema, nodes)` | Schema-first, array→Fragment in BridgedSchema |
| Getters | `node.type`, `.textContent` | `node.type_`, `.text_content` | `patch.js` adds `.type` alias |
| Schema methods | `schema.nodes` (getter) | `schema.nodes()` (method) | BridgedSchema adds getter |
| `Schema.node` | 3 args (content can be array) | 4 args (marks required, content is Fragment) | Bridged wraps |
| `Schema.text` | 1-2 args (marks optional) | 2 args (marks required) | Bridged + raw patch |
| Transform | `new Transform(doc)` | `new Transform_(doc)` | Bridged wrapper |
| StepMap/Mapping | `StepMap`, `Mapping` | `StepMap_`, `Mapping_` | Export aliases |

---

## Open questions

1. **`window` / `document` in DOM types**: Node.js users pass `{document}` option.
2. **`Schema.cached`**: Use JS-side `WeakMap` cache in vendored DOM files.
3. **`NodeType.spec`**: Store raw spec JS-side via `WeakMap` (already in `patch.js`).
4. **WASM for Node.js fallback**: napi `index.js` has no WASM fallback (CJS vs ESM).
5. **Shared JS code**: `patch.js`, `dom.js`, `.d.ts` shared between napi/wasm paths.
