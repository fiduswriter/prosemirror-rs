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
// Also proxy __wbg_ptr so WASM interop sees the underlying pointer
Object.defineProperty(BridgedSchema.prototype, "__wbg_ptr", {
  get() { return this._wasm ? this._wasm.__wbg_ptr : undefined; },
  configurable: true,
});
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

// node: accepts arrays as content (converts to Fragment), also handles single Node
BridgedSchema.prototype.node = function (typeName, attrs, content, marks) {
  let frag = content;
  if (Array.isArray(content)) {
    frag = wasm.Fragment.from_array(this._wasm, content);
  } else if (content != null && typeof content === 'object' && content.type) {
    // Single Node — wrap in Fragment
    frag = wasm.Fragment.from_array(this._wasm, [content]);
  } else if (content == null) {
    frag = null;
  }
  // Normalize marks — recreate from type to avoid freed WASM pointers
  let wasmMarks = marks || [];
  if (Array.isArray(wasmMarks)) {
    wasmMarks = wasmMarks.map(m => {
      if (!m) return m;
      // Try to get type name from both napi (m.type.name) and WASM (m.type_)
      const typeObj = m.type || m.type_;
      if (typeObj && typeObj.name) {
        const attrs = typeof m.attrs === 'function' ? m.attrs() : (m.attrs || null);
        return this._wasm.mark(typeObj.name, attrs);
      }
      // If it's already a WASM Mark with a valid pointer, keep it
      if (typeof m.__wbg_ptr === 'number' && m.__wbg_ptr > 0) {
        return m;
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
    if (!m) continue;
    // Try to get type name from both napi (m.type.name) and WASM (m.type_)
    const typeObj = m.type || m.type_;
    if (typeObj && typeObj.name) {
      const attrs = typeof m.attrs === 'function' ? m.attrs() : (m.attrs || null);
      wasmMarks.push(this._wasm.mark(typeObj.name, attrs));
    } else if (typeof m.__wbg_ptr === 'number' && m.__wbg_ptr > 0) {
      wasmMarks.push(m);
    }
  }
  return this._wasm.text(text, wasmMarks);
};

// mark
BridgedSchema.prototype.mark = function (typeName, attrs) {
  return this._wasm.mark(typeName, attrs || null);
};

// nodeFromJson / markFromJson — tests call nodeFromJSON/markFromJSON (camelCase)
BridgedSchema.prototype.nodeFromJson = function (json) {
  return this._wasm.node_from_json(json);
};
BridgedSchema.prototype.nodeFromJSON = BridgedSchema.prototype.nodeFromJson;
BridgedSchema.prototype.markFromJson = function (json) {
  return this._wasm.mark_from_json(json);
};
BridgedSchema.prototype.markFromJSON = BridgedSchema.prototype.markFromJson;

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
  else if (content != null && typeof content === 'object' && content.type)
    frag = OrigFragment.from_array(this, [content]);
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
        const s = this.schema;
        frag = OrigFragment.from_array(s, content);
      } else if (content != null && typeof content === 'object' && content.type) {
        const s = this.schema;
        frag = OrigFragment.from_array(s, [content]);
      }
      return origCreate.call(this, attrs, frag || null, marks || []);
    };
  }
  // Also patch createChecked and createAndFill
  ['create_checked', 'create_and_fill'].forEach(method => {
    const orig = OrigNodeType.prototype[method];
    if (orig) {
      OrigNodeType.prototype[method] = function (attrs, content, marks) {
        let frag = content;
        if (Array.isArray(content)) {
          const s = this.schema;
          frag = OrigFragment.from_array(s, content);
        } else if (content != null && typeof content === 'object' && content.type) {
          const s = this.schema;
          frag = OrigFragment.from_array(s, [content]);
        } else if (content == null) {
          frag = null;
        }
        return orig.call(this, attrs, frag, marks || []);
      };
    }
  });
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
          if (map && typeof map.forEach === 'function') {
            map.forEach((v, k) => { merged[k] = v; });
          } else if (map && typeof map === 'object') {
            Object.assign(merged, map);
          }
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
          if (map && typeof map.forEach === 'function') {
            map.forEach((v, k) => { merged[k] = v; });
          } else if (map && typeof map === 'object') {
            Object.assign(merged, map);
          }
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

