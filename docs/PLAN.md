# Plan: WASM bindings + complete npm package for prosemirror-model / prosemirror-transform

## Goal

Deliver a **single npm package** — `prosemirror-rs` — that serves as a
**drop-in replacement** for both upstream `prosemirror-model` and
`prosemirror-transform`.  The heavy lifting runs in Rust, dispatched
automatically based on the runtime environment:

- **Node.js** → napi-rs native addon (fastest path)
- **Browser / bundler / Deno / Cloudflare Workers** → WASM

The package auto-detects the environment — consumers just `npm install
prosemirror-rs` and import normally.  Subpath exports keep model and
transform namespaces separate:

```js
// Works everywhere (Node, browser, bundler):
import { Schema, Node } from "prosemirror-rs/model";
import { Transform, Step } from "prosemirror-rs/transform";

// Root entry also exports Editor (high-level JSON bridge):
import { Editor } from "prosemirror-rs";
```

---

## Status

### ✅ Done (Steps 1–2, partial 3–5)

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
| 2.4 | `wasm-pack build` integrated |
| 3.1 | DOM files vendored into `vendor/` |
| 5.1a | napi unit tests (97 pass) |
| 5.1b | napi upstream tests (444 pass) |
| 5.2a | WASM smoke test (10 pass) |

### Test results summary

| Suite | Tests | Status |
|-------|-------|--------|
| `npm test` (napi unit) | 97 | ✅ All pass |
| `npm run test:upstream` (napi) | 444 | ✅ All pass |
| `npm run test:wasm` (WASM smoke) | 10 | ✅ All pass |
| `npm run test:upstream:wasm` (WASM) | 444 | ⚠️ Infrastructure ready, ~1 test file with API gaps |

| Step | Task | Est. effort |
|------|------|-------------|
| 3.2 | Compile vendored DOM `.ts` → `.js` | Medium |
| 5.2b | Fix remaining WASM upstream test API gaps (Schema.spec, node type resolution) | Medium |
| 5.3 | Browser smoke test (Playwright) | Small |
| 4.2 | CI updates | Medium |
| 6.x | Publish workflow + docs | Small |

## WASM upstream test status

### Infrastructure: ✅ Complete
- `node run-upstream-tests.mjs --wasm` builds temp dir, rewrites imports for WASM shims
- `node/test-shim/wasm/prosemirror-model.cjs` — Bridgeschema + raw WASM patches
- `node/test-shim/wasm/prosemirror-transform.cjs` — Step/Transform/Mapping shims

### API bridges implemented:
- `Schema.node` / `Schema.text` — array→Fragment, marks default to `[]`
- `NodeType.create` — array content → Fragment
- `Schema.spec.nodes.get/update/forEach` — OrderedMap compat
- `Fragment.from_array` / `Fragment.from` — schema-first factory
- CamelCase aliases for snake_case methods (Mark.addToSet, ContentMatch.matchType, etc.)
- `Node.fromJSON` / `Node.prototype.toJSON`

### Remaining gaps:
- Some upstream tests use internal Schema APIs (`spec.nodes.update`, `schema.cached`)
- Node type resolution issues in certain test schemas
- `Slice.empty` and other static getters need WASM-specific bridging

### 1.1 Package layout

The published `prosemirror-rs` package ships both back-ends in one tarball.
The source lives at `node/npm/` and the root `node/package.json` points
`main`/`types`/`exports` there:

```
node/
├── package.json              ← main/types/exports point to npm/
├── Cargo.toml / build.rs     ← Rust crate for napi-rs
├── src/                      ← NAPI wrappers (lib.rs, model.rs, transform.rs)
├── copy-artifact.mjs         ← copies .node into npm/napi/
├── tests/                    ← Node.js test runner
├── test-shim/                ← upstream test compatibility shim
├── run-upstream-tests.mjs
└── npm/                      ← publishable package
    ├── package.json          ← conditional exports map
    ├── index.d.ts            ← re-exports model + transform + Editor
    ├── model.js / model.d.ts ← model subpath
    ├── transform.js / transform.d.ts ← transform subpath
    ├── dom.js / dom.d.ts     ← ReplaceError + DOM type declarations
    ├── napi/
    │   ├── index.js          ← loads .node, falls back to WASM
    │   └── *.node files
    └── wasm/
        └── index.js          ← placeholder (throws until WASM is built)
```

### 1.2 Conditional exports — the auto-dispatch mechanism

`package.json` uses the `exports` map with `node` / `browser` / `default`
conditions so that bundlers and runtimes each get the right entry point
**at import resolution time**:

