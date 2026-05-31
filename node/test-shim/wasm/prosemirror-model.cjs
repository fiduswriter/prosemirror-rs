"use strict";

// WASM → napi API compatibility bridge.
// Wraps the WASM back-end so it looks like the napi back-end, allowing
// the existing test-shim to work with minimal changes.

const wasm = require("../../npm/wasm-nodejs/index.js");

// ---------------------------------------------------------------------------
// Schema bridging
// ---------------------------------------------------------------------------
const OrigSchema = wasm.Schema;
function BridgedSchema(spec) {
  // wasm Schema constructor takes JSON string
  const s = new OrigSchema(typeof spec === "string" ? spec : JSON.stringify(spec));
  this._wasm = s;
  this._rawSpec = spec;
}
BridgedSchema.prototype = Object.create(OrigSchema.prototype);

// nodes / marks as getters (wasm has methods)
Object.defineProperty(BridgedSchema.prototype, "nodes", {
  get() { return this._wasm.nodes(); },
  configurable: true,
});
Object.defineProperty(BridgedSchema.prototype, "marks", {
  get() { return this._wasm.marks(); },
  configurable: true,
});
Object.defineProperty(BridgedSchema.prototype, "topNodeType", {
  get() { return this._wasm.top_node_type(); },
  configurable: true,
});

// node: accepts arrays as content (converts to Fragment)
BridgedSchema.prototype.node = function (typeName, attrs, content, marks) {
  let frag = content;
  if (Array.isArray(content)) {
    frag = wasm.Fragment.from_array(this._wasm, content);
  } else if (content == null) {
    frag = null;
  }
  // Normalize marks — recreate from type to avoid freed WASM pointers
  let wasmMarks = marks || [];
  if (Array.isArray(wasmMarks)) {
    wasmMarks = wasmMarks.map(m => {
      if (m && m.type && m.type.name) {
        return this._wasm.mark(m.type.name, m.attrs || null);
      }
      return m;
    });
  }
  return this._wasm.node(typeName, attrs || null, frag, wasmMarks);
};

// text: marks defaults to empty array, ensure WASM Mark instances
// text: marks defaults to empty array, recreate marks to avoid freed WASM pointers
BridgedSchema.prototype.text = function (text, marks) {
  if (!marks || !Array.isArray(marks)) return this._wasm.text(text, []);
  // Always recreate marks from type info to avoid dangling WASM pointers
  const wasmMarks = [];
  for (const m of marks) {
    if (m && m.type && m.type.name) {
      wasmMarks.push(this._wasm.mark(m.type.name, m.attrs || null));
    } else if (m && typeof m.__wbg_ptr === 'number' && m.__wbg_ptr > 0) {
      wasmMarks.push(m);
    }
  }
  return this._wasm.text(text, wasmMarks);
};

// mark
BridgedSchema.prototype.mark = function (typeName, attrs) {
  return this._wasm.mark(typeName, attrs || null);
};

// nodeFromJson / markFromJson
BridgedSchema.prototype.nodeFromJson = function (json) {
  return this._wasm.node_from_json(json);
};
BridgedSchema.prototype.markFromJson = function (json) {
  return this._wasm.mark_from_json(json);
};

// Static fromJSON
OrigSchema.fromJSON = function (spec) {
  return new BridgedSchema(spec);
};

// Patch raw WASM Schema methods
const origSchemaText = OrigSchema.prototype.text;
OrigSchema.prototype.text = function (text, marks) {
  return origSchemaText.call(this, text, marks || []);
};
const origSchemaNode = OrigSchema.prototype.node;
OrigSchema.prototype.node = function (typeName, attrs, content, marks) {
  let frag = content;
  if (Array.isArray(content)) frag = OrigFragment.from_array(this, content);
  return origSchemaNode.call(this, typeName, attrs, frag || null, marks || []);
};

