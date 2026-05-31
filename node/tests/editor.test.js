"use strict";
/**
 * Tests for the prosemirror-rs Node.js bindings.
 *
 * Covers: construction, docJson, applyStep, applyStepsJson, applySteps,
 * reset, version, error handling, atomicity, and schema caching.
 *
 * Run via:  npm test  (from the node/ directory)
 * Or:       node --test tests/  (after building the native addon)
 */
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { Editor } = require("../npm/napi/index.js");

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SCHEMA = JSON.stringify({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*", group: "block" },
    text: { group: "inline" },
  },
  marks: { strong: {}, em: {} },
});

const DOC = JSON.stringify({
  type: "doc",
  content: [{ type: "paragraph", content: [{ type: "text", text: "hello" }] }],
});

// Replace step: insert 'x' at position 2 (inside first paragraph, after 'h').
// Stays valid across repeated inserts because the paragraph grows each time.
const INSERT_STEP = JSON.stringify({
  stepType: "replace",
  from: 2,
  to: 2,
  slice: { content: [{ type: "text", text: "x" }] },
});

// A step that points way past the end of the document → always fails to apply.
const BAD_POSITION_STEP = JSON.stringify({
  stepType: "replace",
  from: 9999,
  to: 9999,
  slice: { content: [{ type: "text", text: "x" }] },
});

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

test("creates an editor with initial version 0", () => {
  const editor = new Editor(SCHEMA, DOC);
  assert.equal(editor.version, 0);
});

test("constructor throws on invalid schema JSON", () => {
  assert.throws(() => new Editor("not-valid-json", DOC), /Invalid schema JSON/);
});

test("constructor throws on invalid doc JSON", () => {
  assert.throws(
    () => new Editor(SCHEMA, "not-valid-json"),
    /Invalid document JSON/,
  );
});

test("constructor throws when doc JSON is not a node object", () => {
  // A JSON array cannot be deserialized as a document node.
  assert.throws(
    () => new Editor(SCHEMA, JSON.stringify([])),
    /Invalid document/,
  );
});

// ---------------------------------------------------------------------------
// docJson with skipDefaults
// ---------------------------------------------------------------------------

test("docJson() without argument includes all attributes", () => {
  // Schema with attributes that have defaults
  const schemaWithAttrs = JSON.stringify({
    nodes: {
      doc: { content: "paragraph+" },
      paragraph: {
        content: "text*",
        group: "block",
        attrs: { align: { default: "left" }, indent: { default: 0 } },
      },
      text: { group: "inline" },
    },
    marks: { strong: { attrs: { level: { default: 1 } } }, em: {} },
  });

  // Document with default attributes (should be included in regular serialization)
  const docDefault = JSON.stringify({
    type: "doc",
    content: [
      {
        type: "paragraph",
        attrs: { align: "left", indent: 0 },
        content: [
          {
            type: "text",
            text: "hello",
            marks: [{ type: "strong", attrs: { level: 1 } }],
          },
        ],
      },
    ],
  });

  const editorDefault = new Editor(schemaWithAttrs, docDefault);

  // Regular serialization should include all attrs
  const fullRaw = editorDefault.docJson();
  const full = JSON.parse(fullRaw);
  assert.ok(
    fullRaw.includes('"attrs"'),
    `Expected attrs in full serialization: ${fullRaw}`,
  );
  assert.equal(full.content[0].attrs.align, "left");
  assert.equal(full.content[0].attrs.indent, 0);
  assert.equal(full.content[0].content[0].marks[0].attrs.level, 1);

  // Mini serialization should skip all default attributes
  const miniRaw = editorDefault.docJson(true);
  const mini = JSON.parse(miniRaw);
  assert.ok(
    !miniRaw.includes('"attrs"'),
    `Expected no attrs in mini serialization: ${miniRaw}`,
  );
  assert.ok(!("attrs" in mini.content[0]), "paragraph should have no attrs");
  assert.ok(
    !("attrs" in mini.content[0].content[0].marks[0]),
    "mark should have no attrs",
  );

  // Document with non-default attributes
  const docCustom = JSON.stringify({
    type: "doc",
    content: [
      {
        type: "paragraph",
        attrs: { align: "right", indent: 2 },
        content: [{ type: "text", text: "world" }],
      },
    ],
  });

  const editorCustom = new Editor(schemaWithAttrs, docCustom);
  const miniCustomRaw = editorCustom.docJson(true);
  const miniCustom = JSON.parse(miniCustomRaw);
  // Non-default attrs should still appear
  assert.ok(
    miniCustomRaw.includes('"attrs"'),
    `Expected attrs for non-default values: ${miniCustomRaw}`,
  );
  assert.equal(miniCustom.content[0].attrs.align, "right");
  assert.equal(miniCustom.content[0].attrs.indent, 2);

  // Mix of default and non-default
  const docMixed = JSON.stringify({
    type: "doc",
    content: [
      {
        type: "paragraph",
        attrs: { align: "center", indent: 0 },
        content: [{ type: "text", text: "mixed" }],
      },
    ],
  });

  const editorMixed = new Editor(schemaWithAttrs, docMixed);
  const miniMixedRaw = editorMixed.docJson(true);
  const miniMixed = JSON.parse(miniMixedRaw);
  // Only 'align' should be present (indent is default 0)
  assert.ok(
    miniMixedRaw.includes('"attrs"'),
    `Expected attrs for partial non-default: ${miniMixedRaw}`,
  );
  assert.equal(miniMixed.content[0].attrs.align, "center");
  assert.ok(
    !("indent" in miniMixed.content[0].attrs),
    "indent should be omitted (default value)",
  );

  // Verify backwards compatibility: no argument == false
  const editorBC = new Editor(SCHEMA, DOC);
  const noArg = editorBC.docJson();
  const falseArg = editorBC.docJson(false);
  assert.equal(
    noArg,
    falseArg,
    "docJson() and docJson(false) should be identical",
  );
});

