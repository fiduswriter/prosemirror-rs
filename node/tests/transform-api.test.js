"use strict";
/**
 * Regression tests for the prosemirror-transform API exposed by the Node.js binding.
 *
 * Covers StepMap, Mapping and Transform — one smoke assertion per method so
 * that any future refactor that silently breaks a binding is caught immediately.
 *
 * Run via:  node --test tests/transform-api.test.js  (from the node/ directory)
 */

const { describe, test } = require("node:test");
const assert = require("node:assert/strict");
const { Schema, StepMap, Mapping, Transform } = require("../index.js");

// ---------------------------------------------------------------------------
// Shared schema / doc helpers
// ---------------------------------------------------------------------------

const SCHEMA = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*", group: "block" },
    blockquote: { content: "block+", group: "block" },
    text: { group: "inline" },
  },
  marks: { em: {} },
});

function makeDoc(...texts) {
  const paras = texts.map((t) =>
    SCHEMA.node("paragraph", {}, t ? [SCHEMA.text(t)] : [])
  );
  return SCHEMA.node("doc", {}, paras);
}

// ---------------------------------------------------------------------------
// Mapping.copy
// ---------------------------------------------------------------------------

describe("Mapping", () => {
  test("copy returns a Mapping instance", () => {
    const m = new Mapping();
    m.appendMap(new StepMap([1, 1, 0]));
    const c = m.copy();
    assert.ok(c instanceof Mapping);
  });

  test("copy is independent of the original", () => {
    const m = new Mapping();
    m.appendMap(new StepMap([1, 1, 0]));
    const c = m.copy();
    const origLen = m.maps.length;
    c.appendMap(new StepMap([3, 1, 1]));
    assert.equal(m.maps.length, origLen, "original should not grow");
    assert.equal(c.maps.length, origLen + 1, "copy should grow");
  });

  test("copy maps the same positions as the original", () => {
    const m = new Mapping();
    m.appendMap(new StepMap([2, 2, 3]));
    const c = m.copy();
    assert.equal(m.map(5), c.map(5));
  });
});

// ---------------------------------------------------------------------------
// Transform.clearIncompatible
// ---------------------------------------------------------------------------

describe("Transform.clearIncompatible", () => {
  test("does not throw on valid content", () => {
    const doc = makeDoc("hello");
    const tr = new Transform(doc);
    assert.doesNotThrow(() =>
      tr.clearIncompatible(1, SCHEMA.nodes.paragraph, false)
    );
  });

  test("is a no-op when content is already valid", () => {
    const doc = makeDoc("hello");
    const tr = new Transform(doc);
    tr.clearIncompatible(1, SCHEMA.nodes.paragraph, false);
    assert.equal(tr.steps.length, 0);
  });

  test("can be called on multiple positions", () => {
    const doc = makeDoc("hello", "world");
    const tr = new Transform(doc);
    tr.clearIncompatible(1, SCHEMA.nodes.paragraph, false);
    tr.clearIncompatible(doc.nodeSize - 3, SCHEMA.nodes.paragraph, false);
    // Neither call should throw and neither should produce spurious steps
    assert.equal(tr.steps.length, 0);
  });
});
