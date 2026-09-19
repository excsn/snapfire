import { applyStyles, discard, loadEntry, patchIsland, scan, setTreeChild, treeRootOf, treeSettled, type TreeChild } from "./boot.js";
import { catalog, currentLocale, setCatalog, setLocale } from "./locale.js";
import { Head, linesOf, parseRow, Segment, SfNode } from "./reader.js";
import { childrenOf, escapeKey, nodeToHtml, propsScript, regionSources, renderSegment, subtreeAt, IdAlloc } from "./render.js";
import { morphElement, morphNodes, type MorphHooks } from "./server.js";
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

/** Ends the islands between a region's delimiters, before the nodes between them go. */
function discardRegion(region: Region): void {
  for (let n: Node | null = region.start.nextSibling; n && n !== region.end; n = n.nextSibling) {
    if (n instanceof Element) discard(n);
  }
}

/** Writes `html` into `el` the way the parser reads a document, declarative shadow roots included, where the browser has `setHTMLUnsafe`. Through `innerHTML` a `<template shadowrootmode>` stays a child, which the element's `shadowOf` answers. */
function writeMarkup(el: Element, html: string): void {
  const unsafe = (el as Element & { setHTMLUnsafe?: (html: string) => void }).setHTMLUnsafe;
  if (typeof unsafe === "function") unsafe.call(el, html);
  else el.innerHTML = html;
}

/** Fails when the region's parent cannot hold the replacement. The root segment's delimiters are children of the document, which admits no text nodes. Inserting before deleting keeps a refusal from emptying the page. */
function replaceRegion(region: Region, html: string): boolean {
  const parent = region.start.parentNode;
  if (!(parent instanceof Element)) return false;
  const template = document.createElement("template");
  writeMarkup(template, html);
  parent.insertBefore(template.content, region.start);
  discardRegion(region);
  const range = document.createRange();
  range.setStartBefore(region.start);
  range.setEndAfter(region.end);
  range.deleteContents();
  return true;
}

/** Fills a streamed slot with its content, delimited as the region its segment key names, so a later navigation can diff it. */
function fillSlot(slot: number, node: SfNode, seg: Segment | null): void {
  const el = document.querySelector(`[data-sf-slot="${slot}"]`);
  if (!el) return;
  const root = treeRootOf(el);
  if (root) {
    treeChild(root, node, seg);
    return;
  }
  const template = document.createElement("template");
  const html = nodeToHtml(node, ids);
  writeMarkup(template, seg === null ? html : `<!--sf-g:${escapeKey(seg.k)}-->${html}<!--/sf-g-->`);
  discard(el);
  el.replaceWith(template.content);
}

/** The segment a slot id resolves, from a sidecar. */
function segmentOfSlot(seg: Segment, slot: number): Segment | null {
  if (seg.s === slot) return seg;
  for (const child of seg.c) {
    const found = segmentOfSlot(child, slot);
    if (found !== null) return found;
  }
  return null;
}

