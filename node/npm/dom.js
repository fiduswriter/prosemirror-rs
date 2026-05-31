"use strict";

// DOM-related types from prosemirror-model.
// These are pure JavaScript — they use browser DOM APIs and cannot be
// implemented in Rust.

const { createDOMSerializer } = require("./to-dom.js");
const { createDOMParser } = require("./from-dom.js");

class ReplaceError extends Error {
  constructor(message) {
    super(message);
    this.name = "ReplaceError";
  }
}

function createDOMTypes(binding) {
  return {
    ReplaceError,
    DOMSerializer: createDOMSerializer(binding),
    DOMParser: createDOMParser(binding),
  };
}

module.exports = { ReplaceError, createDOMTypes };