```json
{
  "exports": {
    ".": {
      "node": "./npm/napi/index.js",
      "browser": "./npm/wasm/index.js",
      "default": "./npm/wasm/index.js"
    },
    "./model": {
      "node": "./npm/model.js",
      "browser": "./npm/model.js",
      "default": "./npm/model.js"
    },
    "./transform": {
      "node": "./npm/transform.js",
      "browser": "./npm/transform.js",
      "default": "./npm/transform.js"
    }
  }
}
```

**napi/index.js** — loads the `.node` addon, with WASM fallback:

```js
let binding;
try {
  binding = require("./prosemirror-rs.linux-x64-gnu.node");
} catch {
  binding = require("../wasm/index.js");
}
module.exports = { ...binding, ...require("../dom") };
```

**model.js / transform.js** — subpath re-exports with inline platform detection:

```js
let pkg;
try {
  pkg = require("./napi/index.js");
} catch {
  pkg = require("./wasm/index.js");
}
module.exports = { /* relevant symbols */ };
```

### 1.3 TypeScript declarations

- `model.d.ts` — copied from upstream `prosemirror-model/dist/index.d.ts`
- `transform.d.ts` — copied from upstream `prosemirror-transform/dist/index.d.ts`
- `index.d.ts` — re-exports both plus `Editor`
- `dom.d.ts` — `DOMSerializer`, `DOMParser`, `ParseRule`, `DOMOutputSpec`, etc.

### 1.4 ReplaceError

Defined as a JS class in `node/npm/dom.js`, re-exported via all entry points.
The native binding does not yet throw it from Rust (future enhancement).

### 1.5 contentMatchParse

Already exported from the native binding (`node/src/model.rs`).  Re-exported
via `model.js` subpath.

---

## Step 2 — Build the WASM crate (partial: scaffolding done)

### 2.1 Create `wasm/` workspace member ✅

```
wasm/
├── Cargo.toml             ← wasm-bindgen + js-sys dependencies
└── src/
    ├── lib.rs             ← re-exports model + transform modules
    ├── model.rs           ← placeholder
    └── transform.rs       ← placeholder
```

Added to root `Cargo.toml` workspace members.

### 2.2 Expose the `B*` types via wasm-bindgen (TODO)

The `src/binding/{model,transform}.rs` types (`BNode`, `BFragment`,
`BSchema`, etc.) are already language-agnostic.  Create `#[wasm_bindgen]`
wrappers in `wasm/src/model.rs` and `wasm/src/transform.rs` that forward
every method from the corresponding `B*` inner value.

Example pattern:

```rust
#[wasm_bindgen]
pub struct Node {
    inner: BNode,
}

#[wasm_bindgen]
impl Node {
    #[wasm_bindgen(getter)]
    pub fn type_(&self) -> NodeType { ... }
    pub fn child(&self, index: usize) -> Option<Node> { ... }
    pub fn nodes_between(&self, from: usize, to: usize, f: &js_sys::Function) { ... }
}
```

### 2.3 Key design decisions

- **Schema passing**: Each object holds `Arc<DynamicSchema>` (same as napi).
- **Callback methods**: Accept `&js_sys::Function`, call inline with early-termination support.
- **Complex return types**: Build JS objects with `js_sys::Object::new()`.
- **Editor**: Constructor takes `(schemaJson: string, docJson: string)`.

### 2.4 Build pipeline (TODO)

```bash
cd wasm && wasm-pack build --target web --out-dir ../node/npm/wasm
```

---

## Step 3 — DOM-related types (JS-only supplement)

### 3.1 Vendor the upstream files ✅

Copied the three DOM files from `prosemirror-model@1.25.7` into `vendor/`
with imports rewritten:

```
vendor/
├── dom.ts       ← DOMNode type
├── to-dom.ts    ← DOMSerializer, DOMOutputSpec
└── from-dom.ts  ← DOMParser, ParseRule, etc.
```

Imports rewritten from `"./fragment"` etc. to `"prosemirror-rs/model"`.

### 3.2 Compile to JS (TODO)

The vendored `.ts` files need to be compiled to plain `.js` (CJS for Node,
ESM for WASM path).  A build script should produce:

- `node/npm/vendor/to-dom.cjs` + `node/npm/vendor/to-dom.mjs`
- `node/npm/vendor/from-dom.cjs` + `node/npm/vendor/from-dom.mjs`

These replace the placeholder `dom.js` which currently only exports
`ReplaceError`.

---

## Step 4 — Build & publish workflow (partial)

### 4.1 Current build steps

```
# Build napi-rs native addon
cd node && cargo build --release && node copy-artifact.mjs
# → copies .node to node/npm/napi/

# (WASM build not yet integrated)
# (DOM vendor compilation not yet integrated)
```

### 4.2 Planned full build

```
# 1. napi-rs native addon
cd node && cargo build --release && node copy-artifact.mjs

# 2. WASM
cd wasm && wasm-pack build --target web --out-dir ../node/npm/wasm

# 3. DOM vendor files
cd vendor && node build.mjs  # compiles .ts → .cjs + .mjs

# 4. Publish
cd node && npm publish   # publishes from node/ (includes npm/ via "files")
```

