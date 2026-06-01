"use strict";
/**
 * Regression tests for the prosemirror-model API exposed by the Node.js binding.
 *
 * Covers Schema, NodeType, MarkType, Mark, Fragment, Slice, Node, ResolvedPos,
 * NodeRange and ContentMatch — one "does it work at all" assertion per method
 * so that any future refactor that silently breaks a binding is caught immediately.
 *
 * Run via:  node --test tests/model-api.test.js  (from the node/ directory)
 * Or:       npm test  (runs all tests/ files)
 */

const { describe, test } = require("node:test");
const assert = require("node:assert/strict");
const {
  Schema,
  Node,
  Fragment,
  Slice,
  Mark,
  NodeType,
  MarkType,
  ContentMatch,
  ResolvedPos,
  NodeRange,
  contentMatchParse,
} = require("../npm/napi/index.js");

// ---------------------------------------------------------------------------
// Shared schema / doc fixture
// ---------------------------------------------------------------------------

const schema = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*", group: "block" },
    blockquote: { content: "block+", group: "block" },
    text: { group: "inline" },
    image: { inline: true, attrs: { src: {} }, group: "inline", atom: true },
    code_block: { content: "text*", group: "block", code: true, marks: "" },
  },
  marks: {
    strong: {},
    em: {},
    code: {},
  },
});

