import { patchIsland, scan } from "./boot.js";
import { Head, parsePayload, Payload, Segment, SfNode } from "./reader.js";
import { escapeKey, nodeToHtml, renderSegment, scriptSafeJson, subtreeAt, IdAlloc } from "./render.js";

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

/** Fills a streamed slot with its content, delimited as the region its segment key names, so a later navigation can diff it. */
function fillSlot(slot: number, node: SfNode, key: string | null): void {
  const el = document.querySelector(`[data-sf-slot="${slot}"]`);
  if (!el) return;
  const template = document.createElement("template");
  const html = nodeToHtml(node, ids);
  template.innerHTML = key === null ? html : `<!--sf-g:${escapeKey(key)}-->${html}<!--/sf-g-->`;
  el.replaceWith(template.content);
}

/** The key of the segment a slot id resolves, from a sidecar. */
function keyOfSlot(seg: Segment, slot: number): string | null {
  if (seg.s === slot) return seg.k;
  for (const child of seg.c) {
    const found = keyOfSlot(child, slot);
    if (found !== null) return found;
  }
  return null;
}

/** The pending node a slot id names, anywhere under `node`. */
function pendingOf(node: SfNode, slot: number): SfNode | null {
  if (node.kind === "pending") return node.slot === slot ? node : null;
  if (node.kind === "seq" || node.kind === "client") {
    for (const child of node.children) {
      const found = pendingOf(child, slot);
      if (found) return found;
    }
  }
  return null;
}

/** Empties what an old child segment occupies: its delimited region, delimiters included, or, while it is still streaming, its slot element. The `<sf-s>` around it stays for the next fill. */
function removeChild(old: Segment): boolean {
  const region = findRegion(old.k);
  if (region) {
    let node: Node | null = region.start;
    while (node) {
      const next: Node | null = node.nextSibling;
      node.parentNode?.removeChild(node);
      if (node === region.end) break;
      node = next;
    }
    return true;
  }
  if (old.s === undefined) return false;
  const el = document.querySelector(`[data-sf-slot="${old.s}"]`);
  if (!el) return false;
  el.remove();
  return true;
}

/** The `<sf-s data-sf-name>` a kept layout region holds for `name`: the one under the layout's own island, not a nested one's. */
function namedSlotOf(region: Region, name: string): Element | null {
  const island = islandOf(region);
  if (!island) return null;
  for (const slot of Array.from(island.el.querySelectorAll(`sf-s[data-sf-name="${name}"]`))) {
    if (slot.parentElement?.closest("sf-i") === island.el) return slot;
  }
  return null;
}

/** Replaces what an old child segment occupies: its delimited region, or, while it is still streaming, its slot element. */
function replaceChild(old: Segment, html: string): boolean {
  const region = findRegion(old.k);
  if (region) return replaceRegion(region, html);
  if (old.s === undefined) return false;
  const el = document.querySelector(`[data-sf-slot="${old.s}"]`);
  if (!el) return false;
  const template = document.createElement("template");
  template.innerHTML = html;
  el.replaceWith(template.content);
  return true;
}

/** The island a kept region holds, when its node is one: the first `sf-i` between the delimiters, with its props script. */
function islandOf(region: Region): { el: Element; script: Element | null } | null {
  for (let n: Node | null = region.start.nextSibling; n && n !== region.end; n = n.nextSibling) {
    if (n instanceof Element && n.tagName === "SF-I") {
      const next = n.nextSibling;
      const script = next instanceof Element && next.tagName === "SCRIPT" && next.getAttribute("data-sf-props") === n.id ? next : null;
      return { el: n, script };
    }
  }
  return null;
}

/** Hands a kept island the props the new payload carries, so it re-renders in place with its DOM and its state. Its props script is rewritten for the next mount. */
function patchProps(region: Region, node: SfNode): void {
  if (node.kind !== "client") return;
  const island = islandOf(region);
  if (!island) return;
  const json = scriptSafeJson(node.props);
  if (island.script?.textContent === json) return;
  if (island.script) island.script.textContent = json;
  void patchIsland(island.el, node.props);
}