// Patch raw WASM NodeType.create to handle arrays as content
const OrigNodeType = wasm.NodeType;
if (OrigNodeType && OrigNodeType.prototype) {
  const origCreate = OrigNodeType.prototype.create;
  if (origCreate) {
    OrigNodeType.prototype.create = function (attrs, content, marks) {
      let frag = content;
      if (Array.isArray(content)) {
        const s = this.schema; // NodeType.schema returns raw WASM Schema
        frag = OrigFragment.from_array(s, content);
      }
      return origCreate.call(this, attrs, frag || null, marks || []);
    };
  }
}

// rawSpec storage
const schemaSpecs = new WeakMap();
BridgedSchema.prototype.__getRawSpec = function () {
  return this._rawSpec || schemaSpecs.get(this._wasm);
};
BridgedSchema.prototype.__setRawSpec = function (spec) {
  this._rawSpec = spec;
  schemaSpecs.set(this._wasm, spec);
};

// .spec getter for upstream test compat (OrderedMap-like interface)
Object.defineProperty(BridgedSchema.prototype, "spec", {
  get() {
    const raw = this._rawSpec || {};
    const nodes = Object.assign({}, raw.nodes || {});
    const marks = Object.assign({}, raw.marks || {});

    function makeNodes(base) {
      return {
        get(name) { return base[name]; },
        update(name, value) {
          const updated = Object.assign({}, base);
          updated[name] = value;
          return makeNodes(updated);
        },
        forEach(fn) { Object.keys(base).forEach(k => fn(k, base[k])); },
        append(map) {
          const merged = Object.assign({}, base);
          if (map && map.forEach) map.forEach((v, k) => { merged[k] = v; });
          return makeNodes(merged);
        },
        toJSON() { return base; },
      };
    }

    function makeMarks(base) {
      return {
        get(name) { return base[name]; },
        update(name, value) {
          const updated = Object.assign({}, base);
          updated[name] = value;
          return makeMarks(updated);
        },
        forEach(fn) { Object.keys(base).forEach(k => fn(k, base[k])); },
        append(map) {
          const merged = Object.assign({}, base);
          if (map && map.forEach) map.forEach((v, k) => { merged[k] = v; });
          return makeMarks(merged);
        },
        toJSON() { return base; },
      };
    }

    return {
      nodes: makeNodes(nodes),
      marks: makeMarks(marks),
    };
  },
  configurable: true,
});

// ---------------------------------------------------------------------------
// Fragment bridging (in-place)
// ---------------------------------------------------------------------------
const OrigFragment = wasm.Fragment;
const origFragmentFrom = OrigFragment.from.bind(OrigFragment);
const origFragmentFromArray = OrigFragment.from_array.bind(OrigFragment);

// Fragment.from: bridge to accept JS arrays by wrapping in
// BridgedSchema.getWasmSchema lookup
OrigFragment.from = function (input, schema) {
  if (input == null) {
    // need schema for empty fragment — try to find one
    const s = schema || (OrigFragment._lastSchema);
    return s ? origFragmentFromArray(s, []) : origFragmentFrom(input);
  }
  if (Array.isArray(input)) {
    const s = schema || (OrigFragment._lastSchema);
    if (s) return origFragmentFromArray(s, input);
  }
  return origFragmentFrom(input);
};
OrigFragment.fromArray = function (nodes, schema) {
  const s = schema || (OrigFragment._lastSchema);
  if (s) return origFragmentFromArray(s, nodes);
  // Fallback: try to get schema from first node
  return origFragmentFromArray(nodes);
};
OrigFragment._wasmBridged = true;

// ---------------------------------------------------------------------------
// Node bridging
// ---------------------------------------------------------------------------
// Node.fromJSON
wasm.Node.fromJSON = wasm.Node.from_json || wasm.Node.fromJson;
if (wasm.Node.prototype && wasm.Node.prototype.to_json) {
  wasm.Node.prototype.toJSON = wasm.Node.prototype.to_json;
}

// ---------------------------------------------------------------------------
// CamelCase aliases for WASM snake_case METHODS (not getters)
// ---------------------------------------------------------------------------

