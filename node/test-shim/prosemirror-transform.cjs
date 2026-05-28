const bindings = require("../prosemirror-rs.linux-x64-gnu.node");

function normalizeNodeType(t) {
  if (t instanceof bindings.NodeType) {
    return { type: t.name };
  }
  if (t && t.type instanceof bindings.NodeType) {
    return { type: t.type.name, attrs: t.attrs };
  }
  return t;
}

function normalizeTypes(types) {
  if (!types) return types;
  return types.map(normalizeNodeType);
}

function normalizeWrappers(wrappers) {
  if (!wrappers) return wrappers;
  return wrappers.map(normalizeNodeType);
}

const OrigTransform = bindings.Transform;
function ShimTransform(doc) {
  return new OrigTransform(doc);
}
ShimTransform.prototype = OrigTransform.prototype;

// Methods that need NodeType argument normalization
ShimTransform.prototype.split = function (pos, depth, typesAfter) {
  OrigTransform.prototype.split.call(
    this,
    pos,
    depth,
    normalizeTypes(typesAfter),
  );
  return this;
};

ShimTransform.prototype.wrap = function (range, wrappers) {
  OrigTransform.prototype.wrap.call(this, range, normalizeWrappers(wrappers));
  return this;
};

// Methods that need to return `this` for chaining
const chainMethods = [
  "replace",
  "replaceWith",
  "delete",
  "addMark",
  "removeMark",
  "addNodeMark",
  "removeNodeMark",
  "lift",
  "join",
  "setBlockType",
  "setNodeMarkup",
  "replaceRange",
  "replaceRangeWith",
  "deleteRange",
  "step",
  "maybeStep",
];

for (const name of chainMethods) {
  const orig = OrigTransform.prototype[name];
  if (orig) {
    ShimTransform.prototype[name] = function (...args) {
      orig.apply(this, args);
      return this;
    };
  }
}

function canSplit(doc, pos, depth, typesAfter) {
  return bindings.canSplit(doc, pos, depth, normalizeTypes(typesAfter));
}

module.exports = {
  Step: bindings.Step,
  Transform: ShimTransform,
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
  canSplit,
  canJoin: bindings.canJoin,
  joinPoint: bindings.joinPoint,
  insertPoint: bindings.insertPoint,
  dropPoint: bindings.dropPoint,
};