/** Walks old and new segment spines together; the first key mismatch swaps that region from the new payload. A kept region whose node is an island takes the new props in place. Children pair by slot name: a slot the new payload fills and the old did not is written into the layout's `<sf-s data-sf-name>`, a slot it no longer fills is emptied, and a slot it says to keep carries over untouched. Slot-addressed children resolve through S rows instead. */
function diff(oldSeg: Segment, newSeg: Segment, newNode: SfNode, force: boolean): boolean {
  const swap = () => replaceChild(oldSeg, renderSegment(newNode, newSeg, ids));
  if (oldSeg.k !== newSeg.k) {
    if (replaceChild(oldSeg, renderSegment(newNode, newSeg, ids))) return true;
    // A region that cannot be replaced is the root, whose delimiters are
    // children of the document. Same module means the same chrome, so the
    // change is below it: retag the delimiter and descend rather than
    // demanding a full load.
    if (moduleOf(oldSeg.k) !== moduleOf(newSeg.k)) return false;
    const region = findRegion(oldSeg.k);
    if (!region) return false;
    region.start.data = `sf-g:${escapeKey(newSeg.k)}`;
  }
  const named = newSeg.c.every((c) => c.n !== undefined) && oldSeg.c.every((c) => c.n !== undefined);
  if (!named && oldSeg.c.length !== newSeg.c.length) return swap();
  if (newNode.kind === "client") {
    const region = findRegion(oldSeg.k);
    if (region) patchProps(region, newNode);
  } else if (force && newSeg.c.length === 0) {
    return swap();
  }
  const keep = newSeg.keep ?? [];
  const carried: Segment[] = [];
  if (named) {
    for (const oldChild of oldSeg.c) {
      if (newSeg.c.some((c) => c.n === oldChild.n)) continue;
      if (keep.includes(oldChild.n ?? "")) {
        carried.push(oldChild);
        continue;
      }
      if (!removeChild(oldChild)) return false;
    }
  }
  for (let i = 0; i < newSeg.c.length; i++) {
    const newChild = newSeg.c[i];
    const oldChild = named ? oldSeg.c.find((c) => c.n === newChild.n) : oldSeg.c[i];
    if (!oldChild) {
      const region = findRegion(newSeg.k);
      const slot = region && newChild.n !== undefined ? namedSlotOf(region, newChild.n) : null;
      if (!slot) return false;
      if (newChild.s !== undefined) {
        const pending = pendingOf(newNode, newChild.s);
        if (!pending) return false;
        slot.innerHTML = nodeToHtml(pending, ids);
      } else {
        slot.innerHTML = renderSegment(subtreeAt(newNode, newChild.p ?? []), newChild, ids);
      }
      continue;
    }
    if (newChild.s !== undefined) {
      // Streaming again: the old child's place takes the slot with its
      // fallback; the resolution fills it with the delimited content.
      const pending = pendingOf(newNode, newChild.s);
      if (!pending || !replaceChild(oldChild, nodeToHtml(pending, ids))) return false;
      continue;
    }
    if (!diff(oldChild, newChild, subtreeAt(newNode, newChild.p ?? []), force)) return false;
  }
  newSeg.c.push(...carried);
  return true;
}

/** The slot an intercepted payload fills: the child, of the segment that keeps its page, that is not kept. */
function interceptSlot(seg: Segment): string | null {
  if (seg.keep && seg.keep.length > 0) {
    return seg.c.find((c) => c.n !== undefined && !seg.keep?.includes(c.n))?.n ?? null;
  }
  for (const child of seg.c) {
    const found = interceptSlot(child);
    if (found !== null) return found;
  }
  return null;
}

/** The slot the current URL is rendered into, when the last navigation was intercepted; a refresh asks for the same. */
let openSlot: string | null = null;

/** The document's path and search as the navigator last left them, which is where the next navigation comes from. */
let currentPath = "";

/** Sets the document's title and description meta from a payload's `H` row; a field the row left out is left alone. */
export function applyHead(head: Head): void {
  if (head.title !== undefined) document.title = head.title;
  if (head.description !== undefined) {
    let meta = document.head.querySelector('meta[name="description"]');
    if (!meta) {
      meta = document.createElement("meta");
      meta.setAttribute("name", "description");
      document.head.appendChild(meta);
    }
    meta.setAttribute("content", head.description);
  }
}

/** False when the payload could not be patched in place, which leaves the caller to fall back to a full load. With `force`, a kept leaf that is not an island is replaced anyway, which is what revalidation asks for. */
function apply(payload: Payload, force: boolean): boolean {
  if (!current || !payload.segments) return false;
  if (!diff(current, payload.segments, payload.tree, force)) return false;
  current = payload.segments;
  openSlot = interceptSlot(payload.segments);
  for (const r of payload.resolutions) {
    fillSlot(r.slot, r.node, keyOfSlot(payload.segments, r.slot));
  }
  for (const head of payload.heads) applyHead(head);
  scan(document);
  return true;
}

interface Cached {
  text: string;
  at: number;
}

/** Payload text by where the navigation comes from and where it goes, or the fetch still bringing it. */
const cache = new Map<string, Cached | Promise<Cached | null>>();
let cacheMs = 30_000;

export type PrefetchTiming = "hover" | "none";

/** How a navigation asks for its payload: `from` is the document's path, which lets the server intercept the target into a live layout's slot; `into` names that slot outright; neither is a full page. */
interface Ask {
  from: string | null;
  into: string | null;
}

export interface NavigateOptions {
  /** The document's rendering of the target, never an intercept. */
  full?: boolean;
  /** Renders the target into this slot of the nearest live layout that declares it. */
  into?: string;
}

function askFor(options: NavigateOptions): Ask {
  if (options.full) return { from: null, into: null };
  if (options.into) return { from: null, into: options.into };
  return { from: currentPath, into: null };
}

function askOf(anchor: Element): NavigateOptions {
  return { full: anchor.hasAttribute("data-sf-full"), into: anchor.getAttribute("data-sf-into") ?? undefined };
}

