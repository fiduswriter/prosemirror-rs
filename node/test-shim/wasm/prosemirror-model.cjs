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
  // Assign schema ID and register for leafText/toDebugString lookup
  this.__schemaId = _nextSchemaId++;
  _specsById[this.__schemaId] = spec && spec.nodes || {};
  // Store schema ID on the WASM schema instance for raw patch access
  try { s.__schemaId = this.__schemaId; } catch (_) {}
  if (spec && spec.nodes && typeof spec === 'object') {
    _allRawSpecs.push({ nodes: spec.nodes, id: this.__schemaId });
  }
}
BridgedSchema.prototype = Object.create(OrigSchema.prototype);

// nodes / marks as getters (wasm has methods)
// Also proxy __wbg_ptr so WASM interop sees the underlying pointer
Object.defineProperty(BridgedSchema.prototype, "__wbg_ptr", {
  get() { return this._wasm ? this._wasm.__wbg_ptr : undefined; },
  configurable: true,
});
Object.defineProperty(BridgedSchema.prototype, "nodes", {
  get() {
    const nodes = this._wasm.nodes;
    const sid = this.__schemaId;
    return new Proxy(nodes, {
      get(target, prop) {
        const val = target[prop];
        if (val && val.__wbg_ptr != null) {
          try { val.__schemaId = sid; } catch (_) {}
        }
        return val;
      }
    });
  },
  configurable: true,
});
Object.defineProperty(BridgedSchema.prototype, "marks", {
  get() { return this._wasm.marks; },
  configurable: true,
});
Object.defineProperty(BridgedSchema.prototype, "topNodeType", {
  get() { return this._wasm.topNodeType(); },
  configurable: true,
});

// node: accepts arrays as content (converts to Fragment), also handles single Node
BridgedSchema.prototype.node = function (typeName, attrs, content, marks) {
  let frag = content;
  if (Array.isArray(content)) {
    frag = wasm.Fragment.fromArray(this._wasm, content);
  } else if (content != null && typeof content === 'object' && content.type) {
    // Single Node — wrap in Fragment
    frag = wasm.Fragment.fromArray(this._wasm, [content]);
  } else if (content == null) {
    frag = null;
  }
  // Bypass the OrigSchema.prototype.node patch to avoid double-cloning.
  return _tagNode(origSchemaNode.call(this._wasm, typeName, attrs || null, frag, _cloneMarksForWasm(this._wasm, marks)), this.__schemaId);
};

// text: marks defaults to empty array, ensure WASM Mark instances
BridgedSchema.prototype.text = function (text, marks) {
  // Bypass the OrigSchema.prototype.text patch to avoid double-cloning.
  // We clone here, then pass directly to the original WASM function.
  return _tagNode(origSchemaText.call(this._wasm, text, _cloneMarksForWasm(this._wasm, marks)), this.__schemaId);
};

// mark
BridgedSchema.prototype.mark = function (typeName, attrs) {
  return this._wasm.mark(typeName, attrs || null);
};

// nodeFromJson / markFromJson — tests call nodeFromJSON/markFromJSON (camelCase)
BridgedSchema.prototype.nodeFromJson = function (json) {
  return this._wasm.nodeFromJSON(json);
};
BridgedSchema.prototype.nodeFromJSON = BridgedSchema.prototype.nodeFromJson;
BridgedSchema.prototype.markFromJson = function (json) {
  return this._wasm.markFromJSON(json);
};
BridgedSchema.prototype.markFromJSON = BridgedSchema.prototype.markFromJson;

// Static fromJSON
OrigSchema.fromJSON = function (spec) {
  return new BridgedSchema(spec);
};

// Add nodes/marks getters to WASM Schema so doc.type.schema.nodes works
// (the test accesses .nodes as a property, but WASM exports it as a method)
(function() {
  var nodesDesc = Object.getOwnPropertyDescriptor(OrigSchema.prototype, "nodes");
  var marksDesc = Object.getOwnPropertyDescriptor(OrigSchema.prototype, "marks");
  if (nodesDesc && typeof nodesDesc.get === "function") {
    var _origNodes = nodesDesc.get;
    Object.defineProperty(OrigSchema.prototype, "nodes", {
      get: function() { return _origNodes.call(this); },
      configurable: true
    });
    OrigSchema.prototype._origNodes = _origNodes;
  }
  if (marksDesc && typeof marksDesc.get === "function") {
    var _origMarks = marksDesc.get;
    Object.defineProperty(OrigSchema.prototype, "marks", {
      get: function() { return _origMarks.call(this); },
      configurable: true
    });
    OrigSchema.prototype._origMarks = _origMarks;
  }
})();

