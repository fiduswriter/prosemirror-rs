"use strict";

// CJS wrapper that loads the WASM module and applies patches.
// Used as an alternative to the napi .node binding for testing.

const wasm = require("./prosemirror_rs_wasm.js");

// Apply JS-side patches (same as napi/index.js)
const patch = require("../patch.js");
patch.patchStatics(wasm);

// Merge DOM types
const dom = require("../dom.js");

module.exports = { ...wasm, ...dom, ...patch };