// ---------------------------------------------------------------------------
// applyStep
// ---------------------------------------------------------------------------

test("applyStep returns true on success and increments version", () => {
  const editor = new Editor(SCHEMA, DOC);
  const ok = editor.applyStep(INSERT_STEP);
  assert.equal(ok, true);
  assert.equal(editor.version, 1);
});

test("applyStep mutates the document", () => {
  const editor = new Editor(SCHEMA, DOC);
  editor.applyStep(INSERT_STEP);
  const doc = JSON.parse(editor.docJson());
  // The inserted 'x' should appear in the serialised document.
  assert.ok(JSON.stringify(doc).includes("x"));
});

test("applyStep accumulates version across multiple calls", () => {
  const editor = new Editor(SCHEMA, DOC);
  editor.applyStep(INSERT_STEP);
  editor.applyStep(INSERT_STEP);
  editor.applyStep(INSERT_STEP);
  assert.equal(editor.version, 3);
});

test("applyStep returns false when the step cannot be applied", () => {
  const editor = new Editor(SCHEMA, DOC);
  const ok = editor.applyStep(BAD_POSITION_STEP);
  assert.equal(ok, false);
  assert.equal(editor.version, 0);
});

test("applyStep leaves the document unchanged on failure", () => {
  const editor = new Editor(SCHEMA, DOC);
  const before = editor.docJson();
  editor.applyStep(BAD_POSITION_STEP);
  assert.equal(editor.docJson(), before);
});

test("applyStep throws on invalid JSON", () => {
  const editor = new Editor(SCHEMA, DOC);
  assert.throws(() => editor.applyStep("not-json"), /Invalid step JSON/);
});

// ---------------------------------------------------------------------------
// applyStepsJson
// ---------------------------------------------------------------------------

test("applyStepsJson applies all steps from a JSON array string", () => {
  const editor = new Editor(SCHEMA, DOC);
  const stepsJson = JSON.stringify([
    JSON.parse(INSERT_STEP),
    JSON.parse(INSERT_STEP),
  ]);
  const ok = editor.applyStepsJson(stepsJson);
  assert.equal(ok, true);
  assert.equal(editor.version, 2);
});

test("applyStepsJson is atomic: rolls back version on step failure", () => {
  const editor = new Editor(SCHEMA, DOC);
  const stepsJson = JSON.stringify([
    JSON.parse(INSERT_STEP),
    JSON.parse(BAD_POSITION_STEP),
  ]);
  const ok = editor.applyStepsJson(stepsJson);
  assert.equal(ok, false);
  assert.equal(editor.version, 0);
});

test("applyStepsJson is atomic: rolls back document on step failure", () => {
  const editor = new Editor(SCHEMA, DOC);
  const before = editor.docJson();
  const stepsJson = JSON.stringify([
    JSON.parse(INSERT_STEP),
    JSON.parse(BAD_POSITION_STEP),
  ]);
  editor.applyStepsJson(stepsJson);
  assert.equal(editor.docJson(), before);
});

