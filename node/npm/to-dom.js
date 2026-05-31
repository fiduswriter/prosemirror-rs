"use strict";

// Converted from vendor/to-dom.ts (prosemirror-model@1.25.7)
// Creates DOMSerializer and related helpers using the provided binding.

function createDOMSerializer(binding) {
  const { Fragment, Node, Schema, NodeType, MarkType, Mark } = binding;

  function doc(options) {
    return options.document || (typeof window !== "undefined" && window.document);
  }

  const suspiciousAttributeCache = new WeakMap();

  function suspiciousAttributes(attrs) {
    let value = suspiciousAttributeCache.get(attrs);
    if (value === undefined)
      suspiciousAttributeCache.set(attrs, value = suspiciousAttributesInner(attrs));
    return value;
  }

  function suspiciousAttributesInner(attrs) {
    let result = null;
    function scan(value) {
      if (value && typeof value == "object") {
        if (Array.isArray(value)) {
          if (typeof value[0] == "string") {
            if (!result) result = [];
            result.push(value);
          } else {
            for (let i = 0; i < value.length; i++) scan(value[i]);
          }
        } else {
          for (let prop in value) scan(value[prop]);
        }
      }
    }
    scan(attrs);
    return result;
  }

  function renderSpec(doc, structure, xmlNS, blockArraysIn) {
    if (structure.nodeType == 1)
      return { dom: structure };
    if (structure.dom && structure.dom.nodeType == 1)
      return structure;
    let tagName = structure[0], suspicious;
    if (typeof tagName != "string") throw new RangeError("Invalid array passed to renderSpec");
    if (blockArraysIn && (suspicious = suspiciousAttributes(blockArraysIn)) &&
        suspicious.indexOf(structure) > -1)
      throw new RangeError("Using an array from an attribute object as a DOM spec. This may be an attempted cross site scripting attack.");
    let space = tagName.indexOf(" ");
    if (space > 0) {
      xmlNS = tagName.slice(0, space);
      tagName = tagName.slice(space + 1);
    }
    let contentDOM;
    let dom = xmlNS ? doc.createElementNS(xmlNS, tagName) : doc.createElement(tagName);
    let attrs = structure[1], start = 1;
    if (attrs && typeof attrs == "object" && attrs.nodeType == null && !Array.isArray(attrs)) {
      start = 2;
      for (let name in attrs) if (attrs[name] != null) {
        let space = name.indexOf(" ");
        if (space > 0) dom.setAttributeNS(name.slice(0, space), name.slice(space + 1), attrs[name]);
        else if (name == "style" && dom.style) dom.style.cssText = attrs[name];
        else dom.setAttribute(name, attrs[name]);
      }
    }
    for (let i = start; i < structure.length; i++) {
      let child = structure[i];
      if (child === 0) {
        if (i < structure.length - 1 || i > start)
          throw new RangeError("Content hole must be the only child of its parent node");
        return { dom, contentDOM: dom };
      } else if (typeof child == "string") {
        dom.appendChild(doc.createTextNode(child));
      } else {
        let { dom: inner, contentDOM: innerContent } = renderSpec(doc, child, xmlNS, blockArraysIn);
        dom.appendChild(inner);
        if (innerContent) {
          if (contentDOM) throw new RangeError("Multiple content holes");
          contentDOM = innerContent;
        }
      }
    }
    return { dom, contentDOM };
  }

  class DOMSerializer {
    constructor(nodes, marks) {
      this.nodes = nodes;
      this.marks = marks;
    }

    serializeFragment(fragment, options = {}, target) {
      if (!target) target = doc(options).createDocumentFragment();

      let top = target, active = [];
      fragment.forEach(node => {
        if (active.length || node.marks.length) {
          let keep = 0, rendered = 0;
          while (keep < active.length && rendered < node.marks.length) {
            let next = node.marks[rendered];
            if (!this.marks[next.type.name]) { rendered++; continue; }
            if (!next.eq(active[keep][0]) || next.type.spec.spanning === false) break;
            keep++; rendered++;
          }
          while (keep < active.length) top = active.pop()[1];
          while (rendered < node.marks.length) {
            let add = node.marks[rendered++];
            let markDOM = this.serializeMark(add, node.isInline, options);
            if (markDOM) {
              active.push([add, top]);
              top.appendChild(markDOM.dom);
              top = markDOM.contentDOM || markDOM.dom;
            }
          }
        }
        top.appendChild(this.serializeNodeInner(node, options));
      });

      return target;
    }

    serializeNodeInner(node, options) {
      if (node.isText) return doc(options).createTextNode(node.text);
      let { dom, contentDOM } =
        renderSpec(doc(options), this.nodes[node.type.name](node), null, node.attrs);
      if (contentDOM) {
        if (node.isLeaf)
          throw new RangeError("Content hole not allowed in a leaf node spec");
        this.serializeFragment(node.content, options, contentDOM);
      }
      return dom;
    }

    serializeNode(node, options = {}) {
      let dom = this.serializeNodeInner(node, options);
      for (let i = node.marks.length - 1; i >= 0; i--) {
        let wrap = this.serializeMark(node.marks[i], node.isInline, options);
        if (wrap) {
          (wrap.contentDOM || wrap.dom).appendChild(dom);
          dom = wrap.dom;
        }
      }
      return dom;
    }

    serializeMark(mark, inline, options = {}) {
      let toDOM = this.marks[mark.type.name];
      return toDOM && renderSpec(doc(options), toDOM(mark, inline), null, mark.attrs);
    }

    static renderSpec(doc, structure, xmlNS = null, blockArraysIn) {
      if (typeof structure == "string") return { dom: doc.createTextNode(structure) };
      return renderSpec(doc, structure, xmlNS, blockArraysIn);
    }

    static fromSchema(schema) {
      return schema.cached.domSerializer ||
        (schema.cached.domSerializer = new DOMSerializer(this.nodesFromSchema(schema), this.marksFromSchema(schema)));
    }

    static nodesFromSchema(schema) {
      let result = gatherToDOM(schema.nodes);
      if (!result.text) result.text = node => node.text;
      return result;
    }

    static marksFromSchema(schema) {
      return gatherToDOM(schema.marks);
    }
  }

  function gatherToDOM(obj) {
    let result = {};
    for (let name in obj) {
      let toDOM = obj[name].spec.toDOM;
      if (toDOM) result[name] = toDOM;
    }
    return result;
  }

  return DOMSerializer;
}

module.exports = { createDOMSerializer };
