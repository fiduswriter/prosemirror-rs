const { Schema, Node, Mark } = require("./prosemirror-model.cjs");

const NO_TAG = Object.create(null);

function flatten(schema, children, f) {
  let result = [],
    pos = 0,
    tag = NO_TAG;

  for (let i = 0; i < children.length; i++) {
    let child = children[i];
    if (typeof child === "string") {
      let re = /<(\w+)>/g,
        m,
        at = 0,
        out = "";
      while ((m = re.exec(child))) {
        out += child.slice(at, m.index);
        pos += m.index - at;
        at = m.index + m[0].length;
        if (tag === NO_TAG) tag = Object.create(null);
        tag[m[1]] = pos;
      }
      out += child.slice(at);
      pos += child.length - at;
      if (out) result.push(f(schema.text(out)));
    } else {
      if (child && child.tag && child.tag !== NO_TAG) {
        if (tag === NO_TAG) tag = Object.create(null);
        let isFlat = child.flat || child.isText;
        for (let id in child.tag)
          tag[id] = child.tag[id] + (isFlat ? 0 : 1) + pos;
      }
      if (child && child.flat) {
        for (let j = 0; j < child.flat.length; j++) {
          let node = f(child.flat[j]);
          pos += node.nodeSize;
          result.push(node);
        }
      } else {
        let node = f(child);
        pos += node.nodeSize;
        result.push(node);
      }
    }
  }
  return { nodes: result, tag };
}

function id(x) {
  return x;
}

function takeAttrs(attrs, args) {
  if (!args.length) return attrs;
  let a0 = args[0];
  if (a0 && (typeof a0 === "string" || a0 instanceof Node || a0.flat))
    return attrs;
  args.shift();
  if (!attrs) return a0;
  if (!a0) return attrs;
  let result = {};
  for (let prop in attrs) result[prop] = attrs[prop];
  for (let prop in a0) result[prop] = a0[prop];
  return result;
}

function block(type, attrs = null) {
  let result = function (...args) {
    let myAttrs = takeAttrs(attrs, args);
    let { nodes, tag } = flatten(type.schema, args, id);
    let node = type.create(myAttrs, nodes);
    node.tag = tag === NO_TAG ? {} : tag;
    return node;
  };
  if (type.isLeaf) {
    try {
      result.flat = [type.create(attrs)];
    } catch (_) {}
  }
  return result;
}

function mark(type, attrs = null) {
  return function (...args) {
    let mk = type.create(takeAttrs(attrs, args));
    let { nodes, tag } = flatten(type.schema, args, (n) => {
      let newMarks = mk.addToSet(n.marks);
      return newMarks.length > n.marks.length ? n.mark(newMarks) : n;
    });
    return { flat: nodes, tag };
  };
}

function builders(schema, names) {
  let result = { schema };
  for (let name in schema.nodes) result[name] = block(schema.nodes[name], {});
  for (let name in schema.marks) result[name] = mark(schema.marks[name], {});

  if (names) {
    for (let name in names) {
      let value = names[name];
      let typeName = value.nodeType || value.markType || name;
      let type = schema.nodes[typeName];
      if (type) {
        result[name] = block(type, value.attrs || {});
      } else {
        type = schema.marks[typeName];
        if (type) result[name] = mark(type, value.attrs || {});
      }
    }
  }
  return result;
}

const testSchema = new Schema({
  nodes: {
    doc: { content: "block+", attrs: { meta: { default: null } } },
    paragraph: { content: "inline*", group: "block" },
    blockquote: { content: "block+", group: "block", defining: true },
    horizontal_rule: { group: "block" },
    heading: {
      attrs: { level: { default: 1 } },
      content: "inline*",
      group: "block",
      defining: true,
    },
    code_block: { content: "text*", marks: "", group: "block", code: true },
    text: { group: "inline" },
    image: {
      inline: true,
      attrs: {
        src: { validate: "string" },
        alt: { default: null },
        title: { default: null },
      },
      group: "inline",
    },
    hard_break: { inline: true, group: "inline" },
    ordered_list: {
      content: "list_item+",
      group: "block",
      attrs: { order: { default: 1 } },
    },
    bullet_list: { content: "list_item+", group: "block" },
    list_item: { content: "paragraph block*", defining: true },
  },
  marks: {
    link: { attrs: { href: {}, title: { default: null } }, inclusive: false },
    em: {},
    strong: {},
    code: { code: true },
  },
});

let b = builders(testSchema, {
  doc: { nodeType: "doc" },
  p: { nodeType: "paragraph" },
  pre: { nodeType: "code_block" },
  h1: { nodeType: "heading", level: 1 },
  h2: { nodeType: "heading", level: 2 },
  h3: { nodeType: "heading", level: 3 },
  li: { nodeType: "list_item" },
  ul: { nodeType: "bullet_list" },
  ol: { nodeType: "ordered_list" },
  br: { nodeType: "hard_break" },
  img: { nodeType: "image", src: "img.png" },
  hr: { nodeType: "horizontal_rule" },
  a: { markType: "link", href: "foo" },
});

module.exports = {
  schema: testSchema,
  eq(a, b) {
    return a.eq(b);
  },
  builders,
  block,
  mark,
  doc: b.doc,
  p: b.p,
  code_block: b.code_block,
  pre: b.pre,
  h1: b.h1,
  h2: b.h2,
  h3: b.h3,
  li: b.li,
  ul: b.ul,
  ol: b.ol,
  img: b.img,
  hr: b.hr,
  br: b.br,
  blockquote: b.blockquote,
  a: b.a,
  em: b.em,
  strong: b.strong,
  code: b.code,
};
