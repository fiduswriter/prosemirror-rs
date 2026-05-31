"use strict";

// Re-exports all model symbols from the correct back-end.
// When used as a subpath export (prosemirror-rs/model), the package.json
// exports map handles dispatch.  When required directly, we detect the
// environment ourselves.

let pkg;
try {
  // Node.js: try native addon first
  pkg = require("./napi/index.js");
} catch {
  // Browser / WASM fallback
  try {
    pkg = require("./wasm/index.js");
  } catch {
    throw new Error(
      "prosemirror-rs: no back-end available. " +
      "Install a prebuilt binary or build from source."
    );
  }
}

module.exports = {
  // Schema & types
  Schema: pkg.Schema,
  Node: pkg.Node,
  NodeType: pkg.NodeType,
  Fragment: pkg.Fragment,
  Slice: pkg.Slice,
  ResolvedPos: pkg.ResolvedPos,
  NodeRange: pkg.NodeRange,
  Mark: pkg.Mark,
  MarkType: pkg.MarkType,
  ContentMatch: pkg.ContentMatch,

  // Errors
  ReplaceError: pkg.ReplaceError,

  // DOM types (JS-only)
  DOMSerializer: pkg.DOMSerializer,
  DOMParser: pkg.DOMParser,
  DOMOutputSpec: pkg.DOMOutputSpec,
  ParseRule: pkg.ParseRule,
  TagParseRule: pkg.TagParseRule,
  StyleParseRule: pkg.StyleParseRule,
  GenericParseRule: pkg.GenericParseRule,
  ParseOptions: pkg.ParseOptions,

  // Utility
  contentMatchParse: pkg.contentMatchParse,
};
