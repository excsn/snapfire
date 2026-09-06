import { loadEntry, patchIsland, scan } from "./boot.js";
import { catalog, currentLocale, setCatalog, setLocale } from "./locale.js";
import { Head, linesOf, parseRow, Segment, SfNode } from "./reader.js";
import { escapeKey, nodeToHtml, renderSegment, scriptSafeJson, subtreeAt, IdAlloc } from "./render.js";
import { seed, transaction } from "./store.js";
import { SfValue } from "./values.js";

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

/** What a named slot held before navigation first filled it: its fallback, or nothing. Emptying the slot puts it back. */
const fallbacks = new WeakMap<Element, string>();

/** Empties what an old child segment occupies: its delimited region, delimiters included, or, while it is still streaming, its slot element. The `<sf-s>` around it stays for the next fill, holding its fallback again. */
function removeChild(old: Segment): boolean {
  const region = findRegion(old.k);
  if (region) {
    const parent = region.start.parentNode;
    let node: Node | null = region.start;
    while (node) {
      const next: Node | null = node.nextSibling;
      node.parentNode?.removeChild(node);
      if (node === region.end) break;
      node = next;
    }
    if (parent instanceof Element && parent.hasAttribute("data-sf-name")) parent.innerHTML = fallbacks.get(parent) ?? "";
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
      if (!fallbacks.has(slot)) fallbacks.set(slot, slot.innerHTML);
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
  if (seg.keep?.includes("content")) {
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
/** The path the document is rooted at, which an intercept does not change: opening a drawer over the agent list puts `/settings` in the address bar while the page underneath is still `/agents`. */
let documentPath = "";

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

/** The eager wave of a payload: every row up to the `G` sidecar, which closes it. */
interface Eager {
  tree: SfNode;
  segments: Segment;
  heads: Head[];
  seeds: { [key: string]: SfValue }[];
  locale: string | null;
  catalog: { [key: string]: string } | null;
  entry: string | null;
}

/** Reads rows up to and including the sidecar, stepping the generator by hand so it stays open for the rows after. Null when the rows end first, or when a resolution arrives before it. */
async function eagerOf(rows: AsyncGenerator<string>): Promise<Eager | null> {
  let tree: SfNode | null = null;
  const eager: Omit<Eager, "tree" | "segments"> = { heads: [], seeds: [], locale: null, catalog: null, entry: null };
  for (;;) {
    const { done, value: line } = await rows.next();
    if (done) return null;
    const row = parseRow(line);
    switch (row.tag) {
      case "V":
        break;
      case "N":
        tree = row.tree;
        break;
      case "H":
        eager.heads.push(row.head);
        break;
      case "T":
        eager.seeds.push(row.seed);
        break;
      case "L":
        eager.locale = row.locale;
        break;
      case "E":
        eager.entry = row.entry;
        break;
      case "D":
        eager.catalog = row.catalog;
        break;
      case "G":
        return tree === null ? null : { ...eager, tree, segments: row.segments };
      case "S":
        return null;
    }
  }
}

/** False when the eager wave could not be patched in place, which leaves the caller to fall back to a full load. With `force`, a kept leaf that is not an island is replaced anyway, which is what revalidation asks for. */
function applyEager(eager: Eager, force: boolean): boolean {
  if (!current) return false;
  transaction(() => {
    for (const values of eager.seeds) seed(values);
  });
  if (!diff(current, eager.segments, eager.tree, force)) return false;
  current = eager.segments;
  openSlot = interceptSlot(eager.segments);
  for (const head of eager.heads) applyHead(head);
  if (eager.locale !== null) {
    if (eager.catalog !== null) setCatalog(eager.locale, eager.catalog);
    setLocale(eager.locale);
  }
  if (eager.entry !== null) loadEntry(eager.entry);
  scan(document);
  watchLinks(document);
  return true;
}

/** Counts navigations, so the rows still arriving for one stop applying once a later one has taken the document. */
let generation = 0;

/** Applies the rows after the sidecar as they arrive: each resolution into its slot, each head and seed as it comes. Stops at a row that cannot be read, leaving the fallbacks that stand. */
async function drain(rows: AsyncGenerator<string>, segments: Segment, gen: number): Promise<void> {
  try {
    for await (const line of rows) {
      if (gen !== generation) return;
      const row = parseRow(line);
      if (row.tag === "S") {
        fillSlot(row.slot, row.node, keyOfSlot(segments, row.slot));
        scan(document);
        watchLinks(document);
      } else if (row.tag === "H") {
        applyHead(row.head);
      } else if (row.tag === "T") {
        seed(row.seed);
      }
    }
  } catch (err) {
    console.warn("sf: a streamed payload stopped applying", err);
  }
}

/** A payload's rows as they arrive, readable from the first by every navigation that consumes it, before and after it is complete. */
class Feed {
  readonly lines: string[] = [];
  done = false;
  /** When the response finished, on the clock `performance.now` reads; 0 while it is still arriving. */
  at = 0;
  /** Whether the response was ok, known once its headers are. */
  readonly ok: Promise<boolean>;
  private settle: (ok: boolean) => void = () => {};
  private waiters: (() => void)[] = [];

  constructor() {
    this.ok = new Promise((resolve) => {
      this.settle = resolve;
    });
  }

  open(ok: boolean): void {
    this.settle(ok);
  }

  push(line: string): void {
    this.lines.push(line);
    this.wake();
  }

  finish(): void {
    this.done = true;
    this.at = performance.now();
    this.settle(false);
    this.wake();
  }

  private wake(): void {
    const waiting = this.waiters;
    this.waiters = [];
    for (const wake of waiting) wake();
  }

  /** Resolves once every row has landed. */
  async whole(): Promise<void> {
    while (!this.done) {
      await new Promise<void>((resolve) => this.waiters.push(resolve));
    }
  }

  async *read(): AsyncGenerator<string> {
    for (let i = 0; ; i++) {
      while (i >= this.lines.length) {
        if (this.done) return;
        await new Promise<void>((resolve) => this.waiters.push(resolve));
      }
      yield this.lines[i];
    }
  }
}

/** Starts fetching a payload and hands back its feed at once; the rows land in it as the body streams. */
function fetchFeed(url: URL, headers: Record<string, string>): Feed {
  const feed = new Feed();
  void (async () => {
    try {
      const res = await fetch(payloadUrl(url), { headers });
      feed.open(res.ok);
      if (!res.ok) return;
      for await (const line of linesOf(res)) feed.push(line);
    } catch {
    } finally {
      feed.finish();
    }
  })();
  return feed;
}

/** Payload feeds by where the navigation comes from and where it goes, complete or still arriving. */
const cache = new Map<string, Feed>();
let cacheMs = 30_000;

/** A held feed answers while it is still arriving and for `cacheMs` after it finished. */
function fresh(feed: Feed): boolean {
  return !feed.done || performance.now() - feed.at < cacheMs;
}

export type PrefetchTiming = "hover" | "viewport" | "none";

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
  const held = currentLocale();
  if (held && catalog(held) !== null) headers["x-sf-catalog"] = held;
  return headers;
}

export interface NavigationOptions {
  /** When a link's payload is fetched ahead of its click: on hover, focus or touch, as the link enters the viewport, or never. A link's own `data-sf-prefetch` overrides it. Defaults to `"hover"`. */
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

/** Fetches a payload into the cache; a response that is not ok leaves the cache without it. */
function fetchPayload(url: URL, ask: Ask): Feed {
  const key = cacheKey(url, ask);
  const feed = fetchFeed(url, headersOf(ask));
  cache.set(key, feed);
  void feed.ok.then((ok) => {
    if (!ok && cache.get(key) === feed) cache.delete(key);
  });
  return feed;
}

/** The route's payload from the cache while it is fresh, else fetched and cached. */
function payloadFor(url: URL, ask: Ask): Feed {
  const held = cache.get(cacheKey(url, ask));
  if (held && fresh(held)) return held;
  return fetchPayload(url, ask);
}

/** The document's timing for a link that names none. */
let fallbackPrefetch: PrefetchTiming = "hover";

/** A link's own timing, else the document's. */
function timingOf(anchor: Element): PrefetchTiming {
  const own = anchor.getAttribute("data-sf-prefetch");
  return own === "hover" || own === "viewport" || own === "none" ? own : fallbackPrefetch;
}

let watched = new WeakSet<Element>();
let viewport: IntersectionObserver | null = null;

/** Drops the observer and what it watched, so a second `enableNavigation` observes the document again under the timing it was given rather than the one before it. */
function resetViewport(): void {
  viewport?.disconnect();
  viewport = null;
  watched = new WeakSet<Element>();
}

/** Observes every link under `root` whose timing is `viewport` and is not observed already; a link that enters the view is prefetched once and dropped. Called after every application, since a navigation brings new links. */
function watchLinks(root: ParentNode): void {
  if (typeof IntersectionObserver !== "function") return;
  viewport ??= new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      viewport?.unobserve(entry.target);
      void prefetch(entry.target.getAttribute("href") ?? "", askOf(entry.target));
    }
  });
  for (const anchor of Array.from(root.querySelectorAll("a[href]"))) {
    if (watched.has(anchor) || timingOf(anchor) !== "viewport") continue;
    watched.add(anchor);
    viewport.observe(anchor);
  }
}

