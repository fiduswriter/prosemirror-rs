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

function stripFunctions(obj) {
  if (typeof obj === "function") return undefined;
  if (obj === null || typeof obj !== "object") return obj;
  if (obj instanceof NodeType) return nodeTypeToSpec(obj);
  if (obj instanceof MarkType) return markTypeToSpec(obj);
  if (obj instanceof Schema) return obj;
  if (Array.isArray(obj)) return obj.map(stripFunctions);
  const result = {};
  for (const key in obj) {
    const val = stripFunctions(obj[key]);
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
  map.forEach = function (fn) {
    for (const k in this) {
      if (
        k === "get" ||
        k === "update" ||
        k === "append" ||
        k === "addBefore" ||
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
  const stripped = stripFunctions(spec);
  const schema = new OrigSchema(stripped);
  schema._rawSpec = spec;
  setRawSpec(schema, spec);
  const specNodes = makeOrderedMap({});
  for (const key in schema.nodes) {
    const nt = schema.nodes[key];
    specNodes[key] = nt;
    Object.defineProperty(nt, "schema", {
      get() {
        return schema;
      },
      configurable: true,
      enumerable: false,
    });
  }
  const specMarks = makeOrderedMap({});
  for (const key in schema.marks) {
    const mt = schema.marks[key];
    specMarks[key] = mt;
    Object.defineProperty(mt, "schema", {
      get() {
        return schema;
      },
      configurable: true,
      enumerable: false,
    });
  }
  schema.spec = {
    nodes: specNodes,
    marks: specMarks,
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
  value: function (nodes) {
    if (nodes instanceof Node) nodes = [nodes];
    return Fragment.from(nodes);
  },
  writable: true,
  configurable: true,
  enumerable: true,
});
Object.defineProperty(WrappedFragment, "fromArray", {
  value: Fragment.fromArray.bind(Fragment),
  writable: true,
  configurable: true,
  enumerable: true,
});

// Static properties that napi-rs can't expose directly
Slice.empty = new Slice(WrappedFragment.from([]), 0, 0);

// ---------------------------------------------------------------------------
// toDebugString / leafText support
// ---------------------------------------------------------------------------

const origToString = Node.prototype.toString;
Node.prototype.toString = function () {
  const spec = this.type.schema._rawSpec;
  if (spec && spec.nodes) {
    const nodeSpec = spec.nodes[this.type.name];
    if (nodeSpec && typeof nodeSpec.toDebugString === "function") {
      return nodeSpec.toDebugString(this);
    }
  }
  return origToString.call(this);
};

const origTextContent = Object.getOwnPropertyDescriptor(
  Node.prototype,
  "textContent",
);
Object.defineProperty(Node.prototype, "textContent", {
  get() {
    if (this.isText) {
      const text = this.text;
      return text || "";
    }
    const spec = this.type.schema._rawSpec;
    if (spec && spec.nodes) {
      const nodeSpec = spec.nodes[this.type.name];
      if (nodeSpec && typeof nodeSpec.leafText === "function") {
        return nodeSpec.leafText(this);
      }
    }
    if (this.isLeaf) return "";
    return origTextContent.get.call(this);
  },
  enumerable: true,
  configurable: true,
});

// ---------------------------------------------------------------------------
// Slice.toString shim
// ---------------------------------------------------------------------------

Slice.prototype.toString = function () {
  return (
    "<" +
    this.content.toString() +
    ">(" +
    this.openStart +
    "," +
    this.openEnd +
    ")"
  );
};

// ---------------------------------------------------------------------------
// Fragment.text_between shim
// ---------------------------------------------------------------------------

const origFragmentStr = Fragment.prototype.toString;
Fragment.prototype.toString = function () {
  const inner = [];
  for (let i = 0; i < this.childCount; i++) {
    inner.push(this.child(i).toString());
  }
  return inner.join(", ");
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
