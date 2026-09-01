import { Segment, SfNode } from "./reader.js";
import { encodeValue, SfValue } from "./values.js";

function escapeText(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function scriptSafeJson(value: SfValue): string {
  return JSON.stringify(encodeValue(value)).replace(/</g, "\\u003c");
}

export interface IdAlloc {
  next: number;
}

/** Client-side ids use the `sf-c` prefix so they can never collide with the server's `sf-i` sequence. */
export function nodeToHtml(node: SfNode, ids: IdAlloc): string {
  switch (node.kind) {
    case "text":
      return escapeText(node.text);
    case "raw":
      return node.html;
    case "seq":
      return node.children.map((c) => nodeToHtml(c, ids)).join("");
    case "client": {
      const id = `sf-c${ids.next++}`;
      const inner = node.ssr ? nodeToHtml(node.ssr, ids) : node.children.map((c) => nodeToHtml(c, ids)).join("");
      const props = scriptSafeJson(node.props);
      return (
        `<sf-i id="${id}" data-sf-module="${node.module}">${inner}</sf-i>` +
        `<script type="application/json" data-sf-props="${id}">${props}</script>`
      );
    }
    case "pending":
      return `<div data-sf-slot="${node.slot}">${nodeToHtml(node.fallback, ids)}</div>`;
  }
}

export function escapeKey(key: string): string {
  return key.replace(/%/g, "%25").replace(/-/g, "%2D");
}

function subtreeAt(node: SfNode, path: number[]): SfNode {
  let current = node;
  for (const idx of path) {
    if (current.kind !== "seq") throw new Error("segment path walks through a non-seq node");
    current = current.children[idx];
  }
  return current;
}

/** Mirrors the server's segment serialization: the subtree wrapped in comment delimiters, recursing into child segments, so a swapped-in region stays diffable on the next navigation. */
export function renderSegment(node: SfNode, seg: Segment, ids: IdAlloc): string {
  let out = `<!--sf-g:${escapeKey(seg.k)}-->`;
  const inner = seg.c.find((c) => c.s === undefined && (c.p ?? []).length === 0);
  const positioned = seg.c.filter((c) => c.s === undefined && (c.p ?? []).length > 0);
  if (inner) {
    out += renderSegment(node, inner, ids);
  } else if (node.kind === "seq" && positioned.length > 0) {
    node.children.forEach((child, idx) => {
      const match = positioned.find((c) => (c.p ?? [])[0] === idx);
      out += match ? renderSegment(child, match, ids) : nodeToHtml(child, ids);
    });
  } else {
    out += nodeToHtml(node, ids);
  }
  return out + "<!--/sf-g-->";
}

export { subtreeAt };