/** Fetches a same-origin route's payload ahead of a click so the navigation that follows applies it without a round trip. A payload already held or in flight is left alone. Resolves once the payload has arrived whole. */
export async function prefetch(href: string, options: NavigateOptions = {}): Promise<void> {
  const url = new URL(href, window.location.href);
  if (url.origin !== window.location.origin) return;
  await payloadFor(url, askFor(options)).whole();
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
  const gen = ++generation;
  const feed = fetchFeed(new URL(window.location.href), headersOf({ from: null, into: openSlot }));
  if (!(await feed.ok)) return bail();
  const rows = feed.read();
  const eager = await eagerOf(rows).catch(() => null);
  if (gen !== generation) return;
  if (!eager || !patch(eager, true)) return bail();
  await drain(rows, eager.segments, gen);
}

/** `applyEager` with a throw counted as a patch that failed. */
function patch(eager: Eager, force: boolean): boolean {
  try {
    return applyEager(eager, force);
  } catch {
    return false;
  }
}

/** Navigates to `href` by payload, from the document's current path unless `options` say otherwise. The eager wave is applied and history moves as soon as the sidecar arrives, deferred segments showing their fallbacks; each resolution fills its slot as it lands, and the promise resolves once the payload has been applied whole. An intercepted navigation opens in its slot without scrolling; anything else scrolls to the top. */
export async function navigate(href: string, push = true, options: NavigateOptions = {}): Promise<void> {
  const url = new URL(href, window.location.href);
  const gen = ++generation;
  const feed = payloadFor(url, askFor(options));
  if (!(await feed.ok)) {
    if (gen === generation) window.location.assign(href);
    return;
  }
  const rows = feed.read();
  const eager = await eagerOf(rows).catch(() => null);
  if (gen !== generation) return;
  if (!eager || !patch(eager, false)) {
    window.location.assign(href);
    return;
  }
  if (push) history.pushState(null, "", href);
  currentPath = `${url.pathname}${url.search}`;
  if (openSlot === null) {
    documentPath = currentPath;
    window.scrollTo(0, 0);
  }
  await drain(rows, eager.segments, gen);
}