function headersOf(ask: Ask): Record<string, string> {
  const headers: Record<string, string> = {};
  if (ask.from !== null) headers["x-sf-from"] = ask.from;
  if (ask.into !== null) headers["x-sf-into"] = ask.into;
  return headers;
}

export interface NavigationOptions {
  /** Whether a link's payload is fetched ahead of its click (on hover, focus or touch) or never. Defaults to `"hover"`. */
  prefetch?: PrefetchTiming;
  /** How long a fetched payload answers a navigation before it is fetched again. Defaults to 30 seconds. */
  cacheMs?: number;
}

function payloadUrl(url: URL): string {
  return `${url.pathname}${url.search}${url.search ? "&" : "?"}__payload`;
}

function cacheKey(url: URL, ask: Ask): string {
  return `${ask.from ?? ""}|${ask.into ?? ""}|${url.pathname}${url.search}`;
}

async function fetchPayload(url: URL, ask: Ask): Promise<Cached | null> {
  const key = cacheKey(url, ask);
  const inflight = (async () => {
    const res = await fetch(payloadUrl(url), { headers: headersOf(ask) });
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
async function payloadFor(url: URL, ask: Ask): Promise<Cached | null> {
  const held = cache.get(cacheKey(url, ask));
  if (held instanceof Promise) return held;
  if (held && performance.now() - held.at < cacheMs) return held;
  return fetchPayload(url, ask);
}

/** Fetches a same-origin route's payload ahead of a click so the navigation that follows applies it without a round trip. A payload already held or in flight is left alone. */
export function prefetch(href: string, options: NavigateOptions = {}): Promise<void> {
  const url = new URL(href, window.location.href);
  if (url.origin !== window.location.origin) return Promise.resolve();
  const ask = askFor(options);
  const held = cache.get(cacheKey(url, ask));
  if (held instanceof Promise || (held && performance.now() - held.at < cacheMs)) return Promise.resolve();
  return fetchPayload(url, ask).then(() => undefined);
}

/** Drops every held payload, which is what a mutation calls for. */
export function clearRouterCache(): void {
  cache.clear();
}

/** Revalidation after a mutation: drops the router cache, re-fetches the current route's payload and applies it, every kept island taking its new props in place and every kept region that is not an island replaced, so layouts and pages keep their DOM and their state while what they show follows the mutation. */
export async function refresh(): Promise<void> {
  const bail = () => window.location.reload();
  if (!current) return bail();
  cache.clear();
  const res = await fetch(payloadUrl(new URL(window.location.href)), { headers: headersOf({ from: null, into: openSlot }) });
  if (!res.ok) return bail();
  let patched = false;
  try {
    patched = apply(parsePayload(await res.text()), true);
  } catch {
    patched = false;
  }
  if (!patched) return bail();
}

/** Navigates to `href` by payload, from the document's current path unless `options` say otherwise. An intercepted navigation opens in its slot without scrolling; anything else scrolls to the top. */
export async function navigate(href: string, push = true, options: NavigateOptions = {}): Promise<void> {
  const url = new URL(href, window.location.href);
  const held = await payloadFor(url, askFor(options));
  if (!held) {
    window.location.assign(href);
    return;
  }
  let patched = false;
  try {
    patched = apply(parsePayload(held.text), false);
  } catch {
    patched = false;
  }
  if (!patched) {
    window.location.assign(href);
    return;
  }
  if (push) history.pushState(null, "", href);
  currentPath = `${url.pathname}${url.search}`;
  if (openSlot === null) window.scrollTo(0, 0);
}

function linkOf(target: EventTarget | null): Element | null {
  const anchor = (target as Element | null)?.closest?.("a[href]") ?? null;
  if (!anchor || anchor.hasAttribute("data-sf-native")) return null;
  return anchor;
}

/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links as they are hovered, focused or touched and owns history from then on. */
export function enableNavigation(options: NavigationOptions = {}): void {
  const g = globalThis as { __sf?: Record<string, unknown> };
  g.__sf = Object.assign(g.__sf ?? {}, { refresh });
  const sidecar = document.querySelector("script[data-sf-segments]");
  if (sidecar?.textContent) {
    current = JSON.parse(sidecar.textContent);
  }
  openSlot = null;
  currentPath = `${window.location.pathname}${window.location.search}`;
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
    void navigate(url.pathname + url.search, true, askOf(anchor));
  });
  if ((options.prefetch ?? "hover") === "hover") {
    const warm = (event: Event) => {
      const anchor = linkOf(event.target);
      if (!anchor || anchor.getAttribute("data-sf-prefetch") === "none") return;
      void prefetch(anchor.getAttribute("href") ?? "", askOf(anchor));
    };
    document.addEventListener("mouseover", warm);
    document.addEventListener("focusin", warm);
    document.addEventListener("touchstart", warm, { passive: true });
  }
  window.addEventListener("popstate", () => {
    void navigate(window.location.pathname + window.location.search, false);
  });
}
