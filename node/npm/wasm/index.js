// WASM back-end for prosemirror-rs.
// Re-exports everything from the wasm-pack generated module.
// Used as the browser/bundler entry point via the package.json exports map.

import * as wasm from "./prosemirror_rs_wasm.js";

// Apply JS-side patches (Fragment.from, Slice.empty, etc.)
// These are shared with the napi back-end.
import { patchStatics } from "../patch.js";
patchStatics(wasm);

// Merge DOM types
import * as dom from "../dom.js";

export const {
  Schema, Node, NodeType, Fragment, Slice, ResolvedPos, NodeRange,
  Mark, MarkType, ContentMatch,
  StepMap, MapResult, Mapping, Step, Transform,
  liftTarget, findWrapping, canSplit, canJoin,
  joinPoint, insertPoint, dropPoint,
  contentMatchParse,
} = wasm;

export const { ReplaceError } = dom;

// Re-export remaining DOM+patch symbols
export { setRawSpec, getRawSpec } from "../patch.js";