// CamelCase aliases for Fragment methods
if (OrigFragment && OrigFragment.prototype) {
  const fragMethods = [
    ["findDiffStart", "find_diff_start"],
    ["findDiffEnd", "find_diff_end"],
  ];
  for (const [camel, snake] of fragMethods) {
    if (!OrigFragment.prototype[camel] && typeof OrigFragment.prototype[snake] === "function") {
      OrigFragment.prototype[camel] = OrigFragment.prototype[snake];
    }
  }
}

// Fragment.from: bridge the JS API to the WASM API.
// JS: Fragment.from(schema) → empty frag
// JS: Fragment.from(node) → frag from single node
// JS: Fragment.from(nodes[]) → frag from array
// WASM: Fragment.from(schema) → empty frag (original)
// WASM: Fragment.from_array(schema, nodes[]) → frag from array
OrigFragment.from = function (input, schema) {
  // Case 1: null/undefined → empty fragment (need schema)
  if (input == null) {
    const s = schema || (OrigFragment._lastSchema);
    return s ? origFragmentFromArray(s, []) : origFragmentFrom(input);
  }
  // Case 2: array of nodes (including empty array)
  if (Array.isArray(input)) {
    const s = schema || (OrigFragment._lastSchema);
    if (s) return origFragmentFromArray(s, input);
    // Fallback: extract schema from first node
    if (input.length > 0 && input[0] && input[0].type && input[0].type.schema) {
      return origFragmentFromArray(input[0].type.schema, input);
    }
    // Empty array with no schema — need a fallback
    if (input.length === 0 && OrigFragment._lastSchema) {
      return origFragmentFromArray(OrigFragment._lastSchema, []);
    }
  }
  // Case 3: single Node → extract schema from node
  if (input && typeof input === 'object' && input.type && input.type.schema) {
    // Store schema for later use
    OrigFragment._lastSchema = input.type.schema;
    return origFragmentFromArray(input.type.schema, [input]);
  }
  // Case 4: Schema → empty fragment
  // Store schema for later use
  if (input && typeof input.nodes === 'function') {
    OrigFragment._lastSchema = input;
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
// Tag preservation — WASM Node objects lose JS properties when copied.
// Tests attach .tag to nodes; we need to preserve it across operations.
// ---------------------------------------------------------------------------
const _nodeTags = new Map(); // ptr → tag

// Hook Node creation to copy tags from source nodes
const _origTransformConstructor = wasm.Transform_;
const _origTransformProto = _origTransformConstructor.prototype;

// Patch Transform to preserve tags from input doc
const _origBeforeDesc = Object.getOwnPropertyDescriptor(_origTransformProto, 'before');
const _origDocDesc = Object.getOwnPropertyDescriptor(_origTransformProto, 'doc');

function _copyTags(src, dst) {
  if (src && src.__wbg_ptr != null && _nodeTags.has(src.__wbg_ptr)) {
    _nodeTags.set(dst.__wbg_ptr, _nodeTags.get(src.__wbg_ptr));
  }
}

function _getTag(node) {
  if (!node || node.__wbg_ptr == null) return undefined;
  // Direct property (set by test builder)
  if (node.hasOwnProperty && node.hasOwnProperty('tag')) return node.tag;
  // Registry lookup
  return _nodeTags.get(node.__wbg_ptr);
}

function _setTag(node, tag) {
  if (!node) return;
  // Set direct property
  Object.defineProperty(node, 'tag', {
    value: tag,
    writable: true,
    configurable: true,
    enumerable: true,
  });
  // Also store in registry
  if (node.__wbg_ptr != null) {
    _nodeTags.set(node.__wbg_ptr, tag);
  }
}

// Override tag setter on Node prototype to sync with registry
if (wasm.Node && wasm.Node.prototype) {
  // We can't override a non-existent setter, but we can track assignments
  // via a proxy in the test builder. Instead, we'll monkey-patch the
  // property assignment on Node instances done by the test builder.
}

// Patch Node constructor to intercept tag assignments
const OrigNode = wasm.Node;
if (OrigNode && OrigNode.prototype) {
  // No constructor interception needed — we handle it in the tag getter/setter
  // by checking the registry.
}

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

// Helper: copy a property descriptor for camelCase aliases
function _addCamelAlias(proto, camel, snake) {
  if (camel in proto) return;
  const desc = Object.getOwnPropertyDescriptor(proto, snake);
  if (desc) {
    Object.defineProperty(proto, camel, desc);
  }
}

// Node getters (these are critical for the test builder)
if (wasm.Node && wasm.Node.prototype) {
  const nodeGetters = [
    ["nodeSize", "node_size"],
    ["childCount", "child_count"],
    ["firstChild", "first_child"],
    ["lastChild", "last_child"],
    ["textContent", "text_content"],
    ["inlineContent", "inline_content"],
    ["isAtom", "is_atom"],
    ["isBlock", "is_block"],
    ["isInline", "is_inline"],
    ["isLeaf", "is_leaf"],
    ["isText", "is_text"],
    ["isTextblock", "is_textblock"],
  ];
  for (const [camel, snake] of nodeGetters) {
    _addCamelAlias(wasm.Node.prototype, camel, snake);
  }
  // type_ → type already handled
  // toJSON already exists
}

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
    ["nodesBetween", "nodes_between"],
  ];
  for (const [camel, snake] of nodeMethods) {
    if (!wasm.Node.prototype[camel] && typeof wasm.Node.prototype[snake] === "function") {
      wasm.Node.prototype[camel] = wasm.Node.prototype[snake];
    }
  }
}

