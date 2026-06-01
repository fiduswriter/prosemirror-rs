// WASM back-end for prosemirror-rs.
// Re-exports everything from the wasm-pack generated module.
// Used as the browser/bundler entry point via the package.json exports map.

import init, * as wasmModule from "./prosemirror_rs_wasm.js";

// Initialize the WASM module. In a browser this fetches the .wasm file
// relative to this module. Bundlers should copy the .wasm file to the
// output directory so that the relative URL resolves.
await init();

// ES module namespace objects have read-only properties (getters only).
// patchStatics needs to mutate exports (e.g. replace Schema with a patched
// constructor), so we shallow-copy into a plain object first.
const binding = { ...wasmModule };

// Apply JS-side patches (Fragment.from, Slice.empty, etc.)
// These are shared with the napi back-end.
import { patchStatics } from "../patch.js";
patchStatics(binding);

// Merge DOM types
import * as dom from "../dom.js";
const domTypes = dom.createDOMTypes(binding);

export const {
  Schema, Node, NodeType, Fragment, Slice, ResolvedPos, NodeRange,
  Mark, MarkType, ContentMatch,
  StepMap, MapResult, Mapping, Step, Transform,
  liftTarget, findWrapping, canSplit, canJoin,
  joinPoint, insertPoint, dropPoint,
  contentMatchParse,
  // Step constructors (added by patch.js)
  ReplaceStep, ReplaceAroundStep,
  AddMarkStep, RemoveMarkStep,
  AddNodeMarkStep, RemoveNodeMarkStep,
  AttrStep, DocAttrStep,
  replaceStep,
} = binding;

export const { ReplaceError, DOMSerializer, DOMParser } = domTypes;

// Re-export remaining DOM+patch symbols
export { setRawSpec, getRawSpec } from "../patch.js";
