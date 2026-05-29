const bindings = require("../prosemirror-rs.linux-x64-gnu.node");

const schemaRawSpecs = new WeakMap();

function getRawSpec(schema) {
  return schemaRawSpecs.get(schema);
}

function setRawSpec(schema, spec) {
  schemaRawSpecs.set(schema, spec);
}

const {
  Schema,
  Node,
  Fragment,
  Slice,
  ResolvedPos,
  Mark,
  MarkType,
  NodeType,
  ContentMatch,
} = bindings;

// ---------------------------------------------------------------------------
// Global registry of all raw specs for fallback lookups
// ---------------------------------------------------------------------------

const allSpecs = [];

function findSpecByNodeTypeName(name) {
  for (const spec of allSpecs) {
    if (spec.nodes && spec.nodes[name]) {
      return spec;
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Strip functions from schema specs before passing to Rust
// ---------------------------------------------------------------------------

function nodeTypeToSpec(nt) {
  const rawSpec = getRawSpec(nt.schema);
  if (rawSpec && rawSpec.nodes) {
    const spec = rawSpec.nodes[nt.name];
    if (spec) return stripFunctions(spec);
  }
  return {};
}

function markTypeToSpec(mt) {
  const rawSpec = getRawSpec(mt.schema);
  if (rawSpec && rawSpec.marks) {
    const spec = rawSpec.marks[mt.name];
    if (spec) return stripFunctions(spec);
  }
  return {};
}

// Iterate a spec map (plain object or makeOrderedMap) in definition order.
// fn(key, val) is called for every real node/mark key, skipping method keys.
function forEachSpecEntry(map, fn) {
  if (!map) return;
  // makeOrderedMap has a .get() method and a .forEach() that skips its own
  // internal keys (get, update, append, …).
  if (typeof map.get === "function" && typeof map.forEach === "function") {
    map.forEach((key, val) => fn(key, val));
  } else {
    for (const [k, v] of Object.entries(map)) {
      if (typeof v !== "function") fn(k, v);
    }
  }
}

function stripFunctions(obj) {
  if (typeof obj === "function") return undefined;
  if (obj === null || typeof obj !== "object") return obj;
  if (obj instanceof NodeType) return nodeTypeToSpec(obj);
  if (obj instanceof MarkType) return markTypeToSpec(obj);
  if (obj instanceof Schema) return obj;
  if (Array.isArray(obj)) return obj.map(stripFunctions);
  // Merge _rawSpec (original spec) into the object so Rust sees the full spec
  const merged = obj._rawSpec ? Object.assign({}, obj._rawSpec, obj) : obj;
  const result = {};
  for (const key in merged) {
    if (key === "_rawSpec") continue;
    const val = stripFunctions(merged[key]);
    if (val !== undefined) result[key] = val;
  }
  return result;
}

function makeOrderedMap(obj) {
  if (!obj) obj = {};
  const map = Object.create(null);
  Object.assign(map, obj);
  map.get = function (key) {
    return this[key];
  };
  map.update = function (key, value) {
    const copy = makeOrderedMap(this);
    copy[key] = value;
    return copy;
  };
  map.append = function (other) {
    const copy = makeOrderedMap(this);
    for (const k in other) copy[k] = other[k];
    return copy;
  };
  map.addBefore = function (refKey, key, value) {
    const copy = makeOrderedMap({});
    for (const k in this) {
      if (k === refKey) copy[key] = value;
      copy[k] = this[k];
    }
    if (!copy[key]) copy[key] = value;
    return copy;
  };
  map.addToEnd = function (key, value) {
    const copy = makeOrderedMap(this);
    copy[key] = value;
    return copy;
  };
  map.forEach = function (fn) {
    for (const k in this) {
      if (
        k === "get" ||
        k === "update" ||
        k === "append" ||
        k === "addBefore" ||
        k === "addToEnd" ||
        k === "forEach"
      )
        continue;
      fn(k, this[k]);
    }
  };
  return map;
}

const OrigSchema = Schema;
function ShimSchema(spec) {
  // Merge node spec overrides: when a plain object is passed for a node,
  // merge it into the node's existing spec so properties like
  // linebreakReplacement are preserved.
  //
  // The schema property on NodeType objects is a non-enumerable getter,
  // so Object.assign({}, nt) doesn't copy it. We find the real schema by
  // looking at any NodeType entry in spec.nodes (they all have the same schema).
  let schemaHint = null;
  if (spec.nodes) {
    for (const val of Object.values(spec.nodes)) {
      if (val && val.schema) { schemaHint = val.schema; break; }
    }
  }
  const mergedNodes = {};
  if (spec.nodes) {
    for (const [key, val] of Object.entries(spec.nodes)) {
      // Skip internal keys that makeOrderedMap adds
      if (typeof val === "function") { mergedNodes[key] = val; continue; }
      if (val instanceof NodeType) {
        mergedNodes[key] = val;
      } else if (typeof val === "object" && val !== null && !Array.isArray(val)) {
        // Use the plain spec dict as-is.  Do NOT merge with existingNodeSpec —
        // if the caller wants to preserve old properties they must spread them
        // explicitly (e.g. Object.assign({}, schema.spec.nodes.get(key), {...})).
        // Merging would incorrectly add `defining: true` (from the original
        // blockquote spec) when the new spec intentionally leaves it out.
        // stripFunctions will still merge any _rawSpec contained in val.
        mergedNodes[key] = val;
      } else {
        mergedNodes[key] = val;
      }
    }
  }
  const mergedSpec = Object.assign({}, spec, { nodes: mergedNodes });
  const stripped = stripFunctions(mergedSpec);
  const schema = new OrigSchema(stripped);
  schema._rawSpec = spec;
  setRawSpec(schema, spec);
  allSpecs.push(spec);

  // Build cachedNodes and specNodes in *definition order* by iterating
  // spec.nodes (which is ordered) rather than schema.nodes (which is a
  // HashMap and returns keys in hash order).
  //
  // specNodes contains plain spec dicts (not NodeType instances) so that
  //   schema.spec.nodes.get("doc")  →  { content: "block+", attrs: {…} }
  // just like the Python binding does.  This means downstream code like
  //   Object.assign({}, schema.spec.nodes.get("doc"), { content: "heading body" })
  // picks up all spec properties correctly.
  //
  // Keeping the correct definition order also ensures that when this spec is
  // forwarded to a new Schema(), the Rust DynamicSchema builds its "block"
  // group with paragraph before heading, so fillBefore picks paragraph.
  const cachedNodes = {};
  const specNodes = {};
  forEachSpecEntry(spec.nodes, (key, val) => {
    const nt = schema.nodes[key];
    if (!nt) return;
    nt._rawSpec = val;
    Object.defineProperty(nt, "schema", {
      get() { return schema; },
      configurable: true,
      enumerable: false,
    });
    cachedNodes[key] = nt;
    // stripFunctions already knows how to turn NodeType_ / MarkType_ / plain
    // objects into a clean spec dict (it uses nodeTypeToSpec / markTypeToSpec).
    specNodes[key] = stripFunctions(val) || {};
  });
  // Include any schema nodes that were not in spec.nodes (shouldn't normally
  // happen, but be defensive).
  for (const key in schema.nodes) {
    if (cachedNodes[key]) continue;
    const nt = schema.nodes[key];
    nt._rawSpec = undefined;
    Object.defineProperty(nt, "schema", {
      get() { return schema; },
      configurable: true,
      enumerable: false,
    });
    cachedNodes[key] = nt;
    specNodes[key] = {};
  }
  Object.defineProperty(schema, "nodes", {
    get() { return cachedNodes; },
    configurable: true,
    enumerable: true,
  });

  const cachedMarks = {};
  const specMarks = {};
  forEachSpecEntry(spec.marks, (key, val) => {
    const mt = schema.marks[key];
    if (!mt) return;
    mt._rawSpec = val;
    Object.defineProperty(mt, "schema", {
      get() { return schema; },
      configurable: true,
      enumerable: false,
    });
    cachedMarks[key] = mt;
    specMarks[key] = stripFunctions(val) || {};
  });
  for (const key in schema.marks) {
    if (cachedMarks[key]) continue;
    const mt = schema.marks[key];
    mt._rawSpec = undefined;
    Object.defineProperty(mt, "schema", {
      get() { return schema; },
      configurable: true,
      enumerable: false,
    });
    cachedMarks[key] = mt;
    specMarks[key] = {};
  }
  Object.defineProperty(schema, "marks", {
    get() { return cachedMarks; },
    configurable: true,
    enumerable: true,
  });

  schema.spec = {
    nodes: makeOrderedMap(specNodes),
    marks: makeOrderedMap(specMarks),
  };
  return schema;
}
ShimSchema.prototype = OrigSchema.prototype;

// ---------------------------------------------------------------------------
// Upstream-compatible aliases
// ---------------------------------------------------------------------------

Node.fromJSON = Node.fromJson;
Node.prototype.toJSON = Node.prototype.toJson;

// Wrap Fragment because napi-rs static methods are non-configurable
const WrappedFragment = Object.create(Fragment);
Object.setPrototypeOf(WrappedFragment, Fragment);
Object.defineProperty(WrappedFragment, "from", {
  value: function (...args) {
    let nodes;
    if (args.length === 0 || args[0] == null) {
      return Fragment.from([]);
    }
    if (args.length === 1) {
      nodes = args[0];
      if (nodes instanceof Fragment) return nodes;
      if (nodes instanceof Node) nodes = [nodes];
      if (!Array.isArray(nodes)) nodes = [nodes];
    } else {
      nodes = args;
    }
    const frag = Fragment.from(nodes);
    if (nodes && nodes.length > 0 && nodes[0]._rawSpec) {
      frag._rawSpec = nodes[0]._rawSpec;
    }
    return frag;
  },
  writable: true,
  configurable: true,
  enumerable: true,
});
Object.defineProperty(WrappedFragment, "fromArray", {
  value: function (nodes) {
    const frag = Fragment.fromArray.call(Fragment, nodes);
    if (nodes && nodes.length > 0 && nodes[0]._rawSpec) {
      frag._rawSpec = nodes[0]._rawSpec;
    }
    return frag;
  },
  writable: true,
  configurable: true,
  enumerable: true,
});

// Static properties that napi-rs can't expose directly
Slice.empty = new Slice(WrappedFragment.from([]), 0, 0);

// ---------------------------------------------------------------------------
// Schema.nodeFromJSON alias
// ---------------------------------------------------------------------------

Schema.prototype.nodeFromJSON = function (json) {
  return this.nodeFromJson(json);
};

// ---------------------------------------------------------------------------
// Child caching to preserve object identity across nodesBetween / nodeAt
// ---------------------------------------------------------------------------

const fragmentChildCache = new WeakMap();
const origFragmentChild = Fragment.prototype.child;
Fragment.prototype.child = function (index) {
  if (!fragmentChildCache.has(this)) fragmentChildCache.set(this, new Map());
  const cache = fragmentChildCache.get(this);
  if (!cache.has(index)) {
    const child = origFragmentChild.call(this, index);
    if (child && this._rawSpec) child._rawSpec = this._rawSpec;
    cache.set(index, child);
  }
  return cache.get(index);
};

// Cache Node.content so that Node.child() and Fragment.child() share the same
// cache, preserving object identity between nodeAt() and nodesBetween().
const nodeContentCache = new WeakMap();
const origContentDesc = Object.getOwnPropertyDescriptor(
  Node.prototype,
  "content",
);
Object.defineProperty(Node.prototype, "content", {
  get() {
    if (!nodeContentCache.has(this)) {
      nodeContentCache.set(this, origContentDesc.get.call(this));
    }
    return nodeContentCache.get(this);
  },
  configurable: true,
  enumerable: true,
});

Node.prototype.child = function (index) {
  return this.content.child(index);
};

Fragment.prototype.findIndex = function (pos) {
  if (pos == 0) return { index: 0, offset: 0 };
  for (let i = 0, acc = 0; i < this.childCount; i++) {
    let child = this.child(i);
    let end = acc + child.nodeSize;
    if (end > pos) return { index: i, offset: acc };
    acc = end;
  }
  return { index: this.childCount, offset: this.size };
};

Node.prototype.maybeChild = function (index) {
  if (index < 0 || index >= this.childCount) return null;
  return this.child(index);
};

Node.prototype.nodeAt = function (pos) {
  for (let node = this; ; ) {
    let { index, offset } = node.content.findIndex(pos);
    node = node.maybeChild(index);
    if (!node) return null;
    if (offset == pos || node.isText) return node;
    pos -= offset + 1;
  }
};

// ---------------------------------------------------------------------------
// Node.nodesBetween / Node.textBetween
// ---------------------------------------------------------------------------

Node.prototype.nodesBetween = function (from, to, f, startPos = 0) {
  this.content.nodesBetween(from, to, f, startPos, this);
};

Node.prototype.textBetween = function (from, to, blockSeparator, leafText) {
  let text = "",
    first = true;
  this.nodesBetween(
    from,
    to,
    (node, pos) => {
      let nodeText = node.isText
        ? node.text.slice(Math.max(from, pos) - pos, to - pos)
        : !node.isLeaf
          ? ""
          : leafText
            ? typeof leafText === "function"
              ? leafText(node)
              : leafText
            : getLeafText(node);
      if (
        node.isBlock &&
        ((node.isLeaf && nodeText) || node.isTextblock) &&
        blockSeparator
      ) {
        if (first) first = false;
        else text += blockSeparator;
      }
      text += nodeText;
    },
    0,
  );
  return text;
};

// ---------------------------------------------------------------------------
// Fragment.nodesBetween / Fragment.textBetween
// ---------------------------------------------------------------------------

Fragment.prototype.nodesBetween = function (
  from,
  to,
  f,
  nodeStart = 0,
  parent,
) {
  for (let i = 0, pos = 0; pos < to; i++) {
    let child = this.child(i);
    if (!child) break;
    let end = pos + child.nodeSize;
    if (end > from) {
      // Use parent.child(i) when available so that nodeAt() and the
      // callback receive the same wrapper object.
      let nodeToPass =
        parent && typeof parent.child === "function" ? parent.child(i) : child;
      let result = f(nodeToPass, nodeStart + pos, parent || null, i);
      if (result !== false && child.content.size) {
        let start = pos + 1;
        child.nodesBetween(
          Math.max(0, from - start),
          Math.min(child.content.size, to - start),
          f,
          nodeStart + start,
        );
      }
    }
    pos = end;
  }
};

Fragment.prototype.textBetween = function (from, to, blockSeparator, leafText) {
  let text = "",
    first = true;
  this.nodesBetween(
    from,
    to,
    (node, pos) => {
      let nodeText = node.isText
        ? node.text.slice(Math.max(from, pos) - pos, to - pos)
        : !node.isLeaf
          ? ""
          : leafText
            ? typeof leafText === "function"
              ? leafText(node)
              : leafText
            : getLeafText(node);
      if (
        node.isBlock &&
        ((node.isLeaf && nodeText) || node.isTextblock) &&
        blockSeparator
      ) {
        if (first) first = false;
        else text += blockSeparator;
      }
      text += nodeText;
    },
    0,
  );
  return text;
};

// ---------------------------------------------------------------------------
// toDebugString / leafText support
// ---------------------------------------------------------------------------

function attachRawSpec(node, spec) {
  if (node && typeof node === "object") {
    node._rawSpec = spec;
  }
  return node;
}

function normalizeContentArg(content) {
  if (content == null) return content;
  if (content instanceof Node) return [content];
  if (content instanceof Fragment) return content;
  if (Array.isArray(content)) return content;
  return [content];
}

const origSchemaNode = Schema.prototype.node;
Schema.prototype.node = function (typeName, attrs, content, marks) {
  return attachRawSpec(
    origSchemaNode.call(
      this,
      typeName,
      attrs,
      normalizeContentArg(content),
      marks,
    ),
    this._rawSpec,
  );
};

const origSchemaText = Schema.prototype.text;
Schema.prototype.text = function (...args) {
  return attachRawSpec(origSchemaText.apply(this, args), this._rawSpec);
};

// NodeType.spec: return the original JS spec from the schema registry
Object.defineProperty(NodeType.prototype, "spec", {
  get() {
    // Try nodeTypeToSpec (works when .schema is set on this NodeType)
    const fromSchema = nodeTypeToSpec(this);
    if (fromSchema && Object.keys(fromSchema).length > 0) return fromSchema;
    // Fall back: look up by node type name in global allSpecs registry
    const name = this.name;
    for (const spec of allSpecs) {
      if (spec.nodes && spec.nodes[name]) {
        const nodeSpec = spec.nodes[name];
        return stripFunctions(nodeSpec) || {};
      }
    }
    return this._rawSpec || {};
  },
  configurable: true,
});

const origNodeTypeCreate = NodeType.prototype.create;
NodeType.prototype.create = function (attrs, content, marks) {
  const node = origNodeTypeCreate.call(
    this,
    attrs,
    normalizeContentArg(content),
    marks,
  );
  const schema = this.schema;
  if (schema && schema._rawSpec) attachRawSpec(node, schema._rawSpec);
  return node;
};

const origNodeTypeCreateChecked = NodeType.prototype.createChecked;
NodeType.prototype.createChecked = function (attrs, content, marks) {
  const node = origNodeTypeCreateChecked.call(
    this,
    attrs,
    normalizeContentArg(content),
    marks,
  );
  const schema = this.schema;
  if (schema && schema._rawSpec) attachRawSpec(node, schema._rawSpec);
  return node;
};

const origNodeTypeCreateAndFill = NodeType.prototype.createAndFill;
NodeType.prototype.createAndFill = function (attrs, content, marks) {
  const result = origNodeTypeCreateAndFill.call(
    this,
    attrs,
    normalizeContentArg(content),
    marks,
  );
  if (result) {
    const schema = this.schema;
    if (schema && schema._rawSpec) attachRawSpec(result, schema._rawSpec);
  }
  return result;
};

function getLeafText(node) {
  if (node._rawSpec && node._rawSpec.nodes) {
    const nodeSpec = node._rawSpec.nodes[node.type.name];
    if (nodeSpec && typeof nodeSpec.leafText === "function") {
      return nodeSpec.leafText(node);
    }
  }
  // Fallback for nodes that lost _rawSpec during binding-level cloning
  const spec = findSpecByNodeTypeName(node.type.name);
  if (spec && spec.nodes && spec.nodes[node.type.name]) {
    const nodeSpec = spec.nodes[node.type.name];
    if (nodeSpec && typeof nodeSpec.leafText === "function") {
      return nodeSpec.leafText(node);
    }
  }
  return "";
}

function getToDebugString(node) {
  if (node._rawSpec && node._rawSpec.nodes) {
    const nodeSpec = node._rawSpec.nodes[node.type.name];
    if (nodeSpec && typeof nodeSpec.toDebugString === "function") {
      return nodeSpec.toDebugString(node);
    }
  }
  return null;
}

const origToString = Node.prototype.toString;
Node.prototype.toString = function () {
  const custom = getToDebugString(this);
  if (custom !== null) return custom;
  return origToString.call(this);
};

Object.defineProperty(Node.prototype, "textContent", {
  get() {
    if (this.isText) {
      const text = this.text;
      return text || "";
    }
    if (this.isLeaf) {
      const leafText = getLeafText(this);
      if (leafText) return leafText;
    }
    return this.textBetween(0, this.content.size, "");
  },
  enumerable: true,
  configurable: true,
});

// ---------------------------------------------------------------------------
// Slice.toString shim
// ---------------------------------------------------------------------------

Slice.prototype.toString = function () {
  return (
    this.content.toString() +
    "(" +
    this.openStart +
    "," +
    this.openEnd +
    ")"
  );
};

// ---------------------------------------------------------------------------
// Fragment.toString shim
// ---------------------------------------------------------------------------

Fragment.prototype.toStringInner = function () {
  const inner = [];
  for (let i = 0; i < this.childCount; i++) {
    const child = this.child(i);
    const custom = getToDebugString(child);
    inner.push(custom !== null ? custom : child.toString());
  }
  return inner.join(", ");
};

Fragment.prototype.toString = function () {
  return "<" + this.toStringInner() + ">";
};

// ---------------------------------------------------------------------------
// Mark.sameSet
// ---------------------------------------------------------------------------

Mark.sameSet = function (a, b) {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (!a[i].eq(b[i])) return false;
  }
  return true;
};

ContentMatch.parse = function (expr, nodeTypes) {
  for (const key in nodeTypes) {
    const val = nodeTypes[key];
    if (val instanceof NodeType) {
      return bindings.contentMatchParse(expr, val.schema);
    }
  }
  throw new Error("No node types found in nodeTypes object");
};

module.exports = {
  Schema: ShimSchema,
  Node,
  Fragment: WrappedFragment,
  Slice,
  ResolvedPos,
  Mark,
  MarkType,
  NodeType,
  ContentMatch,
  ReplaceError: class ReplaceError extends Error {},
};
