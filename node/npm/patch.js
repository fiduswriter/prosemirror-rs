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
  const { Fragment: NativeFragment, Slice, Node } = binding;

  // -- Slice.empty ----------------------------------------------------------
  if (Slice && !Object.getOwnPropertyDescriptor(Slice, "empty")) {
    const fromArray = NativeFragment.fromArray || NativeFragment.from_array;
    Object.defineProperty(Slice, "empty", {
      get() {
        const emptyFrag = fromArray ? fromArray.call(NativeFragment, []) : NativeFragment.from([]);
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

  // -- Fragment.from polymorphic wrapper ------------------------------------
  if (NativeFragment && !NativeFragment._fromWrapped) {
    const nativeFrom = NativeFragment.from.bind(NativeFragment);
    const nativeFromArray = (NativeFragment.fromArray || NativeFragment.from_array);
    const boundFromArray = nativeFromArray ? nativeFromArray.bind(NativeFragment) : null;

    // wasm-bindgen uses snake_case (from_array), napi uses camelCase (fromArray)
    const isWasm = !!NativeFragment.from_array;
    let needsWrapper = !isWasm;

    if (needsWrapper) {
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

      binding.Fragment = WrappedFragment;
    }
    NativeFragment._fromWrapped = true;
  }

  // -- WASM → camelCase getter aliases --------------------------------------
  // wasm-bindgen uses type_ (getter), we want .type for compat.
  if (Node && Node.prototype) {
    const td = Object.getOwnPropertyDescriptor(Node.prototype, "type_");
    if (td && td.get && !Object.getOwnPropertyDescriptor(Node.prototype, "type")) {
      Object.defineProperty(Node.prototype, "type", td);
    }
  }
}

module.exports = { patchStatics, setRawSpec, getRawSpec, schemaSpecs };