// Mark methods
if (wasm.Mark && wasm.Mark.prototype) {
  if (!wasm.Mark.prototype.addToSet && wasm.Mark.prototype.add_to_set) {
    wasm.Mark.prototype.addToSet = wasm.Mark.prototype.add_to_set;
  }
  if (!wasm.Mark.prototype.removeFromSet && wasm.Mark.prototype.remove_from_set) {
    wasm.Mark.prototype.removeFromSet = wasm.Mark.prototype.remove_from_set;
  }
  if (!wasm.Mark.prototype.isInSet && wasm.Mark.prototype.is_in_set) {
    wasm.Mark.prototype.isInSet = wasm.Mark.prototype.is_in_set;
  }
}

// MarkType methods
if (wasm.MarkType && wasm.MarkType.prototype) {
  if (!wasm.MarkType.prototype.removeFromSet && wasm.MarkType.prototype.remove_from_set) {
    wasm.MarkType.prototype.removeFromSet = wasm.MarkType.prototype.remove_from_set;
  }
  if (!wasm.MarkType.prototype.isInSet && wasm.MarkType.prototype.is_in_set) {
    wasm.MarkType.prototype.isInSet = wasm.MarkType.prototype.is_in_set;
  }
}

// Static methods
if (wasm.Mark) {
  if (!wasm.Mark.sameSet && wasm.Mark.same_set) wasm.Mark.sameSet = wasm.Mark.same_set;
  if (!wasm.Mark.setFrom && wasm.Mark.set_from) wasm.Mark.setFrom = wasm.Mark.set_from;
}
if (wasm.Fragment && !wasm.Fragment.fromArray && wasm.Fragment.from_array) {
  wasm.Fragment.fromArray = wasm.Fragment.from_array;
}

// Node methods (not getters — those are handled by patch.js type_ → type)
if (wasm.Node && wasm.Node.prototype) {
  const nodeMethods = [
    ["maybeChild", "maybe_child"], ["sameMarkup", "same_markup"],
    ["contentMatchAt", "content_match_at"], ["textBetween", "text_between"],
    ["rangeHasMark", "range_has_mark"], ["canAppend", "can_append"],
    ["canReplace", "can_replace"], ["canReplaceWith", "can_replace_with"],
    ["hasMarkup", "has_markup"], ["childAfter", "child_after"],
    ["childBefore", "child_before"], ["nodeAt", "node_at"],
  ];
  for (const [camel, snake] of nodeMethods) {
    if (!wasm.Node.prototype[camel] && typeof wasm.Node.prototype[snake] === "function") {
      wasm.Node.prototype[camel] = wasm.Node.prototype[snake];
    }
  }
}

// ResolvedPos methods
if (wasm.ResolvedPos && wasm.ResolvedPos.prototype) {
  if (!wasm.ResolvedPos.prototype.marksAcross && wasm.ResolvedPos.prototype.marks_across) {
    wasm.ResolvedPos.prototype.marksAcross = wasm.ResolvedPos.prototype.marks_across;
  }
  if (!wasm.ResolvedPos.prototype.sameParent && wasm.ResolvedPos.prototype.same_parent) {
    wasm.ResolvedPos.prototype.sameParent = wasm.ResolvedPos.prototype.same_parent;
  }
  if (!wasm.ResolvedPos.prototype.blockRange && wasm.ResolvedPos.prototype.block_range) {
    wasm.ResolvedPos.prototype.blockRange = wasm.ResolvedPos.prototype.block_range;
  }
}

// ContentMatch methods
if (wasm.ContentMatch && wasm.ContentMatch.prototype) {
  const cmMethods = [
    ["matchType", "match_type"], ["matchFragment", "match_fragment"],
    ["fillBefore", "fill_before"], ["defaultType", "default_type"],
    ["findWrapping", "find_wrapping"], ["edgeType", "edge_type"],
    ["edgeMatch", "edge_match"],
  ];
  for (const [camel, snake] of cmMethods) {
    if (!wasm.ContentMatch.prototype[camel] && typeof wasm.ContentMatch.prototype[snake] === "function") {
      wasm.ContentMatch.prototype[camel] = wasm.ContentMatch.prototype[snake];
    }
  }
}