// ---------------------------------------------------------------------------
// Node cloning helper — prevents WASM pointer exhaustion
//
// wasm-bindgen's Vec<Node> conversion calls Node.__unwrap() →
// __destroy_into_raw() which sets __wbg_ptr = 0, consuming the node.
// To safely reuse nodes across multiple calls, we deep-clone them
// by re-creating fresh WASM Node instances from type name + attrs + content.
// ---------------------------------------------------------------------------
function _cloneNode(node) {
  if (!node || node.__wbg_ptr == null || node.__wbg_ptr === 0) return node;
  const schema = node.type && node.type.schema;
  if (!schema) return node;
  const wasmSchema = schema._wasm || schema;
  const attrs = typeof node.attrs === 'function' ? node.attrs() : node.attrs;
  const schemaId = node.__schemaId;
  if (node.isText) {
    const marks = node.marks;
    return _tagNode(origSchemaText.call(wasmSchema, node.text, marks), schemaId);
  }
  let frag = null;
  if (node.content && node.content.childCount > 0) {
    const children = [];
    for (let i = 0; i < node.content.childCount; i++) {
      children.push(_cloneNode(node.content.child(i)));
    }
    frag = origFragmentFromArray(wasmSchema, children);
  }
  return _tagNode(origSchemaNode.call(wasmSchema, node.type.name, attrs, frag, node.marks), schemaId);
}

function _cloneNodes(nodes) {
  if (!nodes || !Array.isArray(nodes)) return nodes;
  return nodes.map(n => _cloneNode(n));
}

// ---------------------------------------------------------------------------
// Mark cloning helper — prevents WASM pointer exhaustion
//
// wasm-bindgen's Vec<Mark> conversion calls Mark.__unwrap() →
// __destroy_into_raw() which sets __wbg_ptr = 0, consuming the mark.
// To safely reuse marks across multiple calls, we clone them by
// re-creating fresh WASM Mark instances from type.name + attrs.
// ---------------------------------------------------------------------------
function _cloneMarksForWasm(schemaObj, marks) {
  if (!marks || !Array.isArray(marks)) return [];
  if (!schemaObj) return marks.slice(); // Can't clone without schema — return copy as-is
  const cloned = [];
  for (const m of marks) {
    if (!m) continue;
    // Get type name from napi (.type.name) or WASM (.type_) patterns
    const typeObj = m.type || m.type_;
    if (typeObj && typeObj.name) {
      const attrs = typeof m.attrs === 'function' ? m.attrs() : (m.attrs || null);
      cloned.push(schemaObj.mark(typeObj.name, attrs));
    } else if (typeof m.__wbg_ptr === 'number' && m.__wbg_ptr > 0) {
      cloned.push(m);
    }
  }
  return cloned;
}

// Patch raw WASM Schema methods
const origSchemaText = OrigSchema.prototype.text;
const origSchemaNode = OrigSchema.prototype.node;
// Patch raw WASM Schema methods to clone marks (avoid pointer exhaustion)
OrigSchema.prototype.text = function (text, marks) {
  return _tagNode(origSchemaText.call(this, text, _cloneMarksForWasm(this, marks)), this.__schemaId);
};
OrigSchema.prototype.node = function (typeName, attrs, content, marks) {
  let frag = content;
  if (Array.isArray(content)) frag = OrigFragment.fromArray(this, _cloneNodes(content));
  else if (content != null && typeof content === 'object' && content.type)
    frag = OrigFragment.fromArray(this, [_cloneNode(content)]);
  return _tagNode(origSchemaNode.call(this, typeName, attrs, frag || null, _cloneMarksForWasm(this, marks)), this.__schemaId);
};

