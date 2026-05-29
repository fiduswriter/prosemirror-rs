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

// ---------------------------------------------------------------------------
// Linebreak <-> newline conversion helpers (linebreakReplacement schema support)
// ---------------------------------------------------------------------------

function getLbType(schema, rawSpec) {
  // Try using NodeType.spec from the schema's nodes (works for shim schemas)
  if (schema && schema.nodes) {
    for (const name in schema.nodes) {
      const spec = schema.nodes[name].spec;
      if (spec && spec.linebreakReplacement) return schema.nodes[name];
    }
  }
  // Fall back: look in rawSpec (works for Rust NAPI schemas where .spec may be incomplete)
  if (rawSpec && rawSpec.nodes) {
    for (const name in rawSpec.nodes) {
      const nodeSpec = rawSpec.nodes[name];
      if (nodeSpec && nodeSpec.linebreakReplacement) {
        // Return NodeType from the schema's nodes
        if (schema && schema.nodes && schema.nodes[name]) return schema.nodes[name];
        break;
      }
    }
  }
  return null;
}

// Build new content fragment with \n -> br conversion (for code->non-code)
function contentNewlinesToBreaks(fragment, schema, lbType) {
  const result = [];
  for (let i = 0; i < fragment.childCount; i++) {
    const child = fragment.child(i);
    if (child.isText) {
      const text = child.text;
      if (text.includes("\n")) {
        // Split text at newlines and insert br nodes
        const parts = text.split("\n");
        for (let j = 0; j < parts.length; j++) {
          if (parts[j].length > 0) result.push(schema.text(parts[j]));
          if (j < parts.length - 1) result.push(lbType.create());
        }
      } else {
        result.push(child);
      }
    } else if (child.type === lbType) {
      result.push(child);
    } else {
      result.push(child);
    }
  }
  return result; // return as array; NodeType.create will normalize it
}

// Build new content fragment with br -> \n conversion (for non-code->code)
function contentBreaksToNewlines(fragment, schema, lbType) {
  const result = [];
  for (let i = 0; i < fragment.childCount; i++) {
    const child = fragment.child(i);
    if (child.type.name === lbType.name) {
      result.push(schema.text("\n"));
    } else {
      result.push(child);
    }
  }
  return result; // return as array
}

// setBlockType: supports attrs as a plain object OR a function (node) => attrs
const origSetBlockType = OrigTransform.prototype.setBlockType;
ShimTransform.prototype.setBlockType = function (from, to, type, attrs) {
  const targetIsCode = !!(type.spec && type.spec.code);
  const lbType = getLbType(type.schema);

  // Collect source nodes that need linebreak conversion
  const lbConversions = [];
  if (lbType) {
    this.doc.nodesBetween(from, to != null ? to : from, (node, pos) => {
      if (node.isTextblock) {
        const sourceIsCode = !!(node.type.spec && node.type.spec.code);
        if (sourceIsCode !== targetIsCode) {
          lbConversions.push({ node, pos, sourceIsCode });
        }
      }
    });
  }

  if (typeof attrs === "function") {
    const newFrom = this.mapping.map(from, -1);
    const newTo = to != null ? this.mapping.map(to, 1) : newFrom;
    const blocks = [];
    this.doc.nodesBetween(newFrom, newTo, (node, pos) => {
      if (node.isTextblock) blocks.push({ node, pos });
    });
    for (let { node, pos } of blocks) {
      const sourceIsCode = !!(node.type.spec && node.type.spec.code);
      const mappedPos = this.mapping.map(pos + 1, 1);
      const computedAttrs = attrs(node);
      if (lbType && sourceIsCode !== targetIsCode) {
        // Manual conversion: build new node with converted content
        const mappedNodePos = this.mapping.map(pos, -1);
        const newFrag = sourceIsCode && !targetIsCode
          ? contentNewlinesToBreaks(node.content, type.schema, lbType)
          : contentBreaksToNewlines(node.content, type.schema, lbType);
        const newNode = type.create(computedAttrs, newFrag);
          this.replace(mappedNodePos, mappedNodePos + node.nodeSize, new bindings.Slice(bindings.Fragment.from([newNode]), 0, 0));
      } else {
        origSetBlockType.call(this, mappedPos, mappedPos, type, computedAttrs);
      }
    }
    return this;
  }

  if (lbType && lbConversions.length > 0) {
    // For blocks needing linebreak conversion, do manual replacement
    // Work backwards to avoid position shifts
    for (let i = lbConversions.length - 1; i >= 0; i--) {
      const { node, pos, sourceIsCode } = lbConversions[i];
      const mappedPos = this.mapping.map(pos, -1);
      const nodeSize = node.nodeSize;
      const nodeAttrs = typeof attrs === "object" ? attrs : null;
      const newFrag = sourceIsCode && !targetIsCode
        ? contentNewlinesToBreaks(node.content, type.schema, lbType)
        : contentBreaksToNewlines(node.content, type.schema, lbType);
      const newNode = type.create(nodeAttrs, newFrag);
      this.replace(mappedPos, mappedPos + nodeSize, new bindings.Slice(bindings.Fragment.from([newNode]), 0, 0));
    }
    // Apply setBlockType for remaining nodes that don't need lb conversion
    // (Simplified: if all nodes need conversion, skip the setBlockType call)
    const hasNonLbNodes = (to != null ? to - from : 0) > 0; // rough check
    if (lbConversions.length === 0) {
      origSetBlockType.call(this, this.mapping.map(from, -1), to != null ? this.mapping.map(to, 1) : undefined, type, attrs);
    }
    return this;
  }

  origSetBlockType.call(this, from, to, type, attrs);
  return this;
};

