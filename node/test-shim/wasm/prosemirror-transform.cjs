"use strict";

// WASM transform shim.
// Loads the model bridge and adds transform-specific compat.

const model = require("./prosemirror-model.cjs");
const wasm = require("../../npm/wasm-nodejs/index.js");

// Step constructors
function ReplaceStep(from, to, slice, structure) {
  return wasm.Step_.replace(from, to, slice, structure || false);
}
ReplaceStep.prototype = wasm.Step_.prototype;

function ReplaceAroundStep(from, to, gapFrom, gapTo, slice, insert, structure) {
  return wasm.Step_.replaceAround(from, to, gapFrom, gapTo, slice, insert, structure || false);
}
ReplaceAroundStep.prototype = wasm.Step_.prototype;

function AddMarkStep(from, to, mark) {
  return wasm.Step_.addMark(from, to, mark);
}
AddMarkStep.prototype = wasm.Step_.prototype;

function RemoveMarkStep(from, to, mark) {
  return wasm.Step_.removeMark(from, to, mark);
}
RemoveMarkStep.prototype = wasm.Step_.prototype;

function AddNodeMarkStep(pos, mark) {
  return wasm.Step_.addNodeMark(pos, mark);
}
AddNodeMarkStep.prototype = wasm.Step_.prototype;

function RemoveNodeMarkStep(pos, mark) {
  return wasm.Step_.removeNodeMark(pos, mark);
}
RemoveNodeMarkStep.prototype = wasm.Step_.prototype;

function AttrStep(pos, attr, value) {
  return wasm.Step_.attr(pos, attr, value);
}
AttrStep.prototype = wasm.Step_.prototype;

function DocAttrStep(attr, value) {
  return wasm.Step_.docAttr(attr, value);
}
DocAttrStep.prototype = wasm.Step_.prototype;

// Step.apply shim
const origStepApply = wasm.Step_.prototype.apply;
wasm.Step_.prototype.apply = function (doc) {
  try {
    const result = origStepApply.call(this, doc);
    return { doc: result, failed: null };
  } catch (e) {
    return { doc: null, failed: e.message };
  }
};

// Mapping constructor shim — accepts optional initial StepMap array
const OrigMapping = wasm.Mapping || wasm.Mapping_;
function ShimMapping(maps) {
  const m = new OrigMapping();
  if (Array.isArray(maps)) {
    for (const map of maps) m.appendMap(map);
  }
  return m;
}
ShimMapping.prototype = OrigMapping.prototype;

module.exports = {
  Step: wasm.Step_,
  Transform: model.Transform,
  Mapping: ShimMapping,
  StepMap: wasm.StepMap_,
  MapResult: wasm.MapResult_,
  ReplaceStep,
  ReplaceAroundStep,
  AddMarkStep,
  RemoveMarkStep,
  AddNodeMarkStep,
  RemoveNodeMarkStep,
  AttrStep,
  DocAttrStep,
  findWrapping: wasm.findWrapping,
  liftTarget: wasm.liftTarget,
  canSplit: wasm.canSplit,
  canJoin: wasm.canJoin,
  joinPoint: wasm.joinPoint,
  insertPoint: wasm.insertPoint,
  dropPoint: wasm.dropPoint,
};
