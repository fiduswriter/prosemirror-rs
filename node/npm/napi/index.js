"use strict";

const { existsSync } = require("fs");
const { join } = require("path");

const { platform, arch } = process;

// Map Node.js platform + arch → napi-rs platform triple.
const TRIPLES = {
  "darwin-x64": "prosemirror-rs.darwin-x64.node",
  "darwin-arm64": "prosemirror-rs.darwin-arm64.node",
  "linux-x64": "prosemirror-rs.linux-x64-gnu.node",
  "linux-arm64": "prosemirror-rs.linux-arm64-gnu.node",
  "win32-x64": "prosemirror-rs.win32-x64-msvc.node",
};

const key = `${platform}-${arch}`;
const filename = TRIPLES[key];

let binding;
if (filename) {
  const localPath = join(__dirname, filename);
  if (existsSync(localPath)) {
    binding = require(localPath);
  }
}

// Fall back to WASM if no prebuilt .node binary is available
if (!binding) {
  binding = require("../wasm/index.js");
}

// Merge DOM types (JS-only supplement)
const dom = require("../dom");

module.exports = { ...binding, ...dom };