test("applyStepsJson succeeds for an empty array", () => {
  const editor = new Editor(SCHEMA, DOC);
  const ok = editor.applyStepsJson("[]");
  assert.equal(ok, true);
  assert.equal(editor.version, 0);
});

test("applyStepsJson throws on invalid JSON", () => {
  const editor = new Editor(SCHEMA, DOC);
  assert.throws(() => editor.applyStepsJson("not-json"), /Invalid steps JSON/);
});

// ---------------------------------------------------------------------------
// applySteps
// ---------------------------------------------------------------------------

test("applySteps applies all steps from an array of JSON strings", () => {
  const editor = new Editor(SCHEMA, DOC);
  const ok = editor.applySteps([INSERT_STEP, INSERT_STEP]);
  assert.equal(ok, true);
  assert.equal(editor.version, 2);
});

test("applySteps is atomic: rolls back version on step failure", () => {
  const editor = new Editor(SCHEMA, DOC);
  const ok = editor.applySteps([INSERT_STEP, BAD_POSITION_STEP]);
  assert.equal(ok, false);
  assert.equal(editor.version, 0);
});

test("applySteps is atomic: rolls back document on step failure", () => {
  const editor = new Editor(SCHEMA, DOC);
  const before = editor.docJson();
  editor.applySteps([INSERT_STEP, BAD_POSITION_STEP]);
  assert.equal(editor.docJson(), before);
});

test("applySteps succeeds for an empty array", () => {
  const editor = new Editor(SCHEMA, DOC);
  const ok = editor.applySteps([]);
  assert.equal(ok, true);
  assert.equal(editor.version, 0);
});

test("applySteps throws on an invalid step JSON string", () => {
  const editor = new Editor(SCHEMA, DOC);
  assert.throws(() => editor.applySteps(["not-json"]), /Invalid step JSON/);
});

test("applySteps throws before mutating when a later element is invalid JSON", () => {
  const editor = new Editor(SCHEMA, DOC);
  const before = editor.docJson();
  // All steps are parsed first, so the bad second element prevents any mutation.
  assert.throws(() => editor.applySteps([INSERT_STEP, "not-json"]));
  assert.equal(editor.docJson(), before);
  assert.equal(editor.version, 0);
});

// ---------------------------------------------------------------------------
// reset
// ---------------------------------------------------------------------------

test("reset restores version to 0", () => {
  const editor = new Editor(SCHEMA, DOC);
  editor.applyStep(INSERT_STEP);
  editor.applyStep(INSERT_STEP);
  assert.equal(editor.version, 2);

  editor.reset(DOC);
  assert.equal(editor.version, 0);
});

test("reset replaces the document", () => {
  const editor = new Editor(SCHEMA, DOC);
  editor.applyStep(INSERT_STEP); // mutate

  editor.reset(DOC);
  // After reset the document should match the original (no 'x' inserted).
  const doc = JSON.parse(editor.docJson());
  assert.equal(doc.type, "doc");
  assert.ok(
    !JSON.stringify(doc.content).startsWith(
      '[{"type":"paragraph","content":[{"type":"text","text":"x',
    ),
  );
});

test("reset allows applying steps again after rollback", () => {
  const editor = new Editor(SCHEMA, DOC);
  editor.applyStep(INSERT_STEP);
  editor.reset(DOC);

  const ok = editor.applyStep(INSERT_STEP);
  assert.equal(ok, true);
  assert.equal(editor.version, 1);
});

test("reset throws on invalid JSON", () => {
  const editor = new Editor(SCHEMA, DOC);
  assert.throws(() => editor.reset("not-json"), /Invalid document JSON/);
});

test("reset throws when new doc JSON is not a node object", () => {
  const editor = new Editor(SCHEMA, DOC);
  // A JSON array cannot be deserialized as a document node.
  assert.throws(() => editor.reset(JSON.stringify([])), /Invalid document/);
});

// ---------------------------------------------------------------------------
// Schema caching
// ---------------------------------------------------------------------------

test("multiple editors with the same schema all work correctly", () => {
  const editors = Array.from({ length: 5 }, () => new Editor(SCHEMA, DOC));
  for (const editor of editors) {
    assert.equal(editor.version, 0);
    const doc = JSON.parse(editor.docJson());
    assert.equal(doc.type, "doc");
  }
});

test("editors with the same schema do not share document state", () => {
  const editorA = new Editor(SCHEMA, DOC);
  const editorB = new Editor(SCHEMA, DOC);

  editorA.applyStep(INSERT_STEP);
  assert.equal(editorA.version, 1);
  assert.equal(editorB.version, 0); // B is untouched
});
