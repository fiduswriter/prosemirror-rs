"use strict";

// ---------------------------------------------------------------------------
// JS-side patches applied to the native binding at load time.
//
// Works with both napi-rs (camelCase) and wasm-bindgen (snake_case) back-ends.
// ---------------------------------------------------------------------------

const schemaSpecs = new WeakMap();
const _allSpecs = [];

function setRawSpec(schema, spec) {
  schemaSpecs.set(schema, spec);
  _allSpecs.push(spec);
}

function getRawSpec(schema) {
  return schemaSpecs.get(schema);
}

function findSpecByNodeTypeName(name) {
  for (const spec of _allSpecs) {
    if (spec.nodes && spec.nodes[name]) return spec;
  }
  return null;
}

function findSpecByMarkTypeName(name) {
  for (const spec of _allSpecs) {
    if (spec.marks && spec.marks[name]) return spec;
  }
  return null;
}

function patchStatics(binding) {
  const { Fragment: NativeFragment, Slice, Node, Schema, NodeType, MarkType, Mark, ContentMatch } = binding;

  // -- WASM snake_case → camelCase aliases for class exports ----------------
  if (!binding.Step && binding.Step_) binding.Step = binding.Step_;
  if (!binding.StepMap && binding.StepMap_) binding.StepMap = binding.StepMap_;
  if (!binding.Mapping && binding.Mapping_) binding.Mapping = binding.Mapping_;
  if (!binding.MapResult && binding.MapResult_) binding.MapResult = binding.MapResult_;
  if (!binding.Transform && binding.Transform_) binding.Transform = binding.Transform_;
  if (!binding.NodeRange && binding.NodeRange_) binding.NodeRange = binding.NodeRange_;

  // -- Mark.none ------------------------------------------------------------
  if (Mark && !Mark.none) {
    Mark.none = [];
  }

  // -- Step constructors (ReplaceStep, ReplaceAroundStep, etc.) -------------
  const StepClass = binding.Step || binding.Step_;
  if (StepClass) {
    function ReplaceStep(from, to, slice, structure) {
      return StepClass.replace(from, to, slice, structure);
    }
    ReplaceStep.prototype = StepClass.prototype;
    binding.ReplaceStep = ReplaceStep;

    function ReplaceAroundStep(from, to, gapFrom, gapTo, slice, insert, structure) {
      return StepClass.replaceAround(from, to, gapFrom, gapTo, slice, insert, structure);
    }
    ReplaceAroundStep.prototype = StepClass.prototype;
    binding.ReplaceAroundStep = ReplaceAroundStep;

    function AddMarkStep(from, to, mark) {
      return StepClass.addMark(from, to, mark);
    }
    AddMarkStep.prototype = StepClass.prototype;
    binding.AddMarkStep = AddMarkStep;

    function RemoveMarkStep(from, to, mark) {
      return StepClass.removeMark(from, to, mark);
    }
    RemoveMarkStep.prototype = StepClass.prototype;
    binding.RemoveMarkStep = RemoveMarkStep;

    function AddNodeMarkStep(pos, mark) {
      return StepClass.addNodeMark(pos, mark);
    }
    AddNodeMarkStep.prototype = StepClass.prototype;
    binding.AddNodeMarkStep = AddNodeMarkStep;

    function RemoveNodeMarkStep(pos, mark) {
      return StepClass.removeNodeMark(pos, mark);
    }
    RemoveNodeMarkStep.prototype = StepClass.prototype;
    binding.RemoveNodeMarkStep = RemoveNodeMarkStep;

    function AttrStep(pos, attr, value) {
      return StepClass.attr(pos, attr, value);
    }
    AttrStep.prototype = StepClass.prototype;
    binding.AttrStep = AttrStep;

    function DocAttrStep(attr, value) {
      return StepClass.docAttr(attr, value);
    }
    DocAttrStep.prototype = StepClass.prototype;
    binding.DocAttrStep = DocAttrStep;

    // replaceStep function shim
    const TransformClass = binding.Transform || binding.Transform_;
    if (TransformClass) {
      // Wrap constructor with a Proxy to store original doc for identity checks.
      // Upstream expects tr.doc === doc when no steps have been applied.
      // A plain function wrapper breaks ES class inheritance (instanceof), so
      // we use a Proxy that intercepts construct without changing the prototype chain.
      const OrigTransform = TransformClass;
      const ProxyTransform = new Proxy(OrigTransform, {
        construct(target, args, newTarget) {
          const instance = Reflect.construct(target, args, newTarget);
          if (args[0]) instance._origDoc = args[0];
          return instance;
        }
      });
      if (binding.Transform === OrigTransform) binding.Transform = ProxyTransform;
      if (binding.Transform_ === OrigTransform) binding.Transform_ = ProxyTransform;

      binding.replaceStep = function replaceStep(doc, from, to, slice) {
        const tr = new ProxyTransform(doc);
        tr.replace(from, to !== undefined ? to : from, slice !== undefined ? slice : Slice.empty);
        for (const step of tr.steps) {
          if (step instanceof ReplaceStep) return step;
        }
        return null;
      };
    }
  }

  // -- Schema.cached + raw spec storage -------------------------------------
  if (Schema) {
    const OrigSchema = Schema;
    function PatchedSchema(spec) {
      const specObj = typeof spec === "string" ? JSON.parse(spec) : spec;
      let instance;
      if (typeof spec === "string") {
        instance = new OrigSchema(spec);
      } else {
        // napi accepts objects; wasm accepts strings. Try object first.
        try {
          instance = new OrigSchema(spec);
        } catch (_) {
          instance = new OrigSchema(JSON.stringify(spec));
        }
      }
      instance.cached = {};
      setRawSpec(instance, specObj);

      // Ensure node/mark type .schema returns this instance so spec merging
      // (which uses WeakMap identity) works correctly.
      if (instance.nodes) {
        for (const name of Object.keys(instance.nodes)) {
          const nt = instance.nodes[name];
          if (nt) {
            try {
              Object.defineProperty(nt, "schema", {
                get() { return instance; },
                configurable: true,
                enumerable: false,
              });
            } catch (_) {}
            if (specObj && specObj.nodes && specObj.nodes[name]) {
              nt._rawSpec = specObj.nodes[name];
            }
          }
        }
      }
      if (instance.marks) {
        for (const name of Object.keys(instance.marks)) {
          const mt = instance.marks[name];
          if (mt) {
            try {
              Object.defineProperty(mt, "schema", {
                get() { return instance; },
                configurable: true,
                enumerable: false,
              });
            } catch (_) {}
            if (specObj && specObj.marks && specObj.marks[name]) {
              mt._rawSpec = specObj.marks[name];
            }
          }
        }
      }

      return instance;
    }
    PatchedSchema.prototype = OrigSchema.prototype;
    Object.setPrototypeOf(PatchedSchema, OrigSchema);
    for (const key of Object.getOwnPropertyNames(OrigSchema)) {
      if (key !== "prototype" && key !== "length" && key !== "name" && key !== "arguments" && key !== "caller") {
        try {
          PatchedSchema[key] = OrigSchema[key];
        } catch (_) {
          // Skip read-only properties like arguments
        }
      }
    }
    binding.Schema = PatchedSchema;
  }

  // -- NodeType.spec getter (merge raw JS functions) ------------------------
  if (NodeType && NodeType.prototype) {
    const specDesc = Object.getOwnPropertyDescriptor(NodeType.prototype, "spec");
    if (specDesc && !specDesc.get) {
      // wasm: spec is a method → replace with getter
      const origSpec = specDesc.value;
      Object.defineProperty(NodeType.prototype, "spec", {
        get() {
          const base = origSpec.call(this);
          const raw = getRawSpec(this.schema);
          if (raw && raw.nodes && raw.nodes[this.name]) {
            const rawSpec = raw.nodes[this.name];
            for (const key of ["toDOM", "parseDOM", "leafText", "whitespace", "linebreakReplacement", "defining", "definingForContent", "isolating", "selectable", "draggable", "code", "atom", "marks", "group", "content", "attrs"]) {
              if (rawSpec[key] !== undefined) base[key] = rawSpec[key];
            }
          }
          return base;
        },
        configurable: true,
      });
    } else if (specDesc && specDesc.get) {
      // napi/wasm: spec is already a getter → wrap it
      const origGet = specDesc.get;
      Object.defineProperty(NodeType.prototype, "spec", {
        get() {
          const base = origGet.call(this);
          let raw = getRawSpec(this.schema);
          if (!raw && this._rawSpec) {
            raw = { nodes: { [this.name]: this._rawSpec } };
          }
          if (!raw) {
            raw = findSpecByNodeTypeName(this.name);
            if (raw) raw = { nodes: { [this.name]: raw.nodes[this.name] } };
          }
          if (raw && raw.nodes && raw.nodes[this.name]) {
            const rawSpec = raw.nodes[this.name];
            for (const key of ["toDOM", "parseDOM", "leafText", "whitespace", "linebreakReplacement", "defining", "definingForContent", "isolating", "selectable", "draggable", "code", "atom", "marks", "group", "content", "attrs"]) {
              if (rawSpec[key] !== undefined) base[key] = rawSpec[key];
            }
          }
          return base;
        },
        configurable: true,
      });
    }
  }

  // -- MarkType.spec getter (merge raw JS functions) ------------------------
  if (MarkType && MarkType.prototype) {
    const specDesc = Object.getOwnPropertyDescriptor(MarkType.prototype, "spec");
    if (specDesc && !specDesc.get) {
      const origSpec = specDesc.value;
      Object.defineProperty(MarkType.prototype, "spec", {
        get() {
          const base = origSpec.call(this);
          const raw = getRawSpec(this.schema);
          if (raw && raw.marks && raw.marks[this.name]) {
            const rawSpec = raw.marks[this.name];
            for (const key of ["toDOM", "parseDOM", "spanning", "excludes", "group", "attrs"]) {
              if (rawSpec[key] !== undefined) base[key] = rawSpec[key];
            }
          }
          return base;
        },
        configurable: true,
      });
    } else if (specDesc && specDesc.get) {
      const origGet = specDesc.get;
      Object.defineProperty(MarkType.prototype, "spec", {
        get() {
          const base = origGet.call(this);
          let raw = getRawSpec(this.schema);
          if (!raw && this._rawSpec) {
            raw = { marks: { [this.name]: this._rawSpec } };
          }
          if (!raw) {
            raw = findSpecByMarkTypeName(this.name);
            if (raw) raw = { marks: { [this.name]: raw.marks[this.name] } };
          }
          if (raw && raw.marks && raw.marks[this.name]) {
            const rawSpec = raw.marks[this.name];
            for (const key of ["toDOM", "parseDOM", "spanning", "excludes", "group", "attrs"]) {
              if (rawSpec[key] !== undefined) base[key] = rawSpec[key];
            }
          }
          return base;
        },
        configurable: true,
      });
    }
  }

  // -- NodeType.defaultAttrs getter -----------------------------------------
  const attrsDesc = NodeType && NodeType.prototype && Object.getOwnPropertyDescriptor(NodeType.prototype, "attrs");
  if (attrsDesc && !Object.getOwnPropertyDescriptor(NodeType.prototype, "defaultAttrs")) {
    Object.defineProperty(NodeType.prototype, "defaultAttrs", {
      get() { return attrsDesc.get ? attrsDesc.get.call(this) : attrsDesc.value.call(this); },
      configurable: true,
    });
  }

  // -- NodeType.create wrappers (handle optional args + array content) ------
  if (NodeType && NodeType.prototype) {
    const methods = ['create', 'createChecked', 'createAndFill'];
    for (const method of methods) {
      const orig = NodeType.prototype[method];
      if (orig) {
        NodeType.prototype[method] = function (attrs, content, marks) {
          let frag = content;
          if (Array.isArray(content)) {
            frag = NativeFragment.fromArray(content);
          } else if (content != null && typeof content === 'object' && content.type) {
            frag = NativeFragment.fromArray([content]);
          } else if (content === undefined) {
            frag = null;
          }
          return orig.call(this, attrs || null, frag, marks || []);
        };
      }
    }
  }

  // -- Schema.text wrapper (optional marks) ---------------------------------
  if (Schema && Schema.prototype && Schema.prototype.text) {
    const origText = Schema.prototype.text;
    Schema.prototype.text = function (text, marks) {
      return origText.call(this, text, marks || []);
    };
  }

  // -- ContentMatch.edge helper ---------------------------------------------
  if (ContentMatch && ContentMatch.prototype && !ContentMatch.prototype.edge) {
    ContentMatch.prototype.edge = function (i) {
      return { type: this.edgeType(i), next: this.edgeMatch(i) };
    };
  }

  // -- ContentMatch.fillBefore wrapper (optional start_index) ---------------
  if (ContentMatch && ContentMatch.prototype && ContentMatch.prototype.fillBefore) {
    const origFillBefore = ContentMatch.prototype.fillBefore;
    ContentMatch.prototype.fillBefore = function (after, to_end, start_index) {
      return origFillBefore.call(this, after, to_end, start_index !== undefined ? start_index : 0);
    };
  }

  // -- Fragment.from polymorphic wrapper (napi + wasm) ----------------------
  if (NativeFragment && !NativeFragment._fromWrapped) {
    const nativeFrom = NativeFragment.from.bind(NativeFragment);
    const nativeFromArray = NativeFragment.fromArray || NativeFragment.from_array;
    const boundFromArray = nativeFromArray ? nativeFromArray.bind(NativeFragment) : null;
    const isWasm = NativeFragment.from.length === 2;

    // Keep a fallback schema for empty-fragment creation
    let lastSchema = null;

    function inferSchema(input) {
      if (input && input.type && input.type.schema) return input.type.schema;
      if (Array.isArray(input) && input.length > 0 && input[0] && input[0].type && input[0].type.schema) {
        return input[0].type.schema;
      }
      return lastSchema;
    }

    function updateLastSchema(input) {
      if (input && input.type && input.type.schema) lastSchema = input.type.schema;
      else if (Array.isArray(input) && input.length > 0 && input[0] && input[0].type && input[0].type.schema) {
        lastSchema = input[0].type.schema;
      }
    }

    if (isWasm) {
      // WASM: Fragment.from(schema, input) → bridge to JS API
      Object.defineProperty(NativeFragment, "from", {
        value: function (input, maybeInput) {
          // If called with two args, delegate to raw wasm API
          if (arguments.length >= 2) {
            return nativeFrom(input, maybeInput);
          }
          if (input == null || (Array.isArray(input) && input.length === 0)) {
            const s = inferSchema(input);
            if (s) return boundFromArray(s, []);
            throw new Error("Fragment.from() requires a schema when empty. Create a Schema first.");
          }
          if (input instanceof NativeFragment) {
            return input;
          }
          if (!Array.isArray(input) && input.type !== undefined) {
            updateLastSchema(input);
            return boundFromArray(input.type.schema, [input]);
          }
          updateLastSchema(input);
          return boundFromArray(input[0].type.schema, input);
        },
        writable: true,
        configurable: true,
      });

      if (boundFromArray) {
        const wrappedFromArray = function (schema, nodes) {
          if (nodes === undefined && Array.isArray(schema)) {
            nodes = schema;
            schema = inferSchema(nodes);
          }
          if (!schema) throw new Error("Fragment.fromArray requires a schema");
          updateLastSchema(nodes);
          return boundFromArray(schema, nodes);
        };
        Object.defineProperty(NativeFragment, "fromArray", {
          value: wrappedFromArray,
          writable: true,
          configurable: true,
        });
        // Also expose snake_case alias for backward compat
        Object.defineProperty(NativeFragment, "from_array", {
          value: wrappedFromArray,
          writable: true,
          configurable: true,
        });
      }

      // Fragment.empty — lazy getter using last known schema
      Object.defineProperty(NativeFragment, "empty", {
        get() {
          const s = lastSchema;
          if (!s) throw new Error("Fragment.empty requires a schema. Create a Schema first.");
          return boundFromArray(s, []);
        },
        configurable: true,
      });
    } else {
      // NAPI: Fragment.from(input) already works, just make it polymorphic
      const WrappedFragment = function (...args) {
        if (new.target) {
          return Reflect.construct(NativeFragment, args, new.target);
        }
        return Reflect.construct(NativeFragment, args, WrappedFragment);
      };
      WrappedFragment.prototype = NativeFragment.prototype;
      Object.setPrototypeOf(WrappedFragment, NativeFragment);

      Object.defineProperty(WrappedFragment, "from", {
        value: function (input) {
          if (input == null) return nativeFrom([]);
          if (input instanceof NativeFragment || input instanceof WrappedFragment) return input;
          if (!Array.isArray(input) && input.type !== undefined) {
            return nativeFrom([input]);
          }
          return nativeFrom(input);
        },
        writable: true,
        configurable: true,
      });
      if (boundFromArray) {
        Object.defineProperty(WrappedFragment, "fromArray", {
          value: boundFromArray,
          writable: true,
          configurable: true,
        });
      }
      // Fragment.empty for napi — create lazily
      Object.defineProperty(WrappedFragment, "empty", {
        get() {
          return nativeFrom([]);
        },
        configurable: true,
      });

      binding.Fragment = WrappedFragment;
    }
    NativeFragment._fromWrapped = true;
  }

  // -- Slice.empty ----------------------------------------------------------
  if (Slice && !Object.getOwnPropertyDescriptor(Slice, "empty")) {
    const isWasm = !!(NativeFragment && NativeFragment.from_array);
    Object.defineProperty(Slice, "empty", {
      get() {
        let emptyFrag;
        if (isWasm && NativeFragment.empty) {
          emptyFrag = NativeFragment.empty;
        } else {
          const fromArray = NativeFragment.fromArray || NativeFragment.from_array;
          emptyFrag = fromArray ? fromArray.call(NativeFragment, []) : NativeFragment.from([]);
        }
        return new Slice(emptyFrag, 0, 0);
      },
      configurable: true,
      enumerable: true,
    });
  }

  // -- Node.fromJSON / Node.prototype.toJSON --------------------------------
  if (Node) {
    const fromJson = Node.fromJson || Node.from_json;
    if (fromJson && !Node.fromJSON) Node.fromJSON = fromJson;

    // WASM doesn't export a static Node.fromJSON — synthesise it from schema.nodeFromJSON
    if (!Node.fromJSON && Schema && Schema.prototype && Schema.prototype.nodeFromJSON) {
      Node.fromJSON = function fromJSON(schema, json) {
        return schema.nodeFromJSON(json);
      };
    }

    const toJson = Node.prototype && (Node.prototype.toJson || Node.prototype.to_json);
    if (toJson && Node.prototype && !Node.prototype.toJSON) {
      Node.prototype.toJSON = toJson;
    }
  }

  // -- WASM → camelCase getter aliases --------------------------------------
  if (Node && Node.prototype) {
    const td = Object.getOwnPropertyDescriptor(Node.prototype, "type_");
    if (td && td.get && !Object.getOwnPropertyDescriptor(Node.prototype, "type")) {
      Object.defineProperty(Node.prototype, "type", td);
    }
  }

  // -- ResolvedPos.blockRange default argument (upstream: blockRange(to = this))
  const ResolvedPos = binding.ResolvedPos || binding.ResolvedPos_;
  if (ResolvedPos && ResolvedPos.prototype && ResolvedPos.prototype.blockRange) {
    const origBlockRange = ResolvedPos.prototype.blockRange;
    ResolvedPos.prototype.blockRange = function (other) {
      // WASM binding takes a required &ResolvedPos; default to `this` when omitted.
      return origBlockRange.call(this, other || this);
    };
  }

  // -- Transform mutating methods should return `this` for chaining -----------
  const TransformClass = binding.Transform || binding.Transform_;
  if (TransformClass && TransformClass.prototype) {
    const chainable = [
      "step", "replace", "replaceWith", "replace_with", "delete", "insert",
      "replaceRange", "replace_range", "replaceRangeWith", "replace_range_with",
      "deleteRange", "delete_range", "lift", "join", "wrap", "split",
      "setBlockType", "set_block_type", "setNodeMarkup", "set_node_markup",
      "setNodeAttribute", "set_node_attribute", "setDocAttribute", "set_doc_attribute",
      "addNodeMark", "add_node_mark", "removeNodeMark", "remove_node_mark",
      "addMark", "add_mark", "removeMark", "remove_mark",
      "clearIncompatible", "clear_incompatible",
    ];
    for (const name of chainable) {
      const orig = TransformClass.prototype[name];
      if (orig && typeof orig === "function") {
        TransformClass.prototype[name] = function (...args) {
          const ret = orig.apply(this, args);
          // If the original returns undefined (wasm void), return `this`.
          // If it returns a value (e.g., an error or result), preserve it.
          return ret === undefined ? this : ret;
        };
      }
    }
  }

  // -- Preserve Node identity across resolve() / .doc accessors ---------------
  // Upstream stores a reference to the document in ResolvedPos; our wasm
  // binding clones it on every access, which breaks `!=` checks like
  // `selection.$from.doc != tr.doc` in prosemirror-state's setSelection.
  if (Node && Node.prototype && Node.prototype.resolve && ResolvedPos) {
    const origResolve = Node.prototype.resolve;
    const origResolveNoCache = Node.prototype.resolveNoCache;
    const origDoc = Object.getOwnPropertyDescriptor(ResolvedPos.prototype, "doc");
    const origMin = ResolvedPos.prototype.min;
    const origMax = ResolvedPos.prototype.max;
    if (origResolve && origDoc && origDoc.get) {
      Node.prototype.resolve = function (pos) {
        const rp = origResolve.call(this, pos);
        rp._sourceDoc = this;
        return rp;
      };
      if (origResolveNoCache) {
        Node.prototype.resolveNoCache = function (pos) {
          const rp = origResolveNoCache.call(this, pos);
          rp._sourceDoc = this;
          return rp;
        };
      }
      if (origMin) {
        ResolvedPos.prototype.min = function (other) {
          const rp = origMin.call(this, other);
          rp._sourceDoc = this._sourceDoc || other._sourceDoc;
          return rp;
        };
      }
      if (origMax) {
        ResolvedPos.prototype.max = function (other) {
          const rp = origMax.call(this, other);
          rp._sourceDoc = this._sourceDoc || other._sourceDoc;
          return rp;
        };
      }
      const origNode = ResolvedPos.prototype.node;
      const origParent = Object.getOwnPropertyDescriptor(ResolvedPos.prototype, "parent");
      if (origNode) {
        ResolvedPos.prototype.node = function (depth) {
          const d = depth === undefined ? this.depth : depth;
          if (d === 0 && this._sourceDoc !== undefined) {
            return this._sourceDoc;
          }
          return origNode.call(this, depth);
        };
      }
      if (origParent && origParent.get) {
        Object.defineProperty(ResolvedPos.prototype, "parent", {
          get() {
            if (this.depth === 0 && this._sourceDoc !== undefined) {
              return this._sourceDoc;
            }
            return origParent.get.call(this);
          },
          configurable: true,
        });
      }
      Object.defineProperty(ResolvedPos.prototype, "doc", {
        get() {
          if (this._sourceDoc !== undefined) {
            return this._sourceDoc;
          }
          return origDoc.get.call(this);
        },
        configurable: true,
      });
    }
  }

  // -- Cache Transform.doc / before so empty transforms preserve identity ----
  // The WASM binding clones the document on every access. Upstream code
  // expects tr.doc === doc (and tr.before === doc) when no steps have been
  // applied. The constructor wrapper above stores _origDoc for this.
  if (TransformClass && TransformClass.prototype) {
    const origTransformDoc = Object.getOwnPropertyDescriptor(TransformClass.prototype, "doc");
    if (origTransformDoc && origTransformDoc.get) {
      Object.defineProperty(TransformClass.prototype, "doc", {
        get() {
          const stepCount = this.steps ? this.steps.length : 0;
          if (stepCount === 0 && this._origDoc !== undefined) {
            return this._origDoc;
          }
          if (!this._docCache || this._docCacheStepCount !== stepCount) {
            this._docCache = origTransformDoc.get.call(this);
            this._docCacheStepCount = stepCount;
          }
          return this._docCache;
        },
        configurable: true,
      });
    }
    const origTransformBefore = Object.getOwnPropertyDescriptor(TransformClass.prototype, "before");
    if (origTransformBefore && origTransformBefore.get) {
      Object.defineProperty(TransformClass.prototype, "before", {
        get() {
          const stepCount = this.steps ? this.steps.length : 0;
          if (stepCount === 0 && this._origDoc !== undefined) {
            return this._origDoc;
          }
          return origTransformBefore.get.call(this);
        },
        configurable: true,
      });
    }
  }
}

module.exports = { patchStatics, setRawSpec, getRawSpec, schemaSpecs };
