"use strict";

// DOM-related types from prosemirror-model.
// These are pure JavaScript — they use browser DOM APIs and cannot be
// implemented in Rust.  They are vendored from the upstream
// prosemirror-model@1.25.7 source.

// Placeholder — real implementations will be in vendor/to-dom.js and
// vendor/from-dom.js, compiled from the upstream TypeScript sources.

class ReplaceError extends Error {
  constructor(message) {
    super(message);
    this.name = "ReplaceError";
  }
}

module.exports = {
  ReplaceError,
  // DOMSerializer, DOMParser, etc. will be added when vendor files are built.
};