// join: apply linebreak conversion after joining code <-> non-code blocks
const origJoin = OrigTransform.prototype.join;
ShimTransform.prototype.join = function (pos, depth) {
  // Record types of the two nodes being joined
  const $pos = this.doc.resolve(pos);
  const before = $pos.nodeBefore;
  const after = $pos.nodeAfter;
  const lbType = getLbType(this.doc.type.schema);

  origJoin.call(this, pos, depth);

  if (lbType && before && after) {
    const beforeIsCode = !!(before.type.spec && before.type.spec.code);
    const afterIsCode = !!(after.type.spec && after.type.spec.code);
    if (beforeIsCode !== afterIsCode) {
      // The joined node takes the type of "before".
      // Find the merged node in the new doc and convert content
      const mappedPos = this.mapping.map(pos - 1, -1);
      const $joined = this.doc.resolve(mappedPos);
      const joinedStart = $joined.start($joined.depth);
      const joinedEnd = $joined.end($joined.depth);

      if (!beforeIsCode && afterIsCode) {
        // Joined into a non-code block: convert \n -> br (work backwards)
        const nlPositions = [];
        this.doc.nodesBetween(joinedStart, joinedEnd, (n, p) => {
          if (n.isText) {
            const text = n.text;
            for (let i = text.length - 1; i >= 0; i--) {
              if (text[i] === "\n") nlPositions.push(p + i);
            }
          }
        });
        for (const nlPos of nlPositions) {
          this.replaceWith(nlPos, nlPos + 1, lbType.create());
        }
      } else if (beforeIsCode && !afterIsCode) {
        // Joined into a code block: convert br -> \n (work backwards)
        const lbPositions = [];
        this.doc.nodesBetween(joinedStart, joinedEnd, (n, p) => {
          if (n.type.name === lbType.name) lbPositions.push(p);
        });
        const schema = $joined.parent.type.schema;
        for (let i = lbPositions.length - 1; i >= 0; i--) {
          const lbPos = lbPositions[i];
          this.replaceWith(lbPos, lbPos + 1, schema.text("\n"));
        }
      }
    }
  }
  return this;
};

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

// Mapping constructor shim — accepts optional initial StepMap array
const OrigMapping = bindings.Mapping;
function ShimMapping(maps) {
  const m = new OrigMapping();
  if (Array.isArray(maps)) {
    for (const map of maps) m.appendMap(map);
  }
  return m;
}
ShimMapping.prototype = OrigMapping.prototype;

module.exports = {
  Step: bindings.Step,
  Transform: ShimTransform,
  Mapping: ShimMapping,
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