// Patch raw WASM NodeType.create to handle arrays as content + clone marks
const OrigNodeType = wasm.NodeType;
if (OrigNodeType && OrigNodeType.prototype) {
  const origCreate = OrigNodeType.prototype.create;
  if (origCreate) {
    OrigNodeType.prototype.create = function (attrs, content, marks) {
      let frag = content;
      if (Array.isArray(content)) {
        const s = this.schema;
        frag = OrigFragment.fromArray(s, _cloneNodes(content));
      } else if (content != null && typeof content === 'object' && content.type) {
        const s = this.schema;
        frag = OrigFragment.fromArray(s, [_cloneNode(content)]);
      }
      return _tagNode(origCreate.call(this, attrs, frag || null, _cloneMarksForWasm(this.schema, marks)), this.__schemaId);
    };
  }
  // Also patch createChecked and createAndFill
  ['createChecked', 'createAndFill'].forEach(method => {
    const orig = OrigNodeType.prototype[method];
    if (orig) {
      OrigNodeType.prototype[method] = function (attrs, content, marks) {
        let frag = content;
        if (Array.isArray(content)) {
          const s = this.schema;
          frag = OrigFragment.fromArray(s, _cloneNodes(content));
        } else if (content != null && typeof content === 'object' && content.type) {
          const s = this.schema;
          frag = OrigFragment.fromArray(s, [_cloneNode(content)]);
        } else if (content == null) {
          frag = null;
        }
        return _tagNode(orig.call(this, attrs, frag, _cloneMarksForWasm(this.schema, marks)), this.__schemaId);
      };
    }
  });

  // Patch NodeType methods that take mark arrays
  ['allowsMarks', 'allowedMarks'].forEach(method => {
    const orig = OrigNodeType.prototype[method];
    if (orig) {
      OrigNodeType.prototype[method] = function (marks) {
        return orig.call(this, _cloneMarksForWasm(this.schema, marks));
      };
    }
  });
}

// Patch Mark static methods (sameSet, setFrom)
const OrigMark = wasm.Mark;
if (OrigMark) {
  const origSameSet = OrigMark.sameSet;
  if (origSameSet) {
    OrigMark.sameSet = function (a, b) {
      // Need schemas from marks in a or b to clone
      const findSchema = (arr) => {
        for (const m of (arr || [])) {
          const typeObj = m && (m.type || m.type_);
          if (typeObj && typeObj.schema) return typeObj.schema;
        }
        return null;
      };
      const schema = findSchema(a) || findSchema(b);
      return origSameSet(
        schema ? _cloneMarksForWasm(schema, a) : a,
        schema ? _cloneMarksForWasm(schema, b) : b
      );
    };
  }

  const origSetFrom = OrigMark.setFrom;
  if (origSetFrom) {
    OrigMark.setFrom = function (schema, marks) {
      return origSetFrom(schema, _cloneMarksForWasm(schema, marks));
    };
  }

  // Patch Mark instance methods that take mark arrays
  if (OrigMark.prototype) {
    ['addToSet', 'removeFromSet', 'isInSet'].forEach(method => {
      const orig = OrigMark.prototype[method];
      if (orig) {
        OrigMark.prototype[method] = function (set) {
          const schema = (this.type_ && this.type_.schema) || null;
          return orig.call(this, _cloneMarksForWasm(schema, set));
        };
      }
    });
  }
}

// Patch MarkType instance methods that take mark arrays
const OrigMarkType = wasm.MarkType;
if (OrigMarkType && OrigMarkType.prototype) {
  ['removeFromSet', 'isInSet'].forEach(method => {
    const orig = OrigMarkType.prototype[method];
    if (orig) {
      OrigMarkType.prototype[method] = function (marks) {
        return orig.call(this, _cloneMarksForWasm(this.schema, marks));
      };
    }
  });
}



// rawSpec storage — global registry indexed by node type name,
// since WASM schema wrappers are not stable across property accesses.
let _nextSchemaId = 1;
const _specsById = {};
const _allRawSpecs = [];
const schemaSpecs = new WeakMap();

