const bindings = require("../prosemirror-rs.linux-x64-gnu.node");

// Upstream-compatible aliases
const { Step, Transform } = bindings;
Step.prototype.toJSON = Step.prototype.toJson;
Step.fromJSON = Step.fromJson;
Transform.prototype.toJSON = Transform.prototype.toJson;
Transform.fromJSON = Transform.fromJson;

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
  const tr = new OrigTransform(doc);
  tr._originalDoc = doc;
  return tr;
}
ShimTransform.prototype = OrigTransform.prototype;

// Preserve the original doc's JS properties (like .tag from test-builder)
const origDocsGetter = Object.getOwnPropertyDescriptor(
  OrigTransform.prototype,
  "docs",
).get;
Object.defineProperty(ShimTransform.prototype, "docs", {
  get() {
    const docs = origDocsGetter.call(this);
    if (this._originalDoc && docs.length > 0) {
      docs[0] = this._originalDoc;
    }
    return docs;
  },
});

Object.defineProperty(ShimTransform.prototype, "before", {
  get() {
    const docs = this.docs;
    if (docs.length > 0) return docs[0];
    return this._originalDoc || this.doc;
  },
});

// Save original native methods before overrides
const origSplit = OrigTransform.prototype.split;
const origWrap = OrigTransform.prototype.wrap;
const origLift = OrigTransform.prototype.lift;
const origRemoveMark = OrigTransform.prototype.removeMark;
const origRemoveMarkType = OrigTransform.prototype.removeMarkType;
const origRemoveNodeMark = OrigTransform.prototype.removeNodeMark;
const origRemoveNodeMarkType = OrigTransform.prototype.removeNodeMarkType;

function unwrapRange(range) {
  if (range && typeof range.from === "object" && typeof range.to === "object") {
    return [range.from, range.to];
  }
  return [range, range];
}

// Methods that need NodeType argument normalization
ShimTransform.prototype.split = function (pos, depth, typesAfter) {
  origSplit.call(this, pos, depth, normalizeTypes(typesAfter));
  return this;
};

ShimTransform.prototype.wrap = function (range, wrappers) {
  origWrap.call(this, range, normalizeWrappers(wrappers));
  return this;
};

ShimTransform.prototype.lift = function (range, target) {
  origLift.call(this, range, target);
  return this;
};

ShimTransform.prototype.removeMark = function (from, to, mark) {
  // Upstream accepts Mark, MarkType, or null/undefined.
  if (mark && mark.create) {
    origRemoveMarkType.call(this, from, to, mark);
  } else {
    origRemoveMark.call(this, from, to, mark || undefined);
  }
  return this;
};

ShimTransform.prototype.removeNodeMark = function (pos, mark) {
  // Upstream accepts either a Mark or a MarkType.
  if (mark && mark.create) {
    origRemoveNodeMarkType.call(this, pos, mark);
  } else {
    origRemoveNodeMark.call(this, pos, mark);
  }
  return this;
};

// Methods that need to return `this` for chaining
const chainMethods = [
  "replace",
  "delete",
  "addMark",
  "addNodeMark",
  "join",
  "setBlockType",
  "setNodeMarkup",
  "setNodeAttribute",
  "setDocAttribute",
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

const origReplace = OrigTransform.prototype.replace;

ShimTransform.prototype.replaceWith = function (from, to, content) {
  let fragment;
  if (content instanceof bindings.Fragment) {
    fragment = content;
  } else if (Array.isArray(content)) {
    fragment = bindings.Fragment.from(content);
  } else if (content instanceof bindings.Node) {
    fragment = bindings.Fragment.from([content]);
  } else {
    fragment = bindings.Fragment.from([content]);
  }
  origReplace.call(this, from, to, new bindings.Slice(fragment, 0, 0));
  return this;
};

ShimTransform.prototype.insert = function (pos, content) {
  return this.replaceWith(pos, pos, content);
};

function canSplit(doc, pos, depth, typesAfter) {
  return bindings.canSplit(doc, pos, depth, normalizeTypes(typesAfter));
}

function liftTarget(range) {
  return bindings.liftTarget(range);
}

function findWrapping(range, nodeType, attrs) {
  return bindings.findWrapping(range, nodeType, attrs);
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