---

## Step 5 — Testing (TODO)

### 5.1 Node.js (napi-rs)

97 tests pass via `node --test tests/*.test.js`.  The upstream test runner
(`run-upstream-tests.mjs`) works against the test-shim which now imports
from `npm/napi/`.

### 5.2 WASM in Node.js (TODO)

Run the same upstream test suite against the WASM back-end.

### 5.3 WASM in browser (TODO)

Playwright test loading the WASM module in a browser context.

---

## Step 6 — Drop-in replacement via bundler aliases

Since we can't take over the `prosemirror-model` / `prosemirror-transform`
npm package names, consumers who want true drop-in replacement without
changing a single import configure their bundler:

**Webpack / Vite:**
```js
resolve: {
  alias: {
    "prosemirror-model": "prosemirror-rs/model",
    "prosemirror-transform": "prosemirror-rs/transform"
  }
}
```

**TypeScript:**
```json
{
  "compilerOptions": {
    "paths": {
      "prosemirror-model": ["./node_modules/prosemirror-rs/model"],
      "prosemirror-transform": ["./node_modules/prosemirror-rs/transform"]
    }
  }
}
```

---

## Implementation order (updated)

| Step | Task                                              | Status    | Dependencies |
|------|---------------------------------------------------|-----------|-------------|
| 1.1  | Restructure npm package layout with subpath exports | ✅ Done  | —           |
| 1.2  | Conditional exports + auto-dispatch logic          | ✅ Done   | 1.1         |
| 1.3  | Complete `.d.ts` files                             | ✅ Done   | 1.1         |
| 1.4  | Add `ReplaceError`                                 | ✅ Done   | 1.1         |
| 1.5  | Add `contentMatchParse` free fn                    | ✅ Done   | 1.1         |
| 1.6  | `Fragment.from` polymorphic                        | ✅ Done   | 1.1         |
| 2.1  | Create `wasm/` workspace member                    | ✅ Done   | —           |
| 2.2  | WASM wrappers for model types                      | ✅ Done   | 2.1         |
| 2.3  | WASM wrappers for transform types                  | ✅ Done   | 2.1         |
| 2.4  | Build pipeline + `wasm-pack` integration           | ✅ Done   | 2.2, 2.3    |
| 3.1  | Vendor DOM files from upstream                     | ✅ Done   | —           |
| 3.2  | Rewrite imports, build CJS + ESM versions          | ⬜ Todo   | 3.1         |
| 4.1  | Integrate WASM + vendor into package build         | ⬜ Todo   | 2.4, 3.2    |
| 4.2  | CI updates                                         | ⬜ Todo   | 4.1         |
| 5.1  | Run upstream tests against restructured package    | ⬜ Todo   | 1.x, 3.x    |
| 5.2  | WASM back-end test suite (Node.js)                 | ⬜ Todo   | 2.2, 2.3    |
| 5.3  | Browser smoke test (Playwright)                    | ⬜ Todo   | 2.4, 3.x    |
| 6.x  | Publish workflow + docs                            | ⬜ Todo   | All above   |

---

## Open questions

1. **`window` / `document` dependency in DOM types**: The vendored `to_dom.ts`
   calls `window.document` by default.  In Node.js (napi path, using jsdom or
   similar), consumers must pass `{document: someJSDOMDocument}`.  In the
   browser (WASM path) it works natively.  Document this clearly.

2. **`Schema.cached`**: Upstream `Schema` has a `.cached` property for
   internal caching (`domSerializer`, etc.).  Our Rust-backed `Schema` does
   not have this.  The vendored DOM types need a small compatibility shim
   or we patch the vendored files to use a `WeakMap`-based cache instead.

3. **`NodeType.spec` returning JS functions**: Upstream `NodeType.spec`
   returns the full spec including `toDOM`, `parseDOM` — JS functions that
   Rust cannot store.  The vendored DOM code accesses `node.type.spec.toDOM`.

   **Recommended approach**: Keep a JS-side `WeakMap` from schema to raw spec,
   like the test shim already does in `node/test-shim/prosemirror-model.cjs`.
   The `NodeType.spec` getter returns the JS spec object from this map.

4. **WASM for Node.js as fallback**: The napi `index.js` tries `.node` first,
   falls back to `require("../wasm/index.js")` if the platform binary is
   missing.  This means Node.js users on unsupported platforms still get a
   working (albeit slower) WASM fallback automatically.

5. **Shared JS code**: The DOM vendor files, `.d.ts` files, and re-export
   wrappers are shared between the napi and WASM paths.  They live in
   `node/npm/` and are imported relatively from both `napi/index.js` and
   `wasm/index.js`.
