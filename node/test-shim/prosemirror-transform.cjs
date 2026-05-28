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

function unwrapRange(range) {
  if (range && typeof range.from === "object" && typeof range.to === "object") {
    return [range.from, range.to];
  }
  return [range, range];
}

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
  const [from, to] = unwrapRange(range);
  OrigTransform.prototype.wrap.call(
    this,
    from,
    to,
    normalizeWrappers(wrappers),
  );
  return this;
};

ShimTransform.prototype.lift = function (range, target) {
  const [from, to] = unwrapRange(range);
  OrigTransform.prototype.lift.call(this, from, to, target);
  return this;
};

ShimTransform.prototype.removeNodeMark = function (pos, mark) {
  // Upstream accepts either a Mark or a MarkType.
  if (mark && mark.create) {
    OrigTransform.prototype.removeNodeMarkType.call(this, pos, mark);
  } else {
    OrigTransform.prototype.removeNodeMark.call(this, pos, mark);
  }
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

function liftTarget(range) {
  const [from, to] = unwrapRange(range);
  return bindings.liftTarget(from, to);
}

function findWrapping(range, nodeType, attrs) {
  const [from, to] = unwrapRange(range);
  return bindings.findWrapping(from, to, nodeType, attrs);
}

// ---------------------------------------------------------------------------
// Step.prototype.apply shim — upstream expects StepResult {doc, failed}
// ---------------------------------------------------------------------------

const origStepApply = bindings.Step.prototype.apply;
bindings.Step.prototype.apply = function (doc) {
  try {
    const result = origStepApply.call(this, doc);
    return { doc: result, failed: null };
  } catch (e) {
    return { doc: null, failed: e.message };
  }
};

// ---------------------------------------------------------------------------
// Step constructors — upstream tests use `new ReplaceStep(from, to, slice)` etc.
// ---------------------------------------------------------------------------

function ReplaceStep(from, to, slice) {
  return bindings.Step.replace(from, to, slice, false);
}
ReplaceStep.prototype = bindings.Step.prototype;

function ReplaceAroundStep(from, to, gapFrom, gapTo, slice, insert, structure) {
  return bindings.Step.replaceAround(
    from,
    to,
    gapFrom,
    gapTo,
    slice,
    insert,
    structure,
  );
}
ReplaceAroundStep.prototype = bindings.Step.prototype;

function AddMarkStep(from, to, mark) {
  return bindings.Step.addMark(from, to, mark);
}
AddMarkStep.prototype = bindings.Step.prototype;

function RemoveMarkStep(from, to, mark) {
  return bindings.Step.removeMark(from, to, mark);
}
RemoveMarkStep.prototype = bindings.Step.prototype;

function AddNodeMarkStep(pos, mark) {
  return bindings.Step.addNodeMark(pos, mark);
}
AddNodeMarkStep.prototype = bindings.Step.prototype;

function RemoveNodeMarkStep(pos, mark) {
  return bindings.Step.removeNodeMark(pos, mark);
}
RemoveNodeMarkStep.prototype = bindings.Step.prototype;

function AttrStep(pos, attr, value) {
  return bindings.Step.attr(pos, attr, value);
}
AttrStep.prototype = bindings.Step.prototype;

function DocAttrStep(attr, value) {
  return bindings.Step.docAttr(attr, value);
}
DocAttrStep.prototype = bindings.Step.prototype;

module.exports = {
  Step: bindings.Step,
  Transform: ShimTransform,
  Mapping: bindings.Mapping,
  StepMap: bindings.StepMap,
  MapResult: bindings.MapResult,
  ReplaceStep,
  ReplaceAroundStep,
  AddMarkStep,
  RemoveMarkStep,
  AddNodeMarkStep,
  RemoveNodeMarkStep,
  AttrStep,
  DocAttrStep,
  findWrapping,
  liftTarget,
  canSplit,
  canJoin: bindings.canJoin,
  joinPoint: bindings.joinPoint,
  insertPoint: bindings.insertPoint,
  dropPoint: bindings.dropPoint,
};
