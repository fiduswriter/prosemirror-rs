const bindings = require("../prosemirror-rs.linux-x64-gnu.node");

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

function stripFunctions(obj) {
  if (typeof obj === "function") return undefined;
  if (obj === null || typeof obj !== "object") return obj;
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
  schema.spec = {
    nodes: makeOrderedMap(spec.nodes),
    marks: makeOrderedMap(spec.marks),
  };
  return schema;
}
ShimSchema.prototype = OrigSchema.prototype;

// ---------------------------------------------------------------------------
// Upstream-compatible aliases
// ---------------------------------------------------------------------------

Node.fromJSON = Node.fromJson;

// Static properties that napi-rs can't expose directly
Slice.empty = new Slice(Fragment.from([]), 0, 0);

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
// Fragment.text_between shim
// ---------------------------------------------------------------------------

const origFragmentStr = Fragment.prototype.toString;
Fragment.prototype.toString = function () {
  const inner = [];
  for (let i = 0; i < this.childCount; i++) {
    inner.push(this.child(i).toString());
  }
  return "<" + inner.join(", ") + ">";
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

module.exports = {
  Schema: ShimSchema,
  Node,
  Fragment,
  Slice,
  ResolvedPos,
  Mark,
  MarkType,
  NodeType,
  ContentMatch,
  ReplaceError: class ReplaceError extends Error {},
};
