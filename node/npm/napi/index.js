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

if (!binding) {
  throw new Error(
    `prosemirror-rs: unsupported platform ${key}. ` +
    "Use the ESM entry point (works with bundlers) or install on a supported platform."
  );
}

// Apply JS-side patches (Fragment.from, Slice.empty, Node.toJSON, NodeType.spec)
const patch = require("../patch");
patch.patchStatics(binding);

// Merge DOM types (ReplaceError, DOMSerializer, etc.)
const dom = require("../dom");

module.exports = { ...binding, ...dom, ...patch };
