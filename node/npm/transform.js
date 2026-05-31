"use strict";

// Re-exports all transform symbols from the correct back-end.
// When used as a subpath export (prosemirror-rs/transform), the package.json
// exports map handles dispatch.  When required directly, we detect the
// environment ourselves.

let pkg;
try {
  // Node.js: try native addon first
  pkg = require("./napi/index.js");
} catch {
  // Browser / WASM fallback
  try {
    pkg = require("./wasm/index.js");
  } catch {
    throw new Error(
      "prosemirror-rs: no back-end available. " +
      "Install a prebuilt binary or build from source."
    );
  }
}

module.exports = {
  // Step types
  Step: pkg.Step,
  ReplaceStep: pkg.ReplaceStep,
  ReplaceAroundStep: pkg.ReplaceAroundStep,
  AddMarkStep: pkg.AddMarkStep,
  RemoveMarkStep: pkg.RemoveMarkStep,
  AddNodeMarkStep: pkg.AddNodeMarkStep,
  RemoveNodeMarkStep: pkg.RemoveNodeMarkStep,
  AttrStep: pkg.AttrStep,
  DocAttrStep: pkg.DocAttrStep,

  // Mapping
  StepMap: pkg.StepMap,
  MapResult: pkg.MapResult,
  Mapping: pkg.Mapping,

  // Transform builder
  Transform: pkg.Transform,

  // Structure utilities (free functions)
  liftTarget: pkg.liftTarget,
  findWrapping: pkg.findWrapping,
  canSplit: pkg.canSplit,
  canJoin: pkg.canJoin,
  joinPoint: pkg.joinPoint,
  insertPoint: pkg.insertPoint,
  dropPoint: pkg.dropPoint,
};