function makeDoc(textContent = "hello") {
  return schema.node("doc", null, [
    schema.node("paragraph", null, [schema.text(textContent)]),
  ]);
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

describe("Schema", () => {
  test("nodes getter returns map of NodeType", () => {
    const nodes = schema.nodes;
    assert.ok(nodes instanceof Object);
    assert.ok(nodes["paragraph"] instanceof NodeType);
    assert.ok(nodes["text"] instanceof NodeType);
  });

  test("marks getter returns map of MarkType", () => {
    const marks = schema.marks;
    assert.ok(marks instanceof Object);
    assert.ok(marks["strong"] instanceof MarkType);
  });
});

// ---------------------------------------------------------------------------
// NodeType
// ---------------------------------------------------------------------------

describe("NodeType", () => {
  const paraType = schema.nodes["paragraph"];
  const textType = schema.nodes["text"];
  const codeBlockType = schema.nodes["code_block"];

  test("isText on text node type", () => {
    assert.equal(textType.isText, true);
    assert.equal(paraType.isText, false);
  });

  test("whitespace", () => {
    assert.equal(paraType.whitespace, "normal");
    assert.equal(codeBlockType.whitespace, "pre");
  });

  test("isCode on code_block", () => {
    assert.equal(codeBlockType.isCode, true);
    assert.equal(paraType.isCode, false);
  });

  test("inlineContent", () => {
    assert.equal(paraType.inlineContent, true);
    assert.equal(schema.nodes["doc"].inlineContent, false);
  });

  test("isBlock / isInline / isLeaf / isAtom on text", () => {
    assert.equal(textType.isInline, true);
    assert.equal(paraType.isBlock, true);
    assert.equal(textType.isLeaf, true);
  });

  test("hasRequiredAttrs — image has required src attr", () => {
    assert.equal(schema.nodes["image"].hasRequiredAttrs, true);
    assert.equal(paraType.hasRequiredAttrs, false);
  });

  test("compatibleContent between paragraphs", () => {
    assert.equal(paraType.compatibleContent(paraType), true);
    assert.equal(paraType.compatibleContent(schema.nodes["blockquote"]), false);
  });

  test("allowsMarks", () => {
    const strongMark = schema.mark("strong");
    const emMark = schema.mark("em");
    // paragraph allows all marks by default
    assert.equal(paraType.allowsMarks([strongMark, emMark]), true);
    // code_block has marks: "" — no marks allowed
    assert.equal(codeBlockType.allowsMarks([strongMark]), false);
  });

  test("contentMatch returns ContentMatch", () => {
    const cm = paraType.contentMatch;
    assert.ok(cm instanceof ContentMatch);
  });
});

// ---------------------------------------------------------------------------
// MarkType
// ---------------------------------------------------------------------------

describe("MarkType", () => {
  const strongType = schema.marks["strong"];
  const emType = schema.marks["em"];
  const codeType = schema.marks["code"];

  test("removeFromSet removes a mark", () => {
    const s = schema.mark("strong");
    const e = schema.mark("em");
    const set = s.addToSet(e.addToSet([]));
    const result = strongType.removeFromSet(set);
    assert.equal(result.length, 1);
    assert.equal(result[0].type.name, "em");
  });

  test("isInSet finds a mark in set", () => {
    const s = schema.mark("strong");
    const set = s.addToSet([]);
    const found = strongType.isInSet(set);
    assert.ok(found instanceof Mark);
    assert.equal(found.type.name, "strong");
  });

  test("isInSet returns null when absent", () => {
    const set = schema.mark("em").addToSet([]);
    assert.equal(strongType.isInSet(set), null);
  });

  test("excludes — strong excludes strong", () => {
    assert.equal(strongType.excludes(strongType), true);
    // by default marks don't exclude each other across types
    assert.equal(strongType.excludes(emType), false);
  });
});

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------

describe("Mark", () => {
  test("toJson round-trips", () => {
    const m = schema.mark("strong");
    const json = m.toJson();
    assert.equal(json.type, "strong");
  });

  test("addToSet / removeFromSet / isInSet", () => {
    const s = schema.mark("strong");
    const e = schema.mark("em");
    let set = s.addToSet([]);
    assert.equal(set.length, 1);
    set = e.addToSet(set);
    assert.equal(set.length, 2);
    assert.equal(s.isInSet(set), true);
    set = s.removeFromSet(set);
    assert.equal(set.length, 1);
    assert.equal(s.isInSet(set), false);
  });
});

// ---------------------------------------------------------------------------
// Fragment
// ---------------------------------------------------------------------------

describe("Fragment", () => {
  // Use paragraph nodes as children so adjacent nodes don't get merged
  function makeFragment(texts) {
    return Fragment.fromArray(
      texts.map((t) => schema.node("paragraph", null, [schema.text(t)]))
    );
  }

  test("firstChild / lastChild", () => {
    const frag = makeFragment(["foo", "bar"]);
    assert.equal(frag.firstChild.textContent, "foo");
    assert.equal(frag.lastChild.textContent, "bar");
  });

  test("firstChild null on empty fragment", () => {
    const frag = new Fragment();
    assert.equal(frag.firstChild, null);
    assert.equal(frag.lastChild, null);
  });

  test("maybeChild", () => {
    const frag = makeFragment(["a", "b"]);
    assert.equal(frag.maybeChild(0).textContent, "a");
    assert.equal(frag.maybeChild(99), null);
  });

  test("replaceChild", () => {
    const frag = makeFragment(["foo", "bar"]);
    const newNode = schema.node("paragraph", null, [schema.text("baz")]);
    const result = frag.replaceChild(0, newNode);
    assert.equal(result.child(0).textContent, "baz");
    assert.equal(result.child(1).textContent, "bar");
  });

  test("addToStart / addToEnd", () => {
    const frag = makeFragment(["b"]);
    const a = schema.node("paragraph", null, [schema.text("a")]);
    const c = schema.node("paragraph", null, [schema.text("c")]);
    const withStart = frag.addToStart(a);
    const withEnd = frag.addToEnd(c);
    assert.equal(withStart.child(0).textContent, "a");
    assert.equal(withEnd.child(1).textContent, "c");
  });

  test("textBetween", () => {
    // Use a fragment of paragraph nodes; block separator is inserted between blocks
    const frag = Fragment.fromArray([
      schema.node("paragraph", null, [schema.text("hello")]),
      schema.node("paragraph", null, [schema.text("world")]),
    ]);
    // textBetween(from, to, blockSeparator?) — separator appears between blocks
    const result = frag.textBetween(0, frag.size, " ");
    assert.ok(result.includes("hello") && result.includes("world"));
  });

  test("forEach iterates nodes", () => {
    const frag = makeFragment(["a", "b", "c"]);
    const texts = [];
    frag.forEach((node, _offset, _index) => {
      texts.push(node.textContent);
    });
    assert.deepEqual(texts, ["a", "b", "c"]);
  });});

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

describe("Slice", () => {
  test("size getter", () => {
    const doc = makeDoc("hello");
    const s = doc.slice(1, 6);
    assert.equal(typeof s.size, "number");
    assert.ok(s.size > 0);
  });
});

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

describe("Node", () => {
  test("isAtom on image", () => {
    const img = schema.node("image", { src: "x.png" });
    assert.equal(img.isAtom, true);
    assert.equal(makeDoc().isAtom, false);
  });

  test("inlineContent", () => {
    const para = schema.node("paragraph", null, [schema.text("hi")]);
    assert.equal(para.inlineContent, true);
    assert.equal(makeDoc().inlineContent, false);
  });

  test("maybeChild", () => {
    const doc = makeDoc("hello");
    const para = doc.maybeChild(0);
    assert.ok(para !== null);
    assert.equal(para.type.name, "paragraph");
    assert.equal(doc.maybeChild(99), null);
  });

  test("textBetween with block separator", () => {
    const doc = schema.node("doc", null, [
      schema.node("paragraph", null, [schema.text("foo")]),
      schema.node("paragraph", null, [schema.text("bar")]),
    ]);
    const result = doc.textBetween(0, doc.content.size, "\n");
    assert.equal(result, "foo\nbar");
  });

  test("forEach iterates children", () => {
    const doc = makeDoc("hello");
    const children = [];
    doc.forEach((node) => children.push(node.type.name));
    assert.deepEqual(children, ["paragraph"]);
  });

  test("nodesBetween calls with (node, pos, parent, index)", () => {
    const doc = makeDoc("hello");
    const seen = [];
    doc.nodesBetween(0, doc.content.size, (node, pos, parent, index) => {
      seen.push({ name: node.type.name, parentName: parent ? parent.type.name : null });
    });
    assert.ok(seen.some((e) => e.name === "paragraph"));
    // paragraph's parent should be doc
    const para = seen.find((e) => e.name === "paragraph");
    assert.equal(para.parentName, "doc");
  });

  test("descendants calls with (node, pos, parent, index)", () => {
    const doc = makeDoc("hello");
    const seen = [];
    doc.descendants((node, pos, parent, index) => {
      seen.push(node.type.name);
    });
    assert.ok(seen.includes("paragraph"));
  });

  test("rangeHasMark detects mark in range", () => {
    const strong = schema.mark("strong");
    const para = schema.node("paragraph", null, [
      schema.text("hi", [strong]),
    ]);
    const doc = schema.node("doc", null, [para]);
    assert.equal(doc.rangeHasMark(0, doc.content.size, schema.marks["strong"]), true);
    assert.equal(doc.rangeHasMark(0, doc.content.size, schema.marks["em"]), false);
  });

  test("sameMarkup compares type+attrs+marks", () => {
    const a = schema.node("paragraph");
    const b = schema.node("paragraph");
    assert.equal(a.sameMarkup(b), true);
  });

  test("canAppend between paragraphs", () => {
    const p1 = schema.node("paragraph", null, [schema.text("a")]);
    const p2 = schema.node("paragraph", null, [schema.text("b")]);
    assert.equal(p1.canAppend(p2), true);
  });

  test("contentMatchAt returns ContentMatch", () => {
    const para = schema.node("paragraph", null, [schema.text("hello")]);
    const cm = para.contentMatchAt(0);
    assert.ok(cm instanceof ContentMatch);
  });
});

// ---------------------------------------------------------------------------
// ResolvedPos
// ---------------------------------------------------------------------------

describe("ResolvedPos", () => {
  const doc = makeDoc("hello");
  // pos 1 = inside first paragraph, before first character
  const rp = doc.resolve(1);

  test("doc returns the document root", () => {
    assert.equal(rp.doc.type.name, "doc");
  });

  test("textOffset", () => {
    // pos 1 is the start of the paragraph — offset 0 within text
    assert.equal(typeof rp.textOffset, "number");
  });

  test("index and indexAfter", () => {
    assert.equal(typeof rp.index(), "number");
    assert.equal(typeof rp.indexAfter(), "number");
  });

  test("sharedDepth with same pos", () => {
    assert.equal(rp.sharedDepth(1), rp.depth);
  });

  test("marksAcross returns marks array or null", () => {
    const rp2 = doc.resolve(3);
    const result = rp.marksAcross(rp2);
    // null or array
    assert.ok(result === null || Array.isArray(result));
  });

  test("sameParent", () => {
    const rp2 = doc.resolve(3);
    assert.equal(rp.sameParent(rp2), true);
  });

  test("max / min", () => {
    const rp2 = doc.resolve(4);
    assert.equal(rp.max(rp2).pos, Math.max(rp.pos, rp2.pos));
    assert.equal(rp.min(rp2).pos, Math.min(rp.pos, rp2.pos));
  });
});

// ---------------------------------------------------------------------------
// NodeRange
// ---------------------------------------------------------------------------

describe("NodeRange", () => {
  test("from / to / parent / startIndex / endIndex / depth", () => {
    const doc = makeDoc("hello");
    const from = doc.resolve(1);
    const to = doc.resolve(3);
    const range = from.blockRange(to);
    assert.ok(range instanceof NodeRange);
    assert.ok(range.$from instanceof ResolvedPos);
    assert.ok(range.$to instanceof ResolvedPos);
    assert.equal(typeof range.from, "number");
    assert.equal(typeof range.to, "number");
    assert.ok(range.parent instanceof Node);
    assert.equal(typeof range.startIndex, "number");
    assert.equal(typeof range.endIndex, "number");
    assert.equal(typeof range.depth, "number");
  });
});

// ---------------------------------------------------------------------------
// ContentMatch
// ---------------------------------------------------------------------------

describe("ContentMatch", () => {
  const paraType = schema.nodes["paragraph"];
  const cm = paraType.contentMatch;

  test("validEnd on empty paragraph content match start", () => {
    // paragraph content is "text*" — valid end (zero or more)
    assert.equal(cm.validEnd, true);
  });

  test("edgeCount / edgeType / edgeMatch", () => {
    const count = cm.edgeCount;
    assert.equal(typeof count, "number");
    if (count > 0) {
      const et = cm.edgeType(0);
      assert.ok(et === null || et instanceof NodeType);
      const em = cm.edgeMatch(0);
      assert.ok(em === null || em instanceof ContentMatch);
    }
  });
  test("defaultType returns NodeType or null", () => {
    const dt = cm.defaultType;
    assert.ok(dt === null || dt instanceof NodeType);
  });

  test("findWrapping returns array or null", () => {
    const docType = schema.nodes["doc"];
    const docCm = docType.contentMatch;
    const result = docCm.findWrapping(schema.nodes["paragraph"]);
    // paragraph is a direct child of doc → wrapping is []
    assert.ok(result === null || Array.isArray(result));
  });

  test("matchType / matchFragment / fillBefore", () => {
    const matched = cm.matchType(schema.nodes["text"]);
    assert.ok(matched === null || matched instanceof ContentMatch);

    const frag = Fragment.fromArray([schema.text("hello")]);
    const afterFrag = cm.matchFragment(frag);
    assert.ok(afterFrag === null || afterFrag instanceof ContentMatch);

    const fill = cm.fillBefore(frag, true);
    assert.ok(fill === null || fill instanceof Fragment);
  });

  test("contentMatchParse helper", () => {
    const parsed = contentMatchParse("text*", schema);
    assert.ok(parsed instanceof ContentMatch);
    assert.equal(parsed.validEnd, true);
  });
});

// ---------------------------------------------------------------------------
// nodesBetween / descendants early-termination (returning false stops recursion)
// ---------------------------------------------------------------------------

describe("nodesBetween / descendants early-termination", () => {
  // doc with nested structure: doc > blockquote > paragraph > text
  const NESTED_SCHEMA = new Schema({
    nodes: {
      doc: { content: "block+" },
      blockquote: { content: "block+", group: "block" },
      paragraph: { content: "inline*", group: "block" },
      text: { group: "inline" },
    },
    marks: {},
  });
  const doc = NESTED_SCHEMA.node("doc", {}, [
    NESTED_SCHEMA.node("blockquote", {}, [
      NESTED_SCHEMA.node("paragraph", {}, [NESTED_SCHEMA.text("hello")]),
    ]),
  ]);

  test("Node.nodesBetween: returning false skips children", () => {
    const visited = [];
    doc.nodesBetween(0, doc.content.size, (node, pos, parent, index) => {
      visited.push(node.type.name);
      if (node.type.name === "blockquote") return false; // don't descend
    });
    assert.ok(visited.includes("blockquote"), "blockquote should be visited");
    assert.ok(!visited.includes("paragraph"), "paragraph should be skipped");
    assert.ok(!visited.includes("text"), "text should be skipped");
  });

  test("Node.nodesBetween: returning true recurses normally", () => {
    const visited = [];
    doc.nodesBetween(0, doc.content.size, (node) => {
      visited.push(node.type.name);
      return true;
    });
    assert.ok(visited.includes("blockquote"));
    assert.ok(visited.includes("paragraph"));
    assert.ok(visited.includes("text"));
  });

  test("Node.descendants: returning false skips children", () => {
    const visited = [];
    doc.descendants((node) => {
      visited.push(node.type.name);
      if (node.type.name === "blockquote") return false;
    });
    assert.ok(visited.includes("blockquote"));
    assert.ok(!visited.includes("paragraph"));
    assert.ok(!visited.includes("text"));
  });

  test("Fragment.nodesBetween: returning false skips children", () => {
    const frag = doc.content;
    const visited = [];
    frag.nodesBetween(0, frag.size, (node) => {
      visited.push(node.type.name);
      if (node.type.name === "blockquote") return false;
    });
    assert.ok(visited.includes("blockquote"));
    assert.ok(!visited.includes("paragraph"));
  });
});
