"use strict";

// ---------------------------------------------------------------------------
// JS-side patches applied to the native binding at load time.
//
// These bridge gaps where napi-rs can't directly express the upstream API:
//   - Fragment.from needs type disambiguation (napi Either3 limitation)
//   - Slice.empty is a static getter that needs a Fragment
//   - Node.fromJSON / toJSON are camelCase aliases
//   - NodeType.spec stores JS functions (toDOM, parseDOM) that Rust can't hold
//
// Both napi/index.js and wasm/index.js call patchStatics(binding) after
// loading their respective back-end.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// NodeType.spec — JS-side spec lookup
//
// WeakMap from schema to raw spec — one entry per schema, zero per-NodeType
// overhead, GC-friendly.
// ---------------------------------------------------------------------------

const schemaSpecs = new WeakMap();

function setRawSpec(schema, spec) {
  schemaSpecs.set(schema, spec);
}

function getRawSpec(schema) {
  return schemaSpecs.get(schema);
}

// ---------------------------------------------------------------------------
// patchStatics(binding)
// ---------------------------------------------------------------------------

function patchStatics(binding) {
  const { Fragment: NativeFragment, Slice, Node } = binding;

  // -- Slice.empty ----------------------------------------------------------
  if (Slice && !Object.getOwnPropertyDescriptor(Slice, "empty")) {
    Object.defineProperty(Slice, "empty", {
      get() {
        const emptyFrag = NativeFragment.from([]);
        return new Slice(emptyFrag, 0, 0);
      },
      configurable: true,
      enumerable: true,
    });
  }

  // -- Node.fromJSON / Node.prototype.toJSON --------------------------------
  if (Node) {
    if (!Node.fromJSON) Node.fromJSON = Node.fromJson;
    if (Node.prototype && !Node.prototype.toJSON) {
      Node.prototype.toJSON = Node.prototype.toJson;
    }
  }

  // -- Fragment.from polymorphic wrapper ------------------------------------
  // The native from() uses napi Either3 and can't distinguish a bare Node
  // from an array at the JS level.  We wrap Fragment in a constructor-
  // compatible function that normalises bare Node → [Node].
  if (NativeFragment && !NativeFragment._fromWrapped) {
    const nativeFrom = NativeFragment.from.bind(NativeFragment);
    const nativeFromArray = NativeFragment.fromArray.bind(NativeFragment);

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
    Object.defineProperty(WrappedFragment, "fromArray", {
      value: nativeFromArray,
      writable: true,
      configurable: true,
    });

    binding.Fragment = WrappedFragment;
    NativeFragment._fromWrapped = true;
  }
}

// ---------------------------------------------------------------------------

module.exports = { patchStatics, setRawSpec, getRawSpec, schemaSpecs };
