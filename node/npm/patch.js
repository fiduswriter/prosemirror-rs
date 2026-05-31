"use strict";

// ---------------------------------------------------------------------------
// JS-side patches applied to the native binding at load time.
//
// Works with both napi-rs (camelCase) and wasm-bindgen (snake_case) back-ends.
// ---------------------------------------------------------------------------

const schemaSpecs = new WeakMap();

function setRawSpec(schema, spec) {
  schemaSpecs.set(schema, spec);
}

function getRawSpec(schema) {
  return schemaSpecs.get(schema);
}

function patchStatics(binding) {
  const { Fragment: NativeFragment, Slice, Node, Schema, NodeType, MarkType, Mark, ContentMatch } = binding;

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
      binding.replaceStep = function replaceStep(doc, from, to, slice) {
        const tr = new TransformClass(doc);
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
      // napi: spec is already a getter → wrap it
      const origGet = specDesc.get;
      Object.defineProperty(NodeType.prototype, "spec", {
        get() {
          const base = origGet.call(this);
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
}

module.exports = { patchStatics, setRawSpec, getRawSpec, schemaSpecs };
