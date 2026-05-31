"use strict";

// WASM transform shim.
// Loads the model bridge and adds transform-specific compat.

const model = require("./prosemirror-model.cjs");
const wasm = require("../../npm/wasm-nodejs/index.js");

// --------------------------------------------------------------------------
// NodeType normalization — converts NodeType objects to {type: name} strings
// that the WASM Rust code can deserialize.
// --------------------------------------------------------------------------
function normalizeNodeType(t) {
  if (t instanceof wasm.NodeType) {
    return { type: t.name };
  }
  if (t && t.type instanceof wasm.NodeType) {
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

// Patch BridgedTransform methods that take NodeType arguments
(function() {
  const BridgedTransform = model.Transform;
  if (!BridgedTransform || !BridgedTransform.prototype) return;

  const origSplit = wasm.Transform_.prototype.split;
  const origWrap = wasm.Transform_.prototype.wrap;
  const origRemoveMark = wasm.Transform_.prototype.removeMark;
  const origRemoveMarkType = wasm.Transform_.prototype.removeMarkType;
  const origRemoveNodeMark = wasm.Transform_.prototype.removeNodeMark;
  const origRemoveNodeMarkType = wasm.Transform_.prototype.removeNodeMarkType;
  const origSetBlockType = wasm.Transform_.prototype.setBlockType;
  const origLift = wasm.Transform_.prototype.lift;

  // Override split to normalize types_after
  if (origSplit) {
    BridgedTransform.prototype.split = function (pos, depth, typesAfter) {
      origSplit.call(this, pos, depth, normalizeTypes(typesAfter));
      return this;
    };
  }

  // Override wrap to normalize wrappers
  if (origWrap) {
    BridgedTransform.prototype.wrap = function (range, wrappers) {
      origWrap.call(this, range, normalizeWrappers(wrappers));
      return this;
    };
  }

  // Override lift to return this (already handled by chainMethods, but needs proper range)
  if (origLift) {
    BridgedTransform.prototype.lift = function (range, target) {
      origLift.call(this, range, target);
      return this;
    };
  }

  // Override removeMark to accept MarkType or Mark
  if (origRemoveMark && origRemoveMarkType) {
    BridgedTransform.prototype.removeMark = function (from, to, mark) {
      if (mark && mark.create && !(mark instanceof wasm.Mark)) {
        origRemoveMarkType.call(this, from, to, mark);
      } else {
        origRemoveMark.call(this, from, to, mark || undefined);
      }
      return this;
    };
  }

  // Override removeNodeMark to accept MarkType or Mark
  if (origRemoveNodeMark && origRemoveNodeMarkType) {
    BridgedTransform.prototype.removeNodeMark = function (pos, mark) {
      if (mark && mark.create && !(mark instanceof wasm.Mark)) {
        origRemoveNodeMarkType.call(this, pos, mark);
      } else {
        origRemoveNodeMark.call(this, pos, mark || undefined);
      }
      return this;
    };
  }

  // Override setBlockType to handle function-style attrs
  if (origSetBlockType) {
    const proxySetBlockType = BridgedTransform.prototype.setBlockType;
    BridgedTransform.prototype.setBlockType = function (from, to, type, attrs) {
      if (typeof attrs === 'function') {
        const newFrom = this.mapping.map(from, -1);
        const newTo = to != null ? this.mapping.map(to, 1) : newFrom;
        this.doc.nodesBetween(newFrom, newTo, function (node, pos) {
          if (node.isTextblock) {
            const computedAttrs = attrs(node);
            const mappedPos = this.mapping.map(pos + 1, 1);
            origSetBlockType.call(this, mappedPos, mappedPos, type, computedAttrs);
          }
        }.bind(this));
        return this;
      }
      return proxySetBlockType.call(this, from, to, type, attrs);
    };
  }

  // Override insert to handle arrays (convert to Fragment)
  if (wasm.Transform_.prototype.replace) {
    const origReplace = wasm.Transform_.prototype.replace;
    const origInsert = wasm.Transform_.prototype.insert;
    if (origInsert) {
      BridgedTransform.prototype.insert = function (pos, content) {
        // If content is an array, use replaceWith
        if (Array.isArray(content)) {
          const Fragment = wasm.Fragment;
          const Slice = wasm.Slice;
          const frag = Fragment.fromArray(this.doc.type.schema, content);
          origReplace.call(this, pos, pos, new Slice(frag, 0, 0));
          return this;
        }
        origInsert.call(this, pos, content);
        return this;
      };
    }
  }

  // Override replaceWith to handle arrays and nodes
  if (wasm.Transform_.prototype.replace) {
    const origReplace = wasm.Transform_.prototype.replace;
    BridgedTransform.prototype.replaceWith = function (from, to, content) {
      let fragment;
      if (content instanceof wasm.Fragment) {
        fragment = content;
      } else if (Array.isArray(content)) {
        fragment = wasm.Fragment.fromArray(this.doc.type.schema, content);
      } else if (content instanceof wasm.Node) {
        fragment = wasm.Fragment.fromArray(this.doc.type.schema, [content]);
      } else {
        fragment = wasm.Fragment.fromArray(this.doc.type.schema, [content]);
      }
      origReplace.call(this, from, to, new wasm.Slice(fragment, 0, 0));
      return this;
    };
  }
})();

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

// Step.apply shim — the WASM apply already returns {doc, failed}.
// We just catch errors thrown by the WASM function.
const origStepApply = wasm.Step_.prototype.apply;
wasm.Step_.prototype.apply = function (doc) {
  try {
    return origStepApply.call(this, doc);
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
    canSplit: function (doc, pos, depth, typesAfter) {
    return wasm.canSplit(doc, pos, depth, normalizeTypes(typesAfter));
  },
  canJoin: wasm.canJoin,
  joinPoint: wasm.joinPoint,
  insertPoint: wasm.insertPoint,
  dropPoint: wasm.dropPoint,
};