// ResolvedPos methods
if (wasm.ResolvedPos && wasm.ResolvedPos.prototype) {
  // marks: WASM has it as a getter, but tests call it as .marks()
  const marksDesc = Object.getOwnPropertyDescriptor(wasm.ResolvedPos.prototype, 'marks');
  if (marksDesc && marksDesc.get) {
    Object.defineProperty(wasm.ResolvedPos.prototype, 'marks', {
      value: function () { return marksDesc.get.call(this); },
      configurable: true,
      writable: true,
    });
  }
  if (!wasm.ResolvedPos.prototype.marksAcross && wasm.ResolvedPos.prototype.marks_across) {
    wasm.ResolvedPos.prototype.marksAcross = wasm.ResolvedPos.prototype.marks_across;
  }
  if (!wasm.ResolvedPos.prototype.sameParent && wasm.ResolvedPos.prototype.same_parent) {
    wasm.ResolvedPos.prototype.sameParent = wasm.ResolvedPos.prototype.same_parent;
  }
  if (!wasm.ResolvedPos.prototype.blockRange && wasm.ResolvedPos.prototype.block_range) {
    wasm.ResolvedPos.prototype.blockRange = wasm.ResolvedPos.prototype.block_range;
  }
  // posAtIndex is used by tests
  if (!wasm.ResolvedPos.prototype.posAtIndex && wasm.ResolvedPos.prototype.pos_at_index) {
    wasm.ResolvedPos.prototype.posAtIndex = wasm.ResolvedPos.prototype.pos_at_index;
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
  // toJSON already exists on Step_ prototype (it's a WASM-generated method).
  // Only override it if it doesn't exist and a snake_case alias does.
  if (!wasm.Step_.prototype.toJSON) {
    wasm.Step_.prototype.toJSON = wasm.Step_.prototype.to_json || wasm.Step_.prototype.toJson;
  }
  if (!wasm.Step_.fromJSON) {
    wasm.Step_.fromJSON = wasm.Step_.from_json || wasm.Step_.fromJson;
  }
}

// ---------------------------------------------------------------------------
// Transform bridging
// ---------------------------------------------------------------------------
if (wasm.Transform_) {
  const OrigTransform = wasm.Transform_;
  function BridgedTransform(doc) {
    if (!(this instanceof BridgedTransform)) return new BridgedTransform(doc);
    const tr = Reflect.construct(OrigTransform, [doc], new.target);

    // Preserve tags from input doc across the WASM boundary.
    // The Transform creates new WASM Node objects for before/doc,
    // so JS properties like .tag set by the test builder are lost.
    if (doc && doc.tag) {
      // Override the 'before' getter on this instance to include the tag
      const origBefore = Object.getOwnPropertyDescriptor(OrigTransform.prototype, 'before').get;
      Object.defineProperty(tr, 'before', {
        get() {
          const node = origBefore.call(this);
          if (node && !node.tag) {
            Object.defineProperty(node, 'tag', {
              value: doc.tag,
              writable: true,
              configurable: true,
              enumerable: true,
            });
          }
          return node;
        },
        configurable: true,
      });
    }

    return tr;
  }
  BridgedTransform.prototype = OrigTransform.prototype;

  // Chainable methods (all methods that modify the transform and return void)
  const chainMethods = [
    "addMark", "addNodeMark", "clearIncompatible",
    "delete", "deleteRange", "insert", "join", "lift",
    "maybeStep", "removeMark", "removeMarkType", "removeNodeMark",
    "removeNodeMarkType", "replace", "replaceRange", "replaceRangeWith",
    "replaceWith", "setBlockType", "setDocAttribute", "setNodeAttribute",
    "setNodeMarkup", "split", "step", "wrap",
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

// Slice bridging — add camelCase aliases for snake_case getters/methods
if (wasm.Slice && wasm.Slice.prototype) {
  const sliceAliases = [
    ['openStart', 'open_start'],
    ['openEnd', 'open_end'],
    ['toJSON', 'to_json'],
  ];
  for (const [camel, snake] of sliceAliases) {
    if (!(camel in wasm.Slice.prototype) && (snake in wasm.Slice.prototype)) {
      const desc = Object.getOwnPropertyDescriptor(wasm.Slice.prototype, snake);
      if (desc) {
        Object.defineProperty(wasm.Slice.prototype, camel, desc);
      }
    }
  }
}

// Slice.empty static — creates an empty slice.
// Tests use Slice.empty as a static property (not a function).
if (wasm.Slice) {
  // Override the existing Slice.empty function to also work as a static value.
  // The WASM function takes a schema argument; the JS API uses it as a constant.
  const origEmpty = wasm.Slice.empty;
  if (typeof origEmpty === 'function') {
    // Replace with a getter that returns an empty slice for the last schema
    let _emptySlice = null;
    let _lastSchemaForEmpty = null;
    Object.defineProperty(wasm.Slice, 'empty', {
      get() {
        const s = OrigFragment._lastSchema;
        if (!_emptySlice || _lastSchemaForEmpty !== s) {
          if (s) {
            _emptySlice = origEmpty(s);
            _lastSchemaForEmpty = s;
          }
        }
        return _emptySlice;
      },
      configurable: true,
    });
  }
}

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

  canSplit: wasm.canSplit,
  canJoin: wasm.canJoin,
  joinPoint: wasm.joinPoint,
  insertPoint: wasm.insertPoint,
  dropPoint: wasm.dropPoint,
  liftTarget: wasm.liftTarget,
  findWrapping: wasm.findWrapping,
};

// ContentMatch.parse static (WASM export is content_match_parse)
if (wasm.ContentMatch && !wasm.ContentMatch.parse) {
  const parseFn = wasm.contentMatchParse || wasm.content_match_parse;
  if (parseFn) {
    wasm.ContentMatch.parse = function (expr, nodeTypes) {
      // Extract group info from NodeType values to pass to the Rust parser.
      // The Rust code reads the .group property from each value.
      const groups = {};
      for (const key in nodeTypes) {
        const val = nodeTypes[key];
        let group = '';
        if (val && typeof val === 'object') {
          // Try direct .group property (from plain objects)
          if (typeof val.group === 'string') {
            group = val.group;
          } else if (typeof val.spec === 'function') {
            // NodeType has spec() method that returns a Map
            const spec = val.spec();
            if (spec && typeof spec.get === 'function') {
              group = spec.get('group') || '';
            }
          }
        }
        groups[key] = { group };
      }
      return parseFn(expr, groups);
    };
  }
}

// Slice static
if (wasm.Slice) {
  if (!wasm.Slice.empty && wasm.Slice.prototype) {
    // Add empty as a static if not present
  }
}