function _tagNode(node, schemaId) {
  if (node && node.__wbg_ptr != null && node.__wbg_ptr !== 0) {
    try { node.__schemaId = schemaId; } catch (_) { Object.defineProperty(node, '__schemaId', { value: schemaId }); }
  }
  return node;
}
BridgedSchema.prototype.__getRawSpec = function () {
  return this._rawSpec || schemaSpecs.get(this._wasm);
};
BridgedSchema.prototype.__setRawSpec = function (spec) {
  this._rawSpec = spec;
  schemaSpecs.set(this._wasm, spec);
  if (spec && spec.nodes) {
    _allRawSpecs.push({ nodes: spec.nodes });
  }
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
            addBefore(place, key, value) {
                      const updated = Object.assign({}, base);
                      // Insert before the given key by rebuilding from keys
                      const entries = [];
                      for (const k of Object.keys(updated)) {
                        if (k === place) entries.push([key, value]);
                        entries.push([k, updated[k]]);
                      }
                      if (!updated[place]) entries.push([key, value]);
                      // Rebuild base
                      const result = {};
                      for (const [k, v] of entries) result[k] = v;
                      return makeNodes(result);
                    },
                    addAfter(place, key, value) {
                      const updated = Object.assign({}, base);
                      const entries = [];
                      for (const k of Object.keys(updated)) {
                        entries.push([k, updated[k]]);
                        if (k === place) entries.push([key, value]);
                      }
                      if (!updated[place]) entries.push([key, value]);
                      const result = {};
                      for (const [k, v] of entries) result[k] = v;
                      return makeNodes(result);
                    },
                    addToEnd(name, value) {
          const updated = Object.assign({}, base);
          updated[name] = value;
          return makeNodes(updated);
        },
        addToStart(name, value) {
          const updated = Object.assign({}, base);
          const result = {};
          result[name] = value;
          Object.assign(result, updated);
          return makeNodes(result);
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
        addBefore(place, key, value) {
          const updated = Object.assign({}, base);
          const entries = [];
          for (const k of Object.keys(updated)) {
            if (k === place) entries.push([key, value]);
            entries.push([k, updated[k]]);
          }
          if (!updated[place]) entries.push([key, value]);
          const result = {};
          for (const [k, v] of entries) result[k] = v;
          return makeMarks(result);
        },
        addAfter(place, key, value) {
                      const updated = Object.assign({}, base);
                      const entries = [];
                      for (const k of Object.keys(updated)) {
                        entries.push([k, updated[k]]);
                        if (k === place) entries.push([key, value]);
                      }
                      if (!updated[place]) entries.push([key, value]);
          const result = {};
          for (const [k, v] of entries) result[k] = v;
          return makeMarks(result);
        },
        addToEnd(name, value) {
          const updated = Object.assign({}, base);
          updated[name] = value;
          return makeMarks(updated);
        },
        addToStart(name, value) {
          const updated = Object.assign({}, base);
          const result = {};
          result[name] = value;
          Object.assign(result, updated);
          return makeMarks(result);
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
const origFragmentFromArray = OrigFragment.fromArray.bind(OrigFragment);

// Fragment.from: bridge the JS API to the WASM API.
// JS: Fragment.from(schema) → empty frag
// JS: Fragment.from(node) → frag from single node
// JS: Fragment.from(nodes[]) → frag from array
// WASM: Fragment.from(schema) → empty frag (original)
// WASM: Fragment.fromArray(schema, nodes[]) → frag from array
OrigFragment.from = function (input, schema) {
  // Case 1: null/undefined → empty fragment (need schema)
  if (input == null) {
    const s = schema || (OrigFragment._lastSchema);
    return s ? origFragmentFromArray(s, []) : origFragmentFrom(input);
  }
  // Case 2: array of nodes (including empty array)
  if (Array.isArray(input)) {
    const s = schema || (OrigFragment._lastSchema);
    if (s) return origFragmentFromArray(s, _cloneNodes(input));
    // Fallback: extract schema from first node
    if (input.length > 0 && input[0] && input[0].type && input[0].type.schema) {
      return origFragmentFromArray(input[0].type.schema, _cloneNodes(input));
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
    return origFragmentFromArray(input.type.schema, [_cloneNode(input)]);
  }
  // Case 4: Schema → empty fragment
  // Store schema for later use
  if (input && typeof input.nodes === 'function') {
    OrigFragment._lastSchema = input;
  }
  return origFragmentFrom(input);
};
OrigFragment.fromArray = function (schema, nodes) {
  // If called with only nodes (no schema), extract schema from first node
  if (nodes === undefined && Array.isArray(schema)) {
    const arr = schema;
    if (arr.length > 0 && arr[0] && arr[0].type && arr[0].type.schema) {
      return origFragmentFromArray(arr[0].type.schema, _cloneNodes(arr));
    }
    if (OrigFragment._lastSchema) {
      return origFragmentFromArray(OrigFragment._lastSchema, _cloneNodes(arr));
    }
  }
   // Direct passthrough to WASM Fragment.fromArray(schema, nodes)
   return origFragmentFromArray(schema, Array.isArray(nodes) ? _cloneNodes(nodes) : nodes);
};
OrigFragment._wasmBridged = true;

// Fragment.prototype.toString — override Object default
if (OrigFragment.prototype) {
  OrigFragment.prototype.toString = function () {
    const parts = [];
    for (let i = 0; i < this.childCount; i++) {
      parts.push(this.child(i).toString());
    }
    return '<' + parts.join(', ') + '>';
  };
}

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

  // Patch Node.mark() to clone marks before passing to WASM (avoid pointer exhaustion)
  const origNodeMark = OrigNode.prototype.mark;
  if (origNodeMark) {
    OrigNode.prototype.mark = function (marks) {
      const schema = (this.type_ && this.type_.schema) || null;
      return origNodeMark.call(this, _cloneMarksForWasm(schema, marks));
    };
  }

  // Patch Node.hasMarkup() to clone marks before passing to WASM
  const origHasMarkup = OrigNode.prototype.hasMarkup;
  if (origHasMarkup) {
    OrigNode.prototype.hasMarkup = function (type_, attrs, marks) {
      const schema = (this.type_ && this.type_.schema) || null;
      return origHasMarkup.call(this, type_, attrs,
        marks ? _cloneMarksForWasm(schema, marks) : marks);
    };
  }
}

// ---------------------------------------------------------------------------
// Node bridging
// ---------------------------------------------------------------------------
// Node.fromJSON (WASM exports as fromJson)
wasm.Node.fromJSON = wasm.Node.fromJson;
// Node.prototype.toJSON for JSON.stringify support
if (wasm.Node.prototype && wasm.Node.prototype.toJson) {
  wasm.Node.prototype.toJSON = wasm.Node.prototype.toJson;
}

// Patch Node.slice and Node.cut to handle undefined `to` parameter.
// The JS tests pass `doc.tag.b` which may be undefined (meaning "to end").
// WASM doesn't support optional numeric params like napi does.
if (wasm.Node && wasm.Node.prototype) {
  const origSlice = wasm.Node.prototype.slice;
  if (origSlice) {
    wasm.Node.prototype.slice = function (from, to, includeParents) {
      if (to === undefined) to = this.content.size;
      if (includeParents) {
        // Include parent nodes: resolve at depth 0, compute content and opens.
        // We use the underlying Rust slice() and then adjust openStart/openEnd.
        // Get content by cutting the root level manually.
        const rpFrom = this.resolve(from);
        const rpTo = this.resolve(to);
        const openStart = rpFrom.depth;
        const openEnd = rpTo.depth;
        // Get content at depth 0
        const rootNode = rpFrom.node(0);
        const startInRoot = rpFrom.pos - rpFrom.start(0);
        const endInRoot = rpTo.pos - rpTo.start(0);
        const contentFragment = rootNode.content.cut(startInRoot, endInRoot);
        return new wasm.Slice(contentFragment, openStart, openEnd);
      }
      return origSlice.call(this, from, to);
    };
  }
  const origCut = wasm.Node.prototype.cut;
  if (origCut) {
    wasm.Node.prototype.cut = function (from, to) {
      if (to === undefined) to = this.content.size;
      return origCut.call(this, from, to);
    };
  }
}

// ---------------------------------------------------------------------------
// Node.textBetween / Fragment.textBetween — JS-side override to support
// leafText functions and correct block separator behavior.
// The WASM native versions only handle string leafText.
// ---------------------------------------------------------------------------
function _getNodeSpecCallback(node, key) {
  if (!node || !node.type) return undefined;
  const name = node.type.name;
  // Try node-tagged schema ID first (most reliable)
  if (node.__schemaId) {
    const nodes = _specsById[node.__schemaId];
    return (nodes && nodes[name] && nodes[name][key]) || undefined;
  }
  // Fall back to global search (for nodes without schema ID)
  // Only use if exactly one schema defines this callback
  let result = undefined;
  let count = 0;
  for (const spec of _allRawSpecs) {
    if (spec.nodes && spec.nodes[name] && spec.nodes[name][key]) {
      result = spec.nodes[name][key];
      count++;
    }
  }
  return count === 1 ? result : undefined;
}

function _getLeafText(node) {
  const fn = _getNodeSpecCallback(node, 'leafText');
  if (typeof fn === 'function') {
    try { return fn(node); } catch (_) { return ''; }
  }
  return '';
}

if (wasm.Node && wasm.Node.prototype) {
  const origNodesBetween = wasm.Node.prototype.nodesBetween;
  if (origNodesBetween) {
    wasm.Node.prototype.textBetween = function (from, to, blockSeparator, leafText) {
      let text = '', first = true;
      origNodesBetween.call(this, from, to, (node, pos) => {
        let nodeText = node.isText
          ? node.text.slice(Math.max(from, pos) - pos, to - pos)
          : !node.isLeaf
            ? ''
            : leafText
              ? typeof leafText === 'function'
                ? leafText(node)
                : leafText
              : _getLeafText(node);
        if (node.isBlock && ((node.isLeaf && nodeText) || node.isTextblock) && blockSeparator) {
          if (first) first = false;
          else text += blockSeparator;
        }
        text += nodeText;
      });
      return text;
    };
  }
}

if (wasm.Fragment && wasm.Fragment.prototype) {
  const origFragNodesBetween = wasm.Fragment.prototype.nodesBetween;
  if (origFragNodesBetween) {
    wasm.Fragment.prototype.textBetween = function (from, to, blockSeparator, leafText) {
      let text = '', first = true;
      origFragNodesBetween.call(this, from, to, (node, pos) => {
        let nodeText = node.isText
          ? node.text.slice(Math.max(from, pos) - pos, to - pos)
          : !node.isLeaf
            ? ''
            : leafText
              ? typeof leafText === 'function'
                ? leafText(node)
                : leafText
              : _getLeafText(node);
        if (node.isBlock && ((node.isLeaf && nodeText) || node.isTextblock) && blockSeparator) {
          if (first) first = false;
          else text += blockSeparator;
        }
        text += nodeText;
      });
      return text;
    };
  }
}

// Patch Node.prototype.toString to support toDebugString from NodeSpec
if (wasm.Node && wasm.Node.prototype) {
  const origToString = wasm.Node.prototype.toString;
  if (origToString) {
    wasm.Node.prototype.toString = function () {
      const toDebugString = _getNodeSpecCallback(this, 'toDebugString');
      if (typeof toDebugString === 'function') {
        try { return toDebugString(this); } catch (_) {}
      }
      return origToString.call(this);
    };
  }
}

// Patch Node.prototype.textContent to support leafText from NodeSpec
if (wasm.Node && wasm.Node.prototype) {
  const textContentDesc = Object.getOwnPropertyDescriptor(wasm.Node.prototype, 'textContent');
  if (textContentDesc && textContentDesc.get) {
    const origTextContent = textContentDesc.get;
    Object.defineProperty(wasm.Node.prototype, 'textContent', {
      get: function () {
        if (this.isLeaf) {
          const lt = _getLeafText(this);
          if (lt) return lt;
          return origTextContent.call(this);
        }
        if (this.isText) return this.text;
        // Non-leaf, non-text: recursively collect text from children
        let text = '';
        if (this.content) {
          for (let i = 0; i < this.content.childCount; i++) {
            text += this.content.child(i).textContent;
          }
        }
        return text;
      },
      configurable: true,
    });
  }
}

// Node.prototype.type alias (WASM exports as type_, but JS expects type)
if (wasm.Node && wasm.Node.prototype && ('type_' in wasm.Node.prototype)) {
  Object.defineProperty(wasm.Node.prototype, 'type', Object.getOwnPropertyDescriptor(wasm.Node.prototype, 'type_'));
}

// Node.prototype.attrs getter (WASM exports attrs as a function; tests access it as a property)
if (wasm.Node && wasm.Node.prototype) {
  const attrsDesc = Object.getOwnPropertyDescriptor(wasm.Node.prototype, 'attrs');
  if (attrsDesc && typeof attrsDesc.value === 'function') {
    const origAttrsFn = attrsDesc.value;
    Object.defineProperty(wasm.Node.prototype, 'attrs', {
      get: function() { return origAttrsFn.call(this); },
      configurable: true,
    });
  }
}

// Mark.prototype.type alias (WASM exports as type_, but JS expects type)
if (wasm.Mark && wasm.Mark.prototype && ('type_' in wasm.Mark.prototype)) {
  Object.defineProperty(wasm.Mark.prototype, 'type', Object.getOwnPropertyDescriptor(wasm.Mark.prototype, 'type_'));
}

// ---------------------------------------------------------------------------
// CamelCase aliases — now handled natively by WASM js_name attributes.
// Only kept where WASM exports camelCase but JS tooling expects toJSON.
// ---------------------------------------------------------------------------

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
  // nodeBefore / nodeAfter: WASM has them as methods, but tests access them as properties
  const nodeBeforeDesc = Object.getOwnPropertyDescriptor(wasm.ResolvedPos.prototype, 'nodeBefore');
  const nodeAfterDesc = Object.getOwnPropertyDescriptor(wasm.ResolvedPos.prototype, 'nodeAfter');
  if (nodeBeforeDesc && typeof nodeBeforeDesc.get === 'function') {
    const origNodeBefore = nodeBeforeDesc.get;
    Object.defineProperty(wasm.ResolvedPos.prototype, 'nodeBefore', {
      get: function () { return origNodeBefore.call(this); },
      configurable: true,
    });
  }
  if (nodeAfterDesc && typeof nodeAfterDesc.get === 'function') {
    const origNodeAfter = nodeAfterDesc.get;
    Object.defineProperty(wasm.ResolvedPos.prototype, 'nodeAfter', {
      get: function () { return origNodeAfter.call(this); },
      configurable: true,
    });
  }
  // All other methods are now natively camelCase via WASM js_name
}

// ContentMatch — now natively camelCase via js_name, nothing to alias

// NodeType — now natively camelCase via js_name, nothing to alias

// ---------------------------------------------------------------------------
// Step bridging
// ---------------------------------------------------------------------------
if (wasm.Step_) {
  wasm.Step = wasm.Step_;
  // toJSON and fromJSON are native via js_name = "toJSON" / "fromJSON"
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

// Slice — openStart/openEnd are native; toJSON needs mapping from toJson
if (wasm.Slice && wasm.Slice.prototype) {
  if (!('toJSON' in wasm.Slice.prototype) && ('toJson' in wasm.Slice.prototype)) {
    const desc = Object.getOwnPropertyDescriptor(wasm.Slice.prototype, 'toJson');
    if (desc) {
      Object.defineProperty(wasm.Slice.prototype, 'toJSON', desc);
    }
  }
  // Slice.toString: build debug string from content and openStart/openEnd.
  // WASM binding exports toDebugString on Node, so we use that.
  wasm.Slice.prototype.toString = function () {
    const os = this.openStart || 0;
    const oe = this.openEnd || 0;
    const content = this.content;
    let contentStr = '';
    // Build content string by iterating children
    const parts = [];
    for (let i = 0; i < content.childCount; i++) {
      parts.push(content.child(i).toString());
    }
    contentStr = parts.join(', ');
    return '<' + contentStr + '>(' + os + ',' + oe + ')';
  };
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

// ContentMatch.parse static (WASM export is now contentMatchParse)
if (wasm.ContentMatch && !wasm.ContentMatch.parse) {
  const parseFn = wasm.contentMatchParse;
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
          } else if (val.spec) {
            // NodeType has spec property/getter that returns a Map or plain object
            const spec = typeof val.spec === 'function' ? val.spec() : val.spec;
            if (spec) {
              if (typeof spec.get === 'function') {
                group = spec.get('group') || '';
              } else if (typeof spec.group === 'string') {
                group = spec.group;
              } else if (spec.group) {
                group = String(spec.group);
              }
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
