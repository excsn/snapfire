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
    if (current.kind !== "seq" && current.kind !== "client") throw new Error("segment path walks through a node with no children");
    current = current.children[idx];
  }
  return current;
}

/** Mirrors the server's segment serialization: the subtree wrapped in comment delimiters, recursing into child segments, so a swapped-in region stays diffable on the next navigation. */
export function renderSegment(node: SfNode, seg: Segment, ids: IdAlloc): string {
  let out = `<!--sf-g:${escapeKey(seg.k)}-->`;
  const inner = seg.c.find((c) => c.s === undefined && (c.p ?? []).length === 0);
  if (inner) {
    out += renderSegment(node, inner, ids);
  } else {
    const positioned = seg.c.filter((c) => c.s === undefined && (c.p ?? []).length > 0).map((c) => ({ path: c.p ?? [], seg: c }));
    out += renderPositioned(node, positioned, ids);
  }
  return out + "<!--/sf-g-->";
}

/** `node` with each positioned child segment wrapped at its path, descending through seq items and an island's children alike. */
function renderPositioned(node: SfNode, positioned: { path: number[]; seg: Segment }[], ids: IdAlloc): string {
  if (positioned.length === 0) return nodeToHtml(node, ids);
  const items = (children: SfNode[]): string =>
    children
      .map((child, idx) => {
        const here = positioned.filter((c) => c.path[0] === idx).map((c) => ({ path: c.path.slice(1), seg: c.seg }));
        const exact = here.find((c) => c.path.length === 0);
        return exact ? renderSegment(child, exact.seg, ids) : renderPositioned(child, here, ids);
      })
      .join("");
  if (node.kind === "seq") return items(node.children);
  if (node.kind === "client" && !node.ssr) {
    const id = `sf-c${ids.next++}`;
    return `<sf-i id="${id}" data-sf-module="${node.module}">${items(node.children)}</sf-i><script type="application/json" data-sf-props="${id}">${scriptSafeJson(node.props)}</script>`;
  }
  return nodeToHtml(node, ids);
}

export { scriptSafeJson };

export { subtreeAt };
