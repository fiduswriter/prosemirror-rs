"use strict";

// DOM-related types from prosemirror-model.
// These are pure JavaScript — they use browser DOM APIs and cannot be
// implemented in Rust.  They will be compiled from the vendored TypeScript
// sources in vendor/ (Step 3.2 of the plan).

class ReplaceError extends Error {
  constructor(message) {
    super(message);
    this.name = "ReplaceError";
  }
}

module.exports = {
  ReplaceError,
  // DOMSerializer, DOMParser, DOMOutputSpec, ParseRule, etc.
  // — compiled separately from vendor/to-dom.ts and vendor/from-dom.ts
};