/** The page the document is showing, which is not always what the address bar says: an intercepted navigation puts the target's URL there while the page underneath stays. Empty before `enableNavigation` runs. */
export function currentDocumentPath(): string {
  return documentPath;
}

/** The page the document is showing, under another locale: its path with the current locale's prefix replaced by `to`. Nothing else is rewritten, and a path given explicitly is used as it stands. This is what a language switcher links to, so choosing a language keeps the reader where they are instead of sending them wherever the switcher happens to live. */
export function localePath(to: string, from?: string): string {
  const path = from ?? documentPath ?? "";
  const at = path || (typeof window === "undefined" ? "/" : `${window.location.pathname}${window.location.search}`);
  const cut = at.indexOf("?");
  const search = cut === -1 ? "" : at.slice(cut);
  let rest = cut === -1 ? at : at.slice(0, cut);
  const tag = currentLocale();
  if (tag) {
    if (rest === `/${tag}`) rest = "";
    else if (rest.startsWith(`/${tag}/`)) rest = rest.slice(tag.length + 1);
  }
  if (rest === "/") rest = "";
  return `/${to}${rest}${search}`;
}

function linkOf(target: EventTarget | null): Element | null {
  const anchor = (target as Element | null)?.closest?.("a[href]") ?? null;
  if (!anchor || anchor.hasAttribute("data-sf-native")) return null;
  return anchor;
}

/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links as they are hovered, focused or touched, or as they enter the viewport where one asks for that, and owns history from then on. */
export function enableNavigation(options: NavigationOptions = {}): void {
  const g = globalThis as { __sf?: Record<string, unknown> };
  g.__sf = Object.assign(g.__sf ?? {}, { refresh });
  const sidecar = document.querySelector("script[data-sf-segments]");
  if (sidecar?.textContent) {
    current = JSON.parse(sidecar.textContent);
  }
  openSlot = null;
  currentPath = `${window.location.pathname}${window.location.search}`;
  documentPath = currentPath;
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
  fallbackPrefetch = options.prefetch ?? "hover";
  resetViewport();
  const warm = (event: Event) => {
    const anchor = linkOf(event.target);
    if (!anchor || timingOf(anchor) !== "hover") return;
    void prefetch(anchor.getAttribute("href") ?? "", askOf(anchor));
  };
  document.addEventListener("mouseover", warm);
  document.addEventListener("focusin", warm);
  document.addEventListener("touchstart", warm, { passive: true });
  watchLinks(document);
  document.addEventListener("sf:fill", () => watchLinks(document));
  window.addEventListener("popstate", () => {
    void navigate(window.location.pathname + window.location.search, false);
  });
}
