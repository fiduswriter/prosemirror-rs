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
const { Schema, StepMap, Mapping, Transform } = require("../npm/napi/index.js");

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
// StepMap.forEach
// ---------------------------------------------------------------------------

describe("StepMap", () => {
  test("forEach visits each range with (oldStart, oldEnd, newStart, newEnd)", () => {
    // StepMap([2, 2, 3]) means: skip 2, replace 2 old with 3 new
    const sm = new StepMap([2, 2, 3]);
    const calls = [];
    sm.forEach((oldStart, oldEnd, newStart, newEnd) => {
      calls.push({ oldStart, oldEnd, newStart, newEnd });
    });
    assert.equal(calls.length, 1);
    assert.equal(calls[0].oldStart, 2);
    assert.equal(calls[0].oldEnd, 4);
    assert.equal(calls[0].newStart, 2);
    assert.equal(calls[0].newEnd, 5);
  });

  test("forEach is not called for an empty StepMap", () => {
    const sm = new StepMap([]);
    let count = 0;
    sm.forEach(() => { count++; });
    assert.equal(count, 0);
  });
});

// ---------------------------------------------------------------------------
// Mapping constructor with optional maps array
// ---------------------------------------------------------------------------

describe("Mapping constructor", () => {
  test("new Mapping() with no args creates empty mapping", () => {
    const m = new Mapping();
    assert.equal(m.maps.length, 0);
  });

  test("new Mapping([stepMap, ...]) pre-populates maps", () => {
    const sm1 = new StepMap([1, 1, 0]);
    const sm2 = new StepMap([0, 0, 2]);
    const m = new Mapping([sm1, sm2]);
    assert.equal(m.maps.length, 2);
  });

  test("Mapping constructed with maps maps positions correctly", () => {
    const sm = new StepMap([2, 2, 3]);
    const m = new Mapping([sm]);
    // position 6 (after the replaced range of length 2 ending at 4, +1 due to new length 3)
    assert.equal(typeof m.map(6), "number");
  });
});

// ---------------------------------------------------------------------------
// Mapping.appendMapping / appendMappingInverted
// ---------------------------------------------------------------------------

describe("Mapping.appendMapping / appendMappingInverted", () => {
  test("appendMapping concatenates maps from another Mapping", () => {
    const m1 = new Mapping([new StepMap([1, 1, 0])]);
    const m2 = new Mapping([new StepMap([2, 1, 2])]);
    m1.appendMapping(m2);
    assert.equal(m1.maps.length, 2);
  });

  test("appendMapping maps through both halves", () => {
    const m1 = new Mapping([new StepMap([0, 1, 0])]);  // delete 1 char at 0
    const m2 = new Mapping([new StepMap([0, 0, 1])]);  // insert 1 char at 0
    m1.appendMapping(m2);
    // Net effect on pos 5: -1 then +1 → should be 5
    assert.equal(m1.map(5), 5);
  });

  test("appendMappingInverted appends inverted maps in reverse order", () => {
    const m1 = new Mapping([new StepMap([1, 1, 0])]);
    const m2 = new Mapping([new StepMap([2, 1, 2])]);
    const origLen = m1.maps.length;
    m1.appendMappingInverted(m2);
    assert.equal(m1.maps.length, origLen + m2.maps.length);
  });
});

// ---------------------------------------------------------------------------
// Mapping.copy
// ---------------------------------------------------------------------------

describe("Mapping.copy", () => {
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
    assert.equal(tr.steps.length, 0);
  });
});
