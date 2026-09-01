import { scan } from "./boot.js";
import { parsePayload, Payload, Segment, SfNode } from "./reader.js";
import { escapeKey, nodeToHtml, renderSegment, subtreeAt, IdAlloc } from "./render.js";

let current: Segment | null = null;
const ids: IdAlloc = { next: 0 };

interface Region {
  start: Comment;
  end: Comment;
}

/** Finds the comment pair delimiting a segment's region, respecting nesting. */
function findRegion(key: string): Region | null {
  const open = `sf-g:${escapeKey(key)}`;
  const walker = document.createTreeWalker(document.documentElement.parentNode ?? document, NodeFilter.SHOW_COMMENT);
  let start: Comment | null = null;
  let depth = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = (node as Comment).data;
    if (!start) {
      if (text === open) start = node as Comment;
    } else if (text.startsWith("sf-g:")) {
      depth++;
    } else if (text === "/sf-g") {
      if (depth === 0) return { start, end: node as Comment };
      depth--;
    }
  }
  return null;
}

/** Fails when the region's parent cannot hold the replacement. The root segment's delimiters are children of the document, which admits no text nodes. Inserting before deleting keeps a refusal from emptying the page. */
function replaceRegion(region: Region, html: string): boolean {
  const parent = region.start.parentNode;
  if (!(parent instanceof Element)) return false;
  const template = document.createElement("template");
  template.innerHTML = html;
  parent.insertBefore(template.content, region.start);
  const range = document.createRange();
  range.setStartBefore(region.start);
  range.setEndAfter(region.end);
  range.deleteContents();
  return true;
}

function fillSlot(slot: number, node: SfNode): void {
  const el = document.querySelector(`[data-sf-slot="${slot}"]`);
  if (!el) return;
  const template = document.createElement("template");
  template.innerHTML = nodeToHtml(node, ids);
  el.replaceWith(template.content);
}

/** Walks old and new segment spines together; the first key mismatch swaps that region from the new payload. Slot-addressed children resolve through S rows instead. */
function diff(oldSeg: Segment, newSeg: Segment, newNode: SfNode): boolean {
  const swap = () => {
    const region = findRegion(oldSeg.k);
    return region ? replaceRegion(region, renderSegment(newNode, newSeg, ids)) : false;
  };
  if (oldSeg.k !== newSeg.k) return swap();
  const oldChildren = oldSeg.c.filter((c) => c.s === undefined);
  const newChildren = newSeg.c.filter((c) => c.s === undefined);
  if (oldChildren.length !== newChildren.length) return swap();
  for (let i = 0; i < newChildren.length; i++) {
    if (!diff(oldChildren[i], newChildren[i], subtreeAt(newNode, newChildren[i].p ?? []))) return false;
  }
  return true;
}

/** False when the payload could not be patched in place, which leaves the caller to fall back to a full load. */
function apply(payload: Payload): boolean {
  if (!current || !payload.segments) return false;
  if (!diff(current, payload.segments, payload.tree)) return false;
  current = payload.segments;
  for (const r of payload.resolutions) {
    fillSlot(r.slot, r.node);
  }
  scan(document);
  return true;
}

/** Revalidation after a mutation: re-fetches the current route's payload and force-replaces the top-level child segments, so the layout's DOM survives while mutated content refreshes. */
export async function refresh(): Promise<void> {
  const bail = () => window.location.reload();
  if (!current) return bail();
  const search = window.location.search;
  const res = await fetch(`${window.location.pathname}${search}${search ? "&" : "?"}__payload`);
  if (!res.ok) return bail();
  const payload = parsePayload(await res.text());
  if (!payload.segments) return bail();
  const oldChildren = current.c.filter((c) => c.s === undefined);
  const newChildren = payload.segments.c.filter((c) => c.s === undefined);
  if (oldChildren.length !== newChildren.length) return bail();
  for (let i = 0; i < newChildren.length; i++) {
    const region = findRegion(oldChildren[i].k);
    if (!region) return bail();
    if (!replaceRegion(region, renderSegment(subtreeAt(payload.tree, newChildren[i].p ?? []), newChildren[i], ids))) {
      return bail();
    }
  }
  current = payload.segments;
  for (const r of payload.resolutions) {
    fillSlot(r.slot, r.node);
  }
  scan(document);
}

export async function navigate(href: string, push = true): Promise<void> {
  const url = new URL(href, window.location.href);
  const fetchUrl = `${url.pathname}${url.search}${url.search ? "&" : "?"}__payload`;
  const res = await fetch(fetchUrl);
  if (!res.ok) {
    window.location.assign(href);
    return;
  }
  let patched = false;
  try {
    patched = apply(parsePayload(await res.text()));
  } catch {
    patched = false;
  }
  if (!patched) {
    window.location.assign(href);
    return;
  }
  if (push) history.pushState(null, "", href);
  window.scrollTo(0, 0);
}

/** Reads the sidecar the server embedded, intercepts same-origin link clicks and owns history from then on. */
export function enableNavigation(): void {
  const sidecar = document.querySelector("script[data-sf-segments]");
  if (sidecar?.textContent) {
    current = JSON.parse(sidecar.textContent);
  }
  document.addEventListener("click", (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
      return;
    }
    const anchor = (event.target as Element).closest?.("a[href]");
    if (!anchor || anchor.hasAttribute("data-sf-native")) return;
    const href = anchor.getAttribute("href") ?? "";
    const url = new URL(href, window.location.href);
    if (url.origin !== window.location.origin) return;
    event.preventDefault();
    void navigate(url.pathname + url.search);
  });
  window.addEventListener("popstate", () => {
    void navigate(window.location.pathname + window.location.search, false);
  });
}
