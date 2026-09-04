import { scan } from "./boot.js";
import { parsePayload, Payload, Segment, SfNode } from "./reader.js";
import { escapeKey, nodeToHtml, renderSegment, subtreeAt, IdAlloc } from "./render.js";

let current: Segment | null = null;
const ids: IdAlloc = { next: 0 };

interface Region {
  start: Comment;
  end: Comment;
}

/** A key is `module` or `module?params`; the module half decides whether two segments are the same kind of thing. */
function moduleOf(key: string): string {
  const q = key.indexOf("?");
  return q === -1 ? key : key.slice(0, q);
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
  if (oldSeg.k !== newSeg.k) {
    if (swap()) return true;
    // A region that cannot be replaced is the root, whose delimiters are
    // children of the document. Same module means the same chrome, so the
    // change is below it: retag the delimiter and descend rather than
    // demanding a full load.
    if (moduleOf(oldSeg.k) !== moduleOf(newSeg.k)) return false;
    const region = findRegion(oldSeg.k);
    if (!region) return false;
    region.start.data = `sf-g:${escapeKey(newSeg.k)}`;
  }
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

interface Cached {
  text: string;
  at: number;
}

/** Payload text by `pathname + search` or the fetch still bringing it. */
const cache = new Map<string, Cached | Promise<Cached | null>>();
let cacheMs = 30_000;

export type PrefetchTiming = "hover" | "none";

export interface NavigationOptions {
  /** Whether a link's payload is fetched ahead of its click (on hover, focus or touch) or never. Defaults to `"hover"`. */
  prefetch?: PrefetchTiming;
  /** How long a fetched payload answers a navigation before it is fetched again. Defaults to 30 seconds. */
  cacheMs?: number;
}

function payloadUrl(url: URL): string {
  return `${url.pathname}${url.search}${url.search ? "&" : "?"}__payload`;
}

function cacheKey(url: URL): string {
  return `${url.pathname}${url.search}`;
}

async function fetchPayload(url: URL): Promise<Cached | null> {
  const key = cacheKey(url);
  const inflight = (async () => {
    const res = await fetch(payloadUrl(url));
    if (!res.ok) return null;
    return { text: await res.text(), at: performance.now() };
  })();
  cache.set(key, inflight);
  const entry = await inflight;
  if (cache.get(key) === inflight) {
    if (entry) {
      cache.set(key, entry);
    } else {
      cache.delete(key);
    }
  }
  return entry;
}

/** The route's payload from the cache while it is fresh, else fetched and cached. `null` is a response that was not ok. */
async function payloadFor(url: URL): Promise<Cached | null> {
  const held = cache.get(cacheKey(url));
  if (held instanceof Promise) return held;
  if (held && performance.now() - held.at < cacheMs) return held;
  return fetchPayload(url);
}

/** Fetches a same-origin route's payload ahead of a click so the navigation that follows applies it without a round trip. A payload already held or in flight is left alone. */
export function prefetch(href: string): Promise<void> {
  const url = new URL(href, window.location.href);
  if (url.origin !== window.location.origin) return Promise.resolve();
  const held = cache.get(cacheKey(url));
  if (held instanceof Promise || (held && performance.now() - held.at < cacheMs)) return Promise.resolve();
  return fetchPayload(url).then(() => undefined);
}

/** Drops every held payload, which is what a mutation calls for. */
export function clearRouterCache(): void {
  cache.clear();
}

/** Revalidation after a mutation: drops the router cache, re-fetches the current route's payload and force-replaces the top-level child segments, so the layout's DOM survives while mutated content refreshes. */
export async function refresh(): Promise<void> {
  const bail = () => window.location.reload();
  if (!current) return bail();
  cache.clear();
  const res = await fetch(payloadUrl(new URL(window.location.href)));
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
  const held = await payloadFor(url);
  if (!held) {
    window.location.assign(href);
    return;
  }
  let patched = false;
  try {
    patched = apply(parsePayload(held.text));
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

function linkOf(target: EventTarget | null): Element | null {
  const anchor = (target as Element | null)?.closest?.("a[href]") ?? null;
  if (!anchor || anchor.hasAttribute("data-sf-native")) return null;
  return anchor;
}

/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links as they are hovered, focused or touched and owns history from then on. */
export function enableNavigation(options: NavigationOptions = {}): void {
  const sidecar = document.querySelector("script[data-sf-segments]");
  if (sidecar?.textContent) {
    current = JSON.parse(sidecar.textContent);
  }
  if (options.cacheMs !== undefined) cacheMs = options.cacheMs;
  document.addEventListener("click", (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
      return;
    }
    const anchor = linkOf(event.target);
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    const url = new URL(href, window.location.href);
    if (url.origin !== window.location.origin) return;
    event.preventDefault();
    void navigate(url.pathname + url.search);
  });
  if ((options.prefetch ?? "hover") === "hover") {
    const warm = (event: Event) => {
      const anchor = linkOf(event.target);
      if (!anchor || anchor.getAttribute("data-sf-prefetch") === "none") return;
      void prefetch(anchor.getAttribute("href") ?? "");
    };
    document.addEventListener("mouseover", warm);
    document.addEventListener("focusin", warm);
    document.addEventListener("touchstart", warm, { passive: true });
  }
  window.addEventListener("popstate", () => {
    void navigate(window.location.pathname + window.location.search, false);
  });
}
