"use strict";

// Converted from vendor/from-dom.ts (prosemirror-model@1.25.7)
// Creates DOMParser and related helpers using the provided binding.

function createDOMParser(binding) {
  const { Fragment, Slice, Mark, Node, ContentMatch, ResolvedPos, Schema, NodeType, MarkType } = binding;

  function isTagRule(rule) { return rule.tag != null; }
  function isStyleRule(rule) { return rule.style != null; }

  class DOMParser {
    constructor(schema, rules) {
      this.tags = [];
      this.styles = [];
      this.matchedStyles = [];
      this.schema = schema;
      this.rules = rules;

      let matchedStyles = this.matchedStyles = [];
      rules.forEach(rule => {
        if (isTagRule(rule)) {
          this.tags.push(rule);
        } else if (isStyleRule(rule)) {
          let prop = /[^=]*/.exec(rule.style)[0];
          if (matchedStyles.indexOf(prop) < 0) matchedStyles.push(prop);
          this.styles.push(rule);
        }
      });

      this.normalizeLists = !this.tags.some(r => {
        if (!/^(ul|ol)\b/.test(r.tag) || !r.node) return false;
        let node = schema.nodes()[r.node];
        return node.contentMatch().matchType(node);
      });
    }

    parse(dom, options = {}) {
      let context = new ParseContext(this, options, false);
      context.addAll(dom, Mark.none, options.from, options.to);
      return context.finish();
    }

    parseSlice(dom, options = {}) {
      let context = new ParseContext(this, options, true);
      context.addAll(dom, Mark.none, options.from, options.to);
      return Slice.maxOpen(context.finish());
    }

    matchTag(dom, context, after) {
      for (let i = after ? this.tags.indexOf(after) + 1 : 0; i < this.tags.length; i++) {
        let rule = this.tags[i];
        if (matches(dom, rule.tag) &&
            (rule.namespace === undefined || dom.namespaceURI == rule.namespace) &&
            (!rule.context || context.matchesContext(rule.context))) {
          if (rule.getAttrs) {
            let result = rule.getAttrs(dom);
            if (result === false) continue;
            rule.attrs = result || undefined;
          }
          return rule;
        }
      }
    }

    matchStyle(prop, value, context, after) {
      for (let i = after ? this.styles.indexOf(after) + 1 : 0; i < this.styles.length; i++) {
        let rule = this.styles[i], style = rule.style;
        if (style.indexOf(prop) != 0 ||
            rule.context && !context.matchesContext(rule.context) ||
            style.length > prop.length &&
            (style.charCodeAt(prop.length) != 61 || style.slice(prop.length + 1) != value))
          continue;
        if (rule.getAttrs) {
          let result = rule.getAttrs(value);
          if (result === false) continue;
          rule.attrs = result || undefined;
        }
        return rule;
      }
    }

    static schemaRules(schema) {
      let result = [];
      function insert(rule) {
        let priority = rule.priority == null ? 50 : rule.priority, i = 0;
        for (; i < result.length; i++) {
          let next = result[i], nextPriority = next.priority == null ? 50 : next.priority;
          if (nextPriority < priority) break;
        }
        result.splice(i, 0, rule);
      }

      for (let name in schema.marks()) {
        let rules = schema.marks()[name].spec.parseDOM;
        if (rules) rules.forEach(rule => {
          insert(rule = copy(rule));
          if (!(rule.mark || rule.ignore || rule.clearMark))
            rule.mark = name;
        });
      }
      for (let name in schema.nodes()) {
        let rules = schema.nodes()[name].spec.parseDOM;
        if (rules) rules.forEach(rule => {
          insert(rule = copy(rule));
          if (!(rule.node || rule.ignore || rule.mark))
            rule.node = name;
        });
      }
      return result;
    }

    static fromSchema(schema) {
      return schema.cached.domParser ||
        (schema.cached.domParser = new DOMParser(schema, DOMParser.schemaRules(schema)));
    }
  }

  const blockTags = {
    address: true, article: true, aside: true, blockquote: true, canvas: true,
    dd: true, div: true, dl: true, fieldset: true, figcaption: true, figure: true,
    footer: true, form: true, h1: true, h2: true, h3: true, h4: true, h5: true,
    h6: true, header: true, hgroup: true, hr: true, li: true, noscript: true, ol: true,
    output: true, p: true, pre: true, section: true, table: true, tfoot: true, ul: true
  };

  const ignoreTags = {
    head: true, noscript: true, object: true, script: true, style: true, title: true
  };

  const listTags = { ol: true, ul: true };

  const OPT_PRESERVE_WS = 1, OPT_PRESERVE_WS_FULL = 2, OPT_OPEN_LEFT = 4;

  function wsOptionsFor(type, preserveWhitespace, base) {
    if (preserveWhitespace != null) return (preserveWhitespace ? OPT_PRESERVE_WS : 0) |
      (preserveWhitespace === "full" ? OPT_PRESERVE_WS_FULL : 0);
    return type && type.whitespace == "pre" ? OPT_PRESERVE_WS | OPT_PRESERVE_WS_FULL : base & ~OPT_OPEN_LEFT;
  }

  class NodeContext {
    constructor(type, attrs, marks, solid, match, options) {
      this.type = type;
      this.attrs = attrs;
      this.marks = marks;
      this.solid = solid;
      this.options = options;
      this.match = match || (options & OPT_OPEN_LEFT ? null : type.contentMatch());
      this.content = [];
      this.activeMarks = Mark.none;
    }

    findWrapping(node) {
      if (!this.match) {
        if (!this.type) return [];
        let fill = this.type.contentMatch().fillBefore(Fragment.from([node]), false);
        if (fill) {
          this.match = this.type.contentMatch().matchFragment(fill);
        } else {
          let start = this.type.contentMatch(), wrap;
          if (wrap = start.findWrapping(node.type)) {
            this.match = start;
            return wrap;
          } else {
            return null;
          }
        }
      }
      return this.match.findWrapping(node.type);
    }

    finish(openEnd) {
      if (!(this.options & OPT_PRESERVE_WS)) {
        let last = this.content[this.content.length - 1], m;
        if (last && last.isText && (m = /[ \t\r\n\u000c]+$/.exec(last.text))) {
          let text = last;
          if (last.text.length == m[0].length) this.content.pop();
          else this.content[this.content.length - 1] = text.withText(text.text.slice(0, text.text.length - m[0].length));
        }
      }
      let content = Fragment.from(this.content);
      if (!openEnd && this.match)
        content = content.append(this.match.fillBefore(Fragment.empty, true));
      return this.type ? this.type.create(this.attrs, content, this.marks) : content;
    }

    inlineContext(node) {
      if (this.type) return this.type.inlineContent;
      if (this.content.length) return this.content[0].isInline;
      return node.parentNode && !blockTags.hasOwnProperty(node.parentNode.nodeName.toLowerCase());
    }
  }

  class ParseContext {
    constructor(parser, options, isOpen) {
      this.parser = parser;
      this.options = options;
      this.isOpen = isOpen;
      this.open = 0;
      this.localPreserveWS = false;

      let topNode = options.topNode, topContext;
      let topOptions = wsOptionsFor(null, options.preserveWhitespace, 0) | (isOpen ? OPT_OPEN_LEFT : 0);
      if (topNode)
        topContext = new NodeContext(topNode.type, topNode.attrs, Mark.none, true,
                                     options.topMatch || topNode.type.contentMatch(), topOptions);
      else if (isOpen)
        topContext = new NodeContext(null, null, Mark.none, true, null, topOptions);
      else
        topContext = new NodeContext(parser.schema.topNodeType, null, Mark.none, true, null, topOptions);
      this.nodes = [topContext];
      this.find = options.findPositions;
      this.needsBlock = false;
    }

    get top() {
      return this.nodes[this.open];
    }

    addDOM(dom, marks) {
      if (dom.nodeType == 3) this.addTextNode(dom, marks);
      else if (dom.nodeType == 1) this.addElement(dom, marks);
    }

    addTextNode(dom, marks) {
      let value = dom.nodeValue;
      let top = this.top, preserveWS = (top.options & OPT_PRESERVE_WS_FULL) ? "full"
        : this.localPreserveWS || (top.options & OPT_PRESERVE_WS) > 0;
      let { schema } = this.parser;
      if (preserveWS === "full" ||
          top.inlineContext(dom) ||
          /[^ \t\r\n\u000c]/.test(value)) {
        if (!preserveWS) {
          value = value.replace(/[ \t\r\n\u000c]+/g, " ");
          if (/^[ \t\r\n\u000c]/.test(value) && this.open == this.nodes.length - 1) {
            let nodeBefore = top.content[top.content.length - 1];
            let domNodeBefore = dom.previousSibling;
            if (!nodeBefore ||
                (domNodeBefore && domNodeBefore.nodeName == 'BR') ||
                (nodeBefore.isText && /[ \t\r\n\u000c]$/.test(nodeBefore.text)))
              value = value.slice(1);
          }
        } else if (preserveWS === "full") {
          value = value.replace(/\r\n?/g, "\n");
        } else if (schema.linebreakReplacement && /[\r\n]/.test(value) && this.top.findWrapping(schema.linebreakReplacement.create(null, null, []))) {
          let lines = value.split(/\r?\n|\r/);
          for (let i = 0; i < lines.length; i++) {
            if (i) this.insertNode(schema.linebreakReplacement.create(null, null, []), marks, true);
            if (lines[i]) this.insertNode(schema.text(lines[i], []), marks, !/\S/.test(lines[i]));
          }
          value = "";
        } else {
          value = value.replace(/\r?\n|\r/g, " ");
        }
        if (value) this.insertNode(schema.text(value, []), marks, !/\S/.test(value));
        this.findInText(dom);
      } else {
        this.findInside(dom);
      }
    }

    addElement(dom, marks, matchAfter) {
      let outerWS = this.localPreserveWS, top = this.top;
      if (dom.tagName == "PRE" || /pre/.test(dom.style && dom.style.whiteSpace))
        this.localPreserveWS = true;
      let name = dom.nodeName.toLowerCase(), ruleID;
      if (listTags.hasOwnProperty(name) && this.parser.normalizeLists) normalizeList(dom);
      let rule = (this.options.ruleFromNode && this.options.ruleFromNode(dom)) ||
          (ruleID = this.parser.matchTag(dom, this, matchAfter));
      out:
      if (rule ? rule.ignore : ignoreTags.hasOwnProperty(name)) {
        this.findInside(dom);
        this.ignoreFallback(dom, marks);
      } else if (!rule || rule.skip || rule.closeParent) {
        if (rule && rule.closeParent) this.open = Math.max(0, this.open - 1);
        else if (rule && rule.skip.nodeType) dom = rule.skip;
        let sync, oldNeedsBlock = this.needsBlock;
        if (blockTags.hasOwnProperty(name)) {
          if (top.content.length && top.content[0].isInline && this.open) {
            this.open--;
            top = this.top;
          }
          sync = true;
          if (!top.type) this.needsBlock = true;
        } else if (!dom.firstChild) {
          this.leafFallback(dom, marks);
          break out;
        }
        let innerMarks = rule && rule.skip ? marks : this.readStyles(dom, marks);
        if (innerMarks) this.addAll(dom, innerMarks);
        if (sync) this.sync(top);
        this.needsBlock = oldNeedsBlock;
      } else {
        let innerMarks = this.readStyles(dom, marks);
        if (innerMarks)
          this.addElementByRule(dom, rule, innerMarks, rule.consuming === false ? ruleID : undefined);
      }
      this.localPreserveWS = outerWS;
    }

    leafFallback(dom, marks) {
      if (dom.nodeName == "BR" && this.top.type && this.top.type.inlineContent)
        this.addTextNode(dom.ownerDocument.createTextNode("\n"), marks);
    }

    ignoreFallback(dom, marks) {
      if (dom.nodeName == "BR" && (!this.top.type || !this.top.type.inlineContent))
        this.findPlace(this.parser.schema.text("-", []), marks, true);
    }

    readStyles(dom, marks) {
      let styles = dom.style;
      if (styles && styles.length) for (let i = 0; i < this.parser.matchedStyles.length; i++) {
        let name = this.parser.matchedStyles[i], value = styles.getPropertyValue(name);
        if (value) for (let after = undefined;;) {
          let rule = this.parser.matchStyle(name, value, this, after);
          if (!rule) break;
          if (rule.ignore) return null;
          if (rule.clearMark)
            marks = marks.filter(m => !rule.clearMark(m));
          else
            marks = marks.concat(this.parser.schema.marks()[rule.mark].create(rule.attrs || null));
          if (rule.consuming === false) after = rule;
          else break;
        }
      }
      return marks;
    }

    addElementByRule(dom, rule, marks, continueAfter) {
      let sync, nodeType;
      if (rule.node) {
        nodeType = this.parser.schema.nodes()[rule.node];
        if (!nodeType.isLeaf) {
          let inner = this.enter(nodeType, rule.attrs || null, marks, rule.preserveWhitespace);
          if (inner) {
            sync = true;
            marks = inner;
          }
        } else if (!this.insertNode(nodeType.create(rule.attrs || null, null, []), marks, dom.nodeName == "BR")) {
          this.leafFallback(dom, marks);
        }
      } else {
        let markType = this.parser.schema.marks()[rule.mark];
        marks = marks.concat(markType.create(rule.attrs || null));
      }
      let startIn = this.top;

      if (nodeType && nodeType.isLeaf) {
        this.findInside(dom);
      } else if (continueAfter) {
        this.addElement(dom, marks, continueAfter);
      } else if (rule.getContent) {
        this.findInside(dom);
        rule.getContent(dom, this.parser.schema).forEach(node => this.insertNode(node, marks, false));
      } else {
        let contentDOM = dom;
        if (typeof rule.contentElement == "string") contentDOM = dom.querySelector(rule.contentElement);
        else if (typeof rule.contentElement == "function") contentDOM = rule.contentElement(dom);
        else if (rule.contentElement) contentDOM = rule.contentElement;
        this.findAround(dom, contentDOM, true);
        this.addAll(contentDOM, marks);
        this.findAround(dom, contentDOM, false);
      }
      if (sync && this.sync(startIn)) this.open--;
    }

    addAll(parent, marks, startIndex, endIndex) {
      let index = startIndex || 0;
      for (let dom = startIndex ? parent.childNodes[startIndex] : parent.firstChild,
               end = endIndex == null ? null : parent.childNodes[endIndex];
           dom != end; dom = dom.nextSibling, ++index) {
        this.findAtPoint(parent, index);
        this.addDOM(dom, marks);
      }
      this.findAtPoint(parent, index);
    }

    findPlace(node, marks, cautious) {
      let route, sync;
      for (let depth = this.open, penalty = 0; depth >= 0; depth--) {
        let cx = this.nodes[depth];
        let found = cx.findWrapping(node);
        if (found && (!route || route.length > found.length + penalty)) {
          route = found;
          sync = cx;
          if (!found.length) break;
        }
        if (cx.solid) {
          if (cautious) break;
          penalty += 2;
        }
      }
      if (!route) return null;
      this.sync(sync);
      for (let i = 0; i < route.length; i++)
        marks = this.enterInner(route[i], null, marks, false);
      return marks;
    }

    insertNode(node, marks, cautious) {
      if (node.isInline && this.needsBlock && !this.top.type) {
        let block = this.textblockFromContext();
        if (block) marks = this.enterInner(block, null, marks);
      }
      let innerMarks = this.findPlace(node, marks, cautious);
      if (innerMarks) {
        this.closeExtra();
        let top = this.top;
        if (top.match) top.match = top.match.matchType(node.type);
        let nodeMarks = Mark.none;
        for (let m of innerMarks.concat(node.marks))
          if (top.type ? top.type.allowsMarkType(m.type) : markMayApply(m.type, node.type))
            nodeMarks = m.addToSet(nodeMarks);
        top.content.push(node.mark(nodeMarks));
        return true;
      }
      return false;
    }

    enter(type, attrs, marks, preserveWS) {
      let innerMarks = this.findPlace(type.create(attrs || null, null, []), marks, false);
      if (innerMarks) innerMarks = this.enterInner(type, attrs, marks, true, preserveWS);
      return innerMarks;
    }

    enterInner(type, attrs, marks, solid, preserveWS) {
      this.closeExtra();
      let top = this.top;
      top.match = top.match && top.match.matchType(type);
      let options = wsOptionsFor(type, preserveWS, top.options);
      if ((top.options & OPT_OPEN_LEFT) && top.content.length == 0) options |= OPT_OPEN_LEFT;
      let applyMarks = Mark.none;
      marks = marks.filter(m => {
        if (top.type ? top.type.allowsMarkType(m.type) : markMayApply(m.type, type)) {
          applyMarks = m.addToSet(applyMarks);
          return false;
        }
        return true;
      });
      this.nodes.push(new NodeContext(type, attrs, applyMarks, solid, null, options));
      this.open++;
      return marks;
    }

    closeExtra(openEnd = false) {
      let i = this.nodes.length - 1;
      if (i > this.open) {
        for (; i > this.open; i--) this.nodes[i - 1].content.push(this.nodes[i].finish(openEnd));
        this.nodes.length = this.open + 1;
      }
    }

    finish() {
      this.open = 0;
      this.closeExtra(this.isOpen);
      return this.nodes[0].finish(!!(this.isOpen || this.options.topOpen));
    }

    sync(to) {
      for (let i = this.open; i >= 0; i--) {
        if (this.nodes[i] == to) {
          this.open = i;
          return true;
        } else if (this.localPreserveWS) {
          this.nodes[i].options |= OPT_PRESERVE_WS;
        }
      }
      return false;
    }

    get currentPos() {
      this.closeExtra();
      let pos = 0;
      for (let i = this.open; i >= 0; i--) {
        let content = this.nodes[i].content;
        for (let j = content.length - 1; j >= 0; j--)
          pos += content[j].nodeSize;
        if (i) pos++;
      }
      return pos;
    }

    findAtPoint(parent, offset) {
      if (this.find) for (let i = 0; i < this.find.length; i++) {
        if (this.find[i].node == parent && this.find[i].offset == offset)
          this.find[i].pos = this.currentPos;
      }
    }

    findInside(parent) {
      if (this.find) for (let i = 0; i < this.find.length; i++) {
        if (this.find[i].pos == null && parent.nodeType == 1 && parent.contains(this.find[i].node))
          this.find[i].pos = this.currentPos;
      }
    }

    findAround(parent, content, before) {
      if (parent != content && this.find) for (let i = 0; i < this.find.length; i++) {
        if (this.find[i].pos == null && parent.nodeType == 1 && parent.contains(this.find[i].node)) {
          let pos = content.compareDocumentPosition(this.find[i].node);
          if (pos & (before ? 2 : 4))
            this.find[i].pos = this.currentPos;
        }
      }
    }

    findInText(textNode) {
      if (this.find) for (let i = 0; i < this.find.length; i++) {
        if (this.find[i].node == textNode)
          this.find[i].pos = this.currentPos - (textNode.nodeValue.length - this.find[i].offset);
      }
    }

    matchesContext(context) {
      if (context.indexOf("|") > -1)
        return context.split(/\s*\|\s*/).some(this.matchesContext, this);

      let parts = context.split("/");
      let option = this.options.context;
      let useRoot = !this.isOpen && (!option || option.parent.type == this.nodes[0].type);
      let minDepth = -(option ? option.depth + 1 : 0) + (useRoot ? 0 : 1);
      let match = (i, depth) => {
        for (; i >= 0; i--) {
          let part = parts[i];
          if (part == "") {
            if (i == parts.length - 1 || i == 0) continue;
            for (; depth >= minDepth; depth--)
              if (match(i - 1, depth)) return true;
            return false;
          } else {
            let next = depth > 0 || (depth == 0 && useRoot) ? this.nodes[depth].type
                : option && depth >= minDepth ? option.node(depth - minDepth).type
                : null;
            if (!next || (next.name != part && !next.isInGroup(part)))
              return false;
            depth--;
          }
        }
        return true;
      };
      return match(parts.length - 1, this.open);
    }

    textblockFromContext() {
      let $context = this.options.context;
      if ($context) for (let d = $context.depth; d >= 0; d--) {
        let deflt = $context.node(d).contentMatchAt($context.indexAfter(d)).defaultType;
        if (deflt && deflt.isTextblock && deflt.defaultAttrs) return deflt;
      }
      for (let name in this.parser.schema.nodes()) {
        let type = this.parser.schema.nodes()[name];
        if (type.isTextblock && type.defaultAttrs) return type;
      }
    }
  }

  function normalizeList(dom) {
    for (let child = dom.firstChild, prevItem = null; child; child = child.nextSibling) {
      let name = child.nodeType == 1 ? child.nodeName.toLowerCase() : null;
      if (name && listTags.hasOwnProperty(name) && prevItem) {
        prevItem.appendChild(child);
        child = prevItem;
      } else if (name == "li") {
        prevItem = child;
      } else if (name) {
        prevItem = null;
      }
    }
  }

  function matches(dom, selector) {
    return (dom.matches || dom.msMatchesSelector || dom.webkitMatchesSelector || dom.mozMatchesSelector).call(dom, selector);
  }

  function copy(obj) {
    let copy = {};
    for (let prop in obj) copy[prop] = obj[prop];
    return copy;
  }

  function markMayApply(markType, nodeType) {
    let nodes = nodeType.schema.nodes();
    for (let name in nodes) {
      let parent = nodes[name];
      if (!parent.allowsMarkType(markType)) continue;
      let seen = [], scan = (match) => {
        seen.push(match);
        for (let i = 0; i < match.edgeCount(); i++) {
          let { type, next } = match.edge(i);
          if (type == nodeType) return true;
          if (seen.indexOf(next) < 0 && scan(next)) return true;
        }
      };
      if (scan(parent.contentMatch())) return true;
    }
  }

  return DOMParser;
}

module.exports = { createDOMParser };
