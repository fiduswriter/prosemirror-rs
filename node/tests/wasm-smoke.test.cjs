"use strict";
/**
 * Smoke test for the WASM back-end of prosemirror-rs.
 *
 * Uses the wasm-bindgen native API (snake_case methods, Schema-first
 * factory methods for Fragment, etc.).
 *
 * Run via:  node --test tests/wasm-smoke.test.cjs
 */
const { test } = require("node:test");
const assert = require("node:assert/strict");

const pm = require("../npm/wasm-nodejs/index.js");

const SPEC = JSON.stringify({
  nodes: {
    doc: { content: "block+" },
    paragraph: { content: "inline*", group: "block" },
    text: { group: "inline" },
  },
  marks: {},
});

function makeSchema() {
  return new pm.Schema(SPEC);
}

test("Schema construction", () => {
  const schema = makeSchema();
  assert.ok(schema);
  const nodes = schema.nodes();
  assert.equal(typeof nodes, "object");
  assert.ok(nodes.paragraph);
});

test("Document creation via Fragment.from_array", () => {
  const s = makeSchema();
  const text = s.text("hello", []);
  const paraFrag = pm.Fragment.from_array(s, [text]);
  const para = s.node("paragraph", null, paraFrag, []);
  assert.equal(para.type.name, "paragraph");

  const docFrag = pm.Fragment.from_array(s, [para]);
  const doc = s.node("doc", null, docFrag, []);
  assert.equal(doc.text_content, "hello");
  assert.equal(doc.child_count, 1);
});

test("nodesBetween traversal", () => {
  const s = makeSchema();
  const text = s.text("hey", []);
  const pf = pm.Fragment.from_array(s, [text]);
  const para = s.node("paragraph", null, pf, []);
  const df = pm.Fragment.from_array(s, [para]);
  const doc = s.node("doc", null, df, []);

  const visited = [];
  doc.nodes_between(0, doc.content.size, (node, _pos, _parent, _index) => {
    visited.push(node.type.name);
    return true;
  });
  assert.ok(visited.includes("text"));
});

test("Fragment.from polymorphic", () => {
  const s = makeSchema();
  const empty = pm.Fragment.from(s, null);
  assert.equal(empty.size, 0);

  const text = s.text("hi", []);
  const f = pm.Fragment.from_array(s, [text]);
  assert.equal(f.child_count, 1);
});

test("ContentMatch", () => {
  const s = makeSchema();
  const cm = s.nodes().paragraph.content_match();
  assert.equal(cm.valid_end, true);
});

test("ResolvedPos", () => {
  const s = makeSchema();
  const text = s.text("hi", []);
  const pf = pm.Fragment.from_array(s, [text]);
  const para = s.node("paragraph", null, pf, []);
  const df = pm.Fragment.from_array(s, [para]);
  const doc = s.node("doc", null, df, []);
  const rp = doc.resolve(1);
  assert.ok(rp.pos >= 1);
  assert.ok(rp.depth >= 0);
});

test("StepMap", () => {
  const sm = new pm.StepMap_([2, 0, 4]);
  assert.ok(sm);
  assert.equal(sm.map(0, 1), 0);
  assert.equal(sm.map(3, 1), 7);
});

test("Mapping", () => {
  const m = new pm.Mapping_();
  m.appendMap(new pm.StepMap_([2, 0, 4]), null);
  assert.equal(m.map(0, 1), 0);
});

test("Transform", () => {
  const s = makeSchema();
  const text = s.text("hi", []);
  const pf = pm.Fragment.from_array(s, [text]);
  const para = s.node("paragraph", null, pf, []);
  const df = pm.Fragment.from_array(s, [para]);
  const doc = s.node("doc", null, df, []);
  const tr = new pm.Transform_(doc);
  assert.ok(tr);
  assert.equal(tr.doc.text_content, "hi");
});

test("Free functions", () => {
  const s = makeSchema();
  const t1 = s.text("hello", []);
  const t2 = s.text("world", []);
  const p1 = s.node("paragraph", null, pm.Fragment.from_array(s, [t1]), []);
  const p2 = s.node("paragraph", null, pm.Fragment.from_array(s, [t2]), []);
  const doc = s.node("doc", null, pm.Fragment.from_array(s, [p1, p2]), []);
  const jp = pm.joinPoint(doc, 7);
  assert.ok(typeof jp === "number" || jp === undefined);
});
