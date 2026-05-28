const bindings = require("../prosemirror-rs.linux-x64-gnu.node");

module.exports = {
  Step: bindings.Step,
  Transform: bindings.Transform,
  Mapping: bindings.Mapping,
  StepMap: bindings.StepMap,
  MapResult: bindings.MapResult,
  ReplaceStep: bindings.Step,
  ReplaceAroundStep: bindings.Step,
  AddMarkStep: bindings.Step,
  RemoveMarkStep: bindings.Step,
  AddNodeMarkStep: bindings.Step,
  RemoveNodeMarkStep: bindings.Step,
  AttrStep: bindings.Step,
  DocAttrStep: bindings.Step,
  findWrapping: bindings.findWrapping,
  liftTarget: bindings.liftTarget,
  canSplit: bindings.canSplit,
  canJoin: bindings.canJoin,
  joinPoint: bindings.joinPoint,
  insertPoint: bindings.insertPoint,
  dropPoint: bindings.dropPoint,
};