/** Hands a tree root what its child region shows now. A page, which is a client node with no child segments, is described for the root to render; anything else is markup for it to adopt, delimited the way the region was. */
function treeChild(root: Element, node: SfNode, seg: Segment | null): void {
  const key = seg === null ? null : escapeKey(seg.k);
  const html = seg === null ? nodeToHtml(node, ids) : renderSegment(node, seg, ids);
  const adopted: TreeChild = { module: null, props: {}, regions: null, children: null, html, rendered: true, instance: 0, gen: 0, marker: null, key };
  const child: TreeChild =
    node.kind === "client" && seg !== null && seg.c.length === 0
      ? { module: node.module, props: node.props, encoded: node.encoded, regions: regionSources(node, ids), children: childrenOf(node, ids), html, rendered: node.ssr !== null || node.children.length > 0, instance: 0, gen: 0, marker: null, key }
      : adopted;
  void setTreeChild(root, child);
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

/** What a named slot held before navigation first filled it: its fallback or nothing. Emptying the slot puts it back. */
const fallbacks = new WeakMap<Element, string>();

/** Empties what an old child segment occupies: its delimited region, delimiters included, or, while it is still streaming, its slot element. The `<sf-s>` around it stays for the next fill, holding its fallback again. */
function removeChild(old: Segment): boolean {
  const region = findRegion(old.k);
  if (region) {
    const parent = region.start.parentNode;
    discardRegion(region);
    let node: Node | null = region.start;
    while (node) {
      const next: Node | null = node.nextSibling;
      node.parentNode?.removeChild(node);
      if (node === region.end) break;
      node = next;
    }
    if (parent instanceof Element && parent.hasAttribute("data-sf-name")) {
      discard(parent);
      writeMarkup(parent, fallbacks.get(parent) ?? "");
    }
    return true;
  }
  if (old.s === undefined) return false;
  const el = document.querySelector(`[data-sf-slot="${old.s}"]`);
  if (!el) return false;
  discard(el);
  el.remove();
  return true;
}

/** The `<sf-s data-sf-name>` a kept layout region holds for `name`: the one under the layout's own island, not a nested one's, or, for a layout nothing mounts, the one directly in the region's markup. */
function namedSlotOf(region: Region, name: string): Element | null {
  const island = islandOf(region);
  if (!island) {
    for (let n: Node | null = region.start.nextSibling; n && n !== region.end; n = n.nextSibling) {
      if (!(n instanceof Element)) continue;
      const found = n.matches(`sf-s[data-sf-name="${name}"]`) ? [n] : Array.from(n.querySelectorAll(`sf-s[data-sf-name="${name}"]`));
      for (const slot of found) {
        const above = slot.parentElement?.closest("sf-i");
        if (!above || !isBetween(above, region)) return slot;
      }
    }
    return null;
  }
  for (const slot of Array.from(island.el.querySelectorAll(`sf-s[data-sf-name="${name}"]`))) {
    if (slot.parentElement?.closest("sf-i") === island.el) return slot;
  }
  return null;
}

/** Replaces what an old child segment occupies: its delimited region, or, while it is still streaming, its slot element. `seg` is the segment `node` renders as, null for a pending node written bare. Under a tree root the node is handed to the root instead, which renders or adopts it. */
function replaceChild(old: Segment, node: SfNode, seg: Segment | null): boolean {
  const html = () => (seg === null ? nodeToHtml(node, ids) : renderSegment(node, seg, ids));
  const region = findRegion(old.k);
  if (region) {
    const root = treeRootOf(region.start);
    if (root) {
      treeChild(root, node, seg);
      return true;
    }
    return replaceRegion(region, html());
  }
  if (old.s === undefined) return false;
  const el = document.querySelector(`[data-sf-slot="${old.s}"]`);
  if (!el) return false;
  const root = treeRootOf(el);
  if (root) {
    treeChild(root, node, seg);
    return true;
  }
  const template = document.createElement("template");
  writeMarkup(template, html());
  discard(el);
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

/** Hands a kept island the props the new payload carries, the regions the payload describes inside it and the markup of its children region, so it re-renders in place with its DOM and its state and the islands nested under it follow. Its props script is rewritten for the next mount. */
function patchProps(region: Region, node: SfNode): void {
  if (node.kind !== "client") return;
  const island = islandOf(region);
  if (!island) return;
  const json = propsScript(node);
  const children = childrenOf(node, ids);
  if (island.script?.textContent === json && children === null) return;
  if (island.script) island.script.textContent = json;
  void patchIsland(island.el, node.props, regionSources(node, ids), children, node.encoded);
}

/** Walks old and new segment spines together. A segment whose digest the two payloads agree on rendered the same, so its DOM is kept whatever its key became; otherwise the first key mismatch swaps that region from the new payload. With `keep`, a mismatch of the same module is morphed in place instead, so every island its new markup places again keeps its DOM and its state, unless the new segment carries a slot over untouched, which its markup does not hold. A kept region whose node is an island takes the new props in place. Children pair by slot name: a slot the new payload fills and the old did not is written into the layout's `<sf-s data-sf-name>`, a slot it no longer fills is emptied and a slot it says to keep carries over untouched. Slot-addressed children resolve through S rows instead. */
function diff(oldSeg: Segment, newSeg: Segment, newNode: SfNode, force: boolean, keep: boolean): boolean {
  const swap = () => replaceChild(oldSeg, newNode, newSeg);
  const paired = moduleOf(oldSeg.k) === moduleOf(newSeg.k);
  const same = paired && oldSeg.d !== undefined && oldSeg.d === newSeg.d;
  const morphs = keep && paired && (newNode.kind === "client" || !newSeg.keep?.length);
  let key = oldSeg.k;
  if (oldSeg.k !== newSeg.k) {
    // A region that cannot be replaced is the root, whose delimiters are
    // children of the document. Same module means the same chrome, so the
    // change is below it: retag the delimiter and descend rather than
    // demanding a full load.
    if (!same && morphs && newNode.kind !== "client" && morphStatic(oldSeg.k, newNode, newSeg)) return true;
    if (!same && !morphs) {
      if (replaceChild(oldSeg, newNode, newSeg)) return true;
      if (!paired) return false;
    }
    const region = findRegion(oldSeg.k);
    if (!region) return false;
    region.start.data = `sf-g:${escapeKey(newSeg.k)}`;
    key = newSeg.k;
  }
  const named = newSeg.c.every((c) => c.n !== undefined) && oldSeg.c.every((c) => c.n !== undefined);
  if (!named && oldSeg.c.length !== newSeg.c.length) return swap();
  if (newNode.kind === "client") {
    if (!same) {
      const region = findRegion(key);
      if (region) patchProps(region, newNode);
    }
  } else if (staticChanged(oldSeg, newSeg, same, force)) {
    return morphStatic(key, newNode, newSeg) || swap();
  }
  const untouched = newSeg.keep ?? [];
  const carried: Segment[] = [];
  if (named) {
    for (const oldChild of oldSeg.c) {
      if (newSeg.c.some((c) => c.n === oldChild.n)) continue;
      if (untouched.includes(oldChild.n ?? "")) {
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
      discard(slot);
      if (newChild.s !== undefined) {
        const pending = pendingOf(newNode, newChild.s);
        if (!pending) return false;
        writeMarkup(slot, nodeToHtml(pending, ids));
      } else {
        writeMarkup(slot, renderSegment(subtreeAt(newNode, newChild.p ?? []), newChild, ids));
      }
      continue;
    }
    if (newChild.s !== undefined) {
      // Streaming again: the old child's place takes the slot with its
      // fallback; the resolution fills it with the delimited content.
      const pending = pendingOf(newNode, newChild.s);
      if (!pending || !replaceChild(oldChild, pending, null)) return false;
      continue;
    }
    if (!diff(oldChild, newChild, subtreeAt(newNode, newChild.p ?? []), force, keep)) return false;
  }
  newSeg.c.push(...carried);
  return true;
}

/** The island regions a static segment's markup holds directly: every `<sf-s data-sf-island data-sf-region>` between the delimiters that is not inside another island's own markup, by region key. Islands nested in a root move with the root. */
function islandRegionsIn(region: Region): Map<string, Element> {
  const out = new Map<string, Element>();
  for (let n: Node | null = region.start.nextSibling; n && n !== region.end; n = n.nextSibling) {
    if (!(n instanceof Element)) continue;
    const found = n.matches("sf-s[data-sf-island][data-sf-region]") ? [n] : [];
    found.push(...Array.from(n.querySelectorAll("sf-s[data-sf-island][data-sf-region]")));
    for (const slot of found) {
      const above = slot.parentElement?.closest("sf-i");
      if (above && isBetween(above, region)) continue;
      const key = slot.getAttribute("data-sf-region");
      if (key) out.set(key, slot);
    }
  }
  return out;
}

/** Whether `el` sits between a region's delimiters. */
function isBetween(el: Element, region: Region): boolean {
  for (let n: Node | null = region.start.nextSibling; n && n !== region.end; n = n.nextSibling) {
    if (n === el || (n instanceof Element && n.contains(el))) return true;
  }
  return false;
}

/** Patches a static segment's markup in place by the rules of `morph`, so every element that stands where it stood keeps its DOM, a scrolled pane its scroll and a focused control its value. An island the new payload places again, by region key, keeps its root and its state wherever in the region it stood and takes the new props in place. A root nothing has mounted yet reads the rewritten props script when its turn comes. False when the region is not in the document, which leaves the caller to swap. */
function morphStatic(key: string, node: SfNode, seg: Segment): boolean {
  const region = findRegion(key);
  if (!region) return false;
  const parent = region.start.parentNode;
  if (!(parent instanceof Element)) return false;
  const template = document.createElement("template");
  writeMarkup(template, renderSegment(node, seg, ids));
  const kept = islandRegionsIn(region);
  const sources = regionSources(node, ids);
  const old: Node[] = [];
  for (let n: Node | null = region.start; n; n = n.nextSibling) {
    old.push(n);
    if (n === region.end) break;
  }
  const hooks: MorphHooks = {
    nested: (current, next) => takeIsland(current, next, sources, hooks),
    adopt: (found) => (found.startsWith("region:") ? (kept.get(found.slice("region:".length)) ?? null) : null),
    drop: (node) => {
      if (node instanceof Element) discard(node);
    },
  };
  morphNodes(parent, old, Array.from(template.content.childNodes), region.end.nextSibling, hooks);
  return true;
}

/** An island marker the new markup places again. A root that was scheduled, under a region the payload describes, keeps its DOM and its state: its props script and the island take the new props. Any other marker is swapped for the new one, whose props script is patched in beside it for the next scan to mount. */
function takeIsland(current: Element, next: Element, sources: ReturnType<typeof regionSources>, hooks: MorphHooks): void {
  const after = current.nextElementSibling;
  const script = after?.tagName === "SCRIPT" && after.getAttribute("data-sf-props") === current.id ? after : null;
  const wrapper = current.parentElement;
  const regionKey = wrapper?.hasAttribute("data-sf-island") ? wrapper.getAttribute("data-sf-region") : null;
  const source = regionKey === null ? undefined : sources.get(regionKey);
  if (source && current.hasAttribute("data-sf-scheduled")) {
    if (script) script.textContent = propsScript(source);
    void patchIsland(current, source.props, source.nested, source.children, source.encoded);
    return;
  }
  const wanted = next.nextElementSibling;
  discard(current);
  current.replaceWith(document.importNode(next, true));
  if (script && wanted?.tagName === "SCRIPT") morphElement(script, wanted, hooks);
}

/** Whether a segment nothing mounts has to be replaced, child segments included: its own markup changed. Nothing else can carry new markup into it. The root is never one: its own markup is the document, whose head `applyHead` already handles. */
function staticChanged(oldSeg: Segment, newSeg: Segment, same: boolean, force: boolean): boolean {
  const known = oldSeg.d !== undefined && newSeg.d !== undefined;
  if (newSeg.c.length === 0) return known ? !same : force;
  return oldSeg.n !== undefined && known && !same;
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
  styles: string[];
}

/** Reads rows up to and including the sidecar, stepping the generator by hand so it stays open for the rows after. Null when the rows end first or when a resolution arrives before it. */
async function eagerOf(rows: AsyncGenerator<string>): Promise<Eager | null> {
  let tree: SfNode | null = null;
  const eager: Omit<Eager, "tree" | "segments"> = { heads: [], seeds: [], locale: null, catalog: null, entry: null, styles: [] };
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
      case "C":
        eager.styles = row.styles;
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

/** False when the eager wave could not be patched in place, which leaves the caller to fall back to a full load. With `force`, a kept leaf that is not an island is replaced anyway, which is what revalidation asks for. With `keep`, a segment whose key changed within its module is morphed rather than replaced. */
function applyEager(eager: Eager, force: boolean, keep: boolean): boolean {
  if (!current) return false;
  transaction(() => {
    for (const values of eager.seeds) seed(values);
  });
  if (!diff(current, eager.segments, eager.tree, force, keep)) return false;
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

/** Applies the rows after the sidecar as they arrive: each resolution into its slot, each head and seed as it comes, with `sf:fill` dispatched on `document` for each filled slot the way the fill script does for a streamed one. Stops at a row that cannot be read, leaving the fallbacks that stand. */
async function drain(rows: AsyncGenerator<string>, segments: Segment, gen: number): Promise<void> {
  try {
    for await (const line of rows) {
      if (gen !== generation) return;
      const row = parseRow(line);
      if (row.tag === "S") {
        fillSlot(row.slot, row.node, segmentOfSlot(segments, row.slot));
        await treeSettled();
        scan(document);
        watchLinks(document);
        document.dispatchEvent(new CustomEvent("sf:fill", { detail: row.slot }));
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
  /** The session generation the page had when this was fetched; a later one means the session moved and this answers nothing. */
  state = "";
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
      const payload = res.ok || (res.headers.get("content-type") ?? "").includes("x-sf-payload");
      feed.open(payload);
      if (!payload) return;
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

/** A held feed answers while it is still arriving and for `cacheMs` after it finished, under the session generation it was fetched in. */
function fresh(feed: Feed): boolean {
  return (!feed.done || performance.now() - feed.at < cacheMs) && feed.state === sessionState();
}

/** The cookie the host writes beside the session cookie whenever it saves a written session, a fresh value each time. Whatever posted the write, the client's own `action`, a form another library sent or a tab beside this one, the generation moves; every payload fetched under the old one is stale. Empty where there is no cookie or no document. */
const STATE_COOKIE = "sf_state=";

function sessionState(): string {
  if (typeof document === "undefined" || typeof document.cookie !== "string") return "";
  for (const part of document.cookie.split(";")) {
    const cookie = part.trim();
    if (cookie.startsWith(STATE_COOKIE)) return cookie.slice(STATE_COOKIE.length);
  }
  return "";
}

export type PrefetchTiming = "hover" | "viewport" | "none";

/** How a navigation asks for its payload: `from` is the document's path, which lets the server intercept the target into a live layout's slot and marks the links inside the intercept by the page it opens over; `into` names that slot outright, with `from` sent for the marks alone; neither is a full page. */
interface Ask {
  from: string | null;
  into: string | null;
}

export interface NavigateOptions {
  /** The document's rendering of the target, never an intercept. */
  full?: boolean;
  /** Renders the target into this slot of the nearest live layout that declares it. */
  into?: string;
  /** Replaces the current history entry rather than adding one. */
  replace?: boolean;
  /** Whether a segment whose key changed but whose module did not is morphed in place, keeping every island its new markup places again with its DOM and its state, rather than replaced. Defaults to true when the target has the current pathname, which means only the query changed. Otherwise it defaults to false. */
  keep?: boolean;
  /** Whether the window scrolls to the element the target's fragment names or to the top when it names none. Defaults to true; false leaves the window where it is. */
  scroll?: boolean;
}

function askFor(options: NavigateOptions): Ask {
  if (options.full) return { from: null, into: null };
  if (options.into) return { from: documentPath, into: options.into };
  return { from: currentPath, into: null };
}

function askOf(anchor: Element): NavigateOptions {
  const keep = anchor.getAttribute("data-sf-keep");
  return { full: anchor.hasAttribute("data-sf-full"), into: anchor.getAttribute("data-sf-into") ?? undefined, keep: keep === null ? undefined : keep !== "false" };
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
  /** When a link's payload is fetched ahead of its click: on hover, focus or touch; as the link enters the viewport; or never. A link's own `data-sf-prefetch` overrides it. Defaults to `"hover"`. */
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
  feed.state = sessionState();
  cache.set(key, feed);
  void feed.ok.then((ok) => {
    if (!ok && cache.get(key) === feed) cache.delete(key);
  });
  return feed;
}

/** The route's payload from the cache while it is fresh, else fetched and cached. A held feed from before the session moved is dropped on the way past, along with every other one fetched under that generation. */
function payloadFor(url: URL, ask: Ask): Feed {
  const held = cache.get(cacheKey(url, ask));
  if (held && fresh(held)) return held;
  if (held && held.state !== sessionState()) {
    for (const [key, feed] of cache) if (feed.state === held.state) cache.delete(key);
  }
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
  if (url.hash && `${url.pathname}${url.search}` === currentPath) return;
  url.hash = "";
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
  // Before the swap, so the response's own stylesheets are in the document by
  // the time its markup is and the browser never paints it unstyled.
  if (eager) await applyStyles(eager.styles);
  if (gen !== generation) return;
  if (!eager || !patch(eager, true, false)) return bail();
  await treeSettled();
  announce();
  await drain(rows, eager.segments, gen);
}

/** Tells whatever else works on the document that the navigator has just applied a payload to it: the eager wave of a navigation or a refresh, before its deferred segments arrive. A library that wires the markup it finds, htmx for one, processes the document again on it. */
function announce(): void {
  document.dispatchEvent(new CustomEvent("sf:navigate", { detail: { path: currentPath } }));
}

/** `applyEager` with a throw counted as a patch that failed. */
function patch(eager: Eager, force: boolean, keep: boolean): boolean {
  try {
    const applied = applyEager(eager, force, keep);
    if (!applied) console.warn("sf: the payload could not be patched in place; loading the document instead");
    return applied;
  } catch (err) {
    console.warn("sf: patching the payload threw; loading the document instead", err);
    return false;
  }
}

/** Scrolls to the element a fragment names, by id and then by an anchor's name, the way a browser does; to the top when it names none. */
function scrollToFragment(hash: string): void {
  let id = hash.slice(1);
  try {
    id = decodeURIComponent(id);
  } catch {
    // A malformed escape is looked up as written.
  }
  const target = id ? (document.getElementById(id) ?? Array.from(document.querySelectorAll("a[name]")).find((a) => a.getAttribute("name") === id)) : null;
  if (target) target.scrollIntoView();
  else window.scrollTo(0, 0);
}

/** Navigates to `href` by payload, from the document's current path unless `options` say otherwise. The eager wave is applied and history moves as soon as the sidecar arrives, deferred segments showing their fallbacks; each resolution fills its slot as it lands and the promise resolves once the payload has been applied whole. A navigation that changes only the query keeps the islands the page places again, unless `options.keep` says otherwise. An intercepted navigation opens in its slot without scrolling, as does one whose `options.scroll` is false; anything else scrolls to the element its fragment names or to the top. A fragment of the page already showing scrolls without fetching, as does a step back or forward within that page. */
export async function navigate(href: string, push = true, options: NavigateOptions = {}): Promise<void> {
  const url = new URL(href, window.location.href);
  const record = (same: boolean) => (options.replace || same ? history.replaceState(null, "", href) : history.pushState(null, "", href));
  if (!options.full && !options.into && `${url.pathname}${url.search}` === currentPath && (url.hash !== "" || !push)) {
    if (push) record(url.href === window.location.href);
    if (options.scroll !== false) scrollToFragment(url.hash);
    return;
  }
  const keep = options.keep ?? url.pathname === currentPath.split("?")[0];
  const page = new URL(url.href);
  page.hash = "";
  const gen = ++generation;
  const feed = payloadFor(page, askFor(options));
  if (!(await feed.ok)) {
    if (gen === generation) window.location.assign(href);
    return;
  }
  const rows = feed.read();
  const eager = await eagerOf(rows).catch(() => null);
  if (gen !== generation) return;
  if (eager) await applyStyles(eager.styles);
  if (gen !== generation) return;
  if (!eager || !patch(eager, false, keep)) {
    window.location.assign(href);
    return;
  }
  if (push) record(false);
  currentPath = `${url.pathname}${url.search}`;
  if (openSlot === null) {
    documentPath = currentPath;
    if (options.scroll !== false) scrollToFragment(url.hash);
  }
  markLinks();
  await treeSettled();
  announce();
  await drain(rows, eager.segments, gen);
}

/** The page the document is showing, which is not always what the address bar says: an intercepted navigation puts the target's URL there while the page underneath stays. Empty before `enableNavigation` runs. */
export function currentDocumentPath(): string {
  return documentPath;
}

/** The path and search in the address bar as the navigator last set them: the target of the last navigation, intercepted or not. Empty before `enableNavigation` runs. */
export function currentAddressPath(): string {
  return currentPath;
}

/** The path a marked anchor is judged against, query dropped: the document's for one carrying `data-sf-current="document"`, else the address. */
function markedAgainst(anchor: Element): string {
  const at = anchor.getAttribute("data-sf-current") === "document" ? documentPath : currentPath;
  const cut = at.indexOf("?");
  return cut === -1 ? at : at.slice(0, cut);
}

/** The mark an anchor carries on `path`, by the rule its `data-sf-link` names: `page` where its href is the page being shown, `true` where the page is under a `prefix` link, `null` where neither. The href is read as written, so one carrying a query or a fragment never matches; the path a request matched holds neither. */
function markOf(anchor: Element, path: string): string | null {
  const href = anchor.getAttribute("href");
  if (href === null) return null;
  const prefix = anchor.getAttribute("data-sf-link") === "prefix";
  if (href === path) return prefix ? "true" : "page";
  return prefix && path.startsWith(`${href}/`) ? "true" : null;
}

/** Brings every `<a data-sf-link>` under `root` to the page it is judged against: the address for most, the page beneath an open intercept for one that says `data-sf-current="document"`. The server writes the mark at first paint; this keeps it right across a navigation, which re-renders the page segment and leaves the layout holding the nav alone. */
export function markLinks(root: ParentNode = document): void {
  for (const anchor of Array.from(root.querySelectorAll("a[data-sf-link]"))) {
    const mark = markOf(anchor, markedAgainst(anchor));
    if (mark === null) anchor.removeAttribute("aria-current");
    else anchor.setAttribute("aria-current", mark);
  }
}

/** The page the document is showing, under another locale: its path with the current locale's prefix replaced by `to`. Nothing else is rewritten and a path given explicitly is used as it stands. This is what a language switcher links to, so choosing a language keeps the reader where they are instead of sending them wherever the switcher happens to live. */
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

/** The document the navigator is wired to. The sidecar it holds is the document's first paint, stale after any applied payload, so a second call on the same document keeps the spine the last navigation installed rather than reading it again. */
let wired: Document | null = null;

/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links when they are hovered, focused or touched; or as they enter the viewport where one asks for that. It owns history from then on. A second call on the same document, which a mounted site's entry module makes when a payload imports it, keeps the spine, the paths and the listeners the first one installed and changes only the options it names. */
export function enableNavigation(options: NavigationOptions = {}): void {
  const g = globalThis as { __sf?: Record<string, unknown> };
  g.__sf = Object.assign(g.__sf ?? {}, { refresh });
  if (options.cacheMs !== undefined) cacheMs = options.cacheMs;
  if (wired === document) {
    if (options.prefetch !== undefined && options.prefetch !== fallbackPrefetch) {
      fallbackPrefetch = options.prefetch;
      resetViewport();
      watchLinks(document);
    }
    return;
  }
  wired = document;
  const sidecar = document.querySelector("script[data-sf-segments]");
  current = sidecar?.textContent ? JSON.parse(sidecar.textContent) : null;
  openSlot = null;
  currentPath = `${window.location.pathname}${window.location.search}`;
  documentPath = currentPath;
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
    void navigate(url.pathname + url.search + url.hash, true, askOf(anchor));
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
  document.addEventListener("sf:fill", () => {
    watchLinks(document);
    markLinks();
  });
  window.addEventListener("popstate", () => {
    void navigate(window.location.pathname + window.location.search + window.location.hash, false);
  });
}