// NodeType methods
if (wasm.NodeType && wasm.NodeType.prototype) {
  if (!wasm.NodeType.prototype.createChecked && wasm.NodeType.prototype.create_checked) {
    wasm.NodeType.prototype.createChecked = wasm.NodeType.prototype.create_checked;
  }
  if (!wasm.NodeType.prototype.createAndFill && wasm.NodeType.prototype.create_and_fill) {
    wasm.NodeType.prototype.createAndFill = wasm.NodeType.prototype.create_and_fill;
  }
}

// ---------------------------------------------------------------------------
// Step bridging
// ---------------------------------------------------------------------------
if (wasm.Step_) {
  wasm.Step = wasm.Step_;
  wasm.Step_.prototype.toJSON = wasm.Step_.prototype.to_json || wasm.Step_.prototype.toJson;
  wasm.Step_.fromJSON = wasm.Step_.from_json || wasm.Step_.fromJson;
}

// ---------------------------------------------------------------------------
// Transform bridging
// ---------------------------------------------------------------------------
if (wasm.Transform_) {
  const OrigTransform = wasm.Transform_;
  function BridgedTransform(doc) {
    if (!(this instanceof BridgedTransform)) return new BridgedTransform(doc);
    return Reflect.construct(OrigTransform, [doc], new.target);
  }
  BridgedTransform.prototype = OrigTransform.prototype;

  // Chainable methods
  const chainMethods = [
    "replace", "delete", "addMark", "addNodeMark", "setNodeMarkup",
    "setNodeAttribute", "setDocAttribute", "step", "maybeStep",
  ];
  for (const name of chainMethods) {
    const orig = OrigTransform.prototype[name];
    if (orig) {
      BridgedTransform.prototype[name] = function (...args) {
        orig.apply(this, args);
        return this;
      };
    }
  }

  wasm.Transform = BridgedTransform;
}

// ---------------------------------------------------------------------------
// Mapping bridging
// ---------------------------------------------------------------------------
if (wasm.Mapping_) {
  wasm.Mapping = wasm.Mapping_;
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------
module.exports = {
  Schema: BridgedSchema,
  Node: wasm.Node,
  Fragment: OrigFragment,
  Slice: wasm.Slice,
  ResolvedPos: wasm.ResolvedPos,
  Mark: wasm.Mark,
  MarkType: wasm.MarkType,
  NodeType: wasm.NodeType,
  ContentMatch: wasm.ContentMatch,
  ReplaceError: wasm.ReplaceError || (class ReplaceError extends Error {}),
  Step: wasm.Step || wasm.Step_,
  Transform: wasm.Transform || wasm.Transform_,
  StepMap: wasm.StepMap_,
  MapResult: wasm.MapResult_,
  Mapping: wasm.Mapping || wasm.Mapping_,
  // Free functions
  contentMatchParse: wasm.contentMatchParse,
  canSplit: wasm.canSplit,
  canJoin: wasm.canJoin,
  joinPoint: wasm.joinPoint,
  insertPoint: wasm.insertPoint,
  dropPoint: wasm.dropPoint,
  liftTarget: wasm.liftTarget,
  findWrapping: wasm.findWrapping,
};

// ContentMatch.parse static
if (wasm.ContentMatch && !wasm.ContentMatch.parse && wasm.contentMatchParse) {
  wasm.ContentMatch.parse = function (expr, nodeTypes) {
    return wasm.contentMatchParse(expr, nodeTypes);
  };
}

// Step.toJSON / Transform.toJSON
if (wasm.Step_ && wasm.Step_.prototype) {
  if (!wasm.Step_.prototype.toJSON && wasm.Step_.prototype.to_json) {
    wasm.Step_.prototype.toJSON = wasm.Step_.prototype.to_json;
  }
}

// Slice static
if (wasm.Slice) {
  if (!wasm.Slice.empty && wasm.Slice.prototype) {
    // Add empty as a static if not present
  }
}
