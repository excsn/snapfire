import { adoptCatalog, adoptLocale } from "./locale.js";
import { isServerIsland, mountServer, patchServer } from "./server.js";
import { adopt } from "./store.js";
import { decodeValue, SfValue } from "./values.js";

export type Props = { [key: string]: SfValue };
export type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean) => unknown;
/** Re-renders a mounted island in place with new props; `handle` is what the mounter returned. */
export type Patcher = (handle: unknown, module: unknown, props: Props, el: Element) => void;
/** Ends a mounted island, running whatever its framework runs when a root goes away; `handle` is what the mounter returned. */
export type Unmounter = (handle: unknown, el: Element) => void;

export type MountTiming = "load" | "visible" | "idle";

export interface IslandEntry {
  loader: () => Promise<unknown>;
  mount: Mounter;
  /** When hydration happens: immediately, when scrolled into view or when the main thread is idle. Defaults to "load". Per island, not per page. */
  when?: MountTiming;
  patch?: Patcher;
  /** Called by `discard` for an island whose marker is leaving the document. Left out, the root is dropped as it stands. */
  unmount?: Unmounter;
  /** A layout its adapter mounts as one tree with its page: whether `marker`, the island in the layout's child region, is one the root renders itself. The scan then leaves the marker to the root and the navigator hands the root what the payload puts in that region. */
  claims?: (marker: Element) => boolean;
}

/** What a tree root shows in its child region: the page the payload put there, rendered by the root itself, else markup the root adopts as it stands. */
export interface TreeChild {
  /** The page's module id. Null for markup adopted as it stands. */
  module: string | null;
  /** The page module's export, resolved through the registry before the root renders it. */
  component?: unknown;
  props: Props;
  encoded?: unknown;
  /** What the payload said about the island regions inside the page. */
  regions: unknown;
  /** The markup the payload gave the page's children region. */
  children: string | null;
  /** The markup to adopt when the root does not render the child. */
  html: string;
  /** Whether the server rendered the page's markup. False for a page the build left to the browser, which the root renders after it has hydrated. */
  rendered: boolean;
  /** Counts replacements: a new value is a new instance of the child. */
  instance: number;
  /** Counts patches, so a placement consumes each payload once. */
  gen: number;
  /** The marker the page hydrates over; null for a child a payload brought. */
  marker: Element | null;
  /** The segment key the region's delimiters carry, as escaped in the comment. */
  key: string | null;
}

interface Mounted {
  entry: IslandEntry;
  moduleId: string;
  handle: Promise<unknown>;
  /** What the mounter returned, once it has; undefined while the loader is in flight or after a failed mount. */
  root: unknown;
  /** Set by `discard`. A loader that resolves after it mounts nothing. */
  gone: boolean;
  /** The props the island last mounted or patched with. */
  props: Props;
  /** What the payload behind the last patch said about the regions inside this island, for the adapter to hand its nested islands. */
  regions: unknown;
  /** The markup the payload behind the last patch gave the island's children region, null when it gave none. */
  children: string | null;
  /** For a tree root, what its child region shows. */
  child: TreeChild | null;
}

const mounted = new WeakMap<Element, Mounted>();

/** How to call off a mount whose timing has not fired, by marker. */
const pending = new WeakMap<Element, () => void>();

const islands = new Map<string, IslandEntry>();

export function registerIsland(moduleId: string, entry: IslandEntry): void {
  islands.set(moduleId, entry);
}

/** Every island registered so far, by module id. */
export function registeredIslands(): ReadonlyMap<string, IslandEntry> {
  return islands;
}

function rawPropsFor(root: ParentNode, id: string): unknown {
  const script = root.querySelector(`script[data-sf-props="${id}"]`) ?? document.querySelector(`script[data-sf-props="${id}"]`);
  if (!script || !script.textContent) return {};
  return JSON.parse(script.textContent);
}

function propsFor(root: ParentNode, id: string): Props {
  return decodeValue(rawPropsFor(root, id)) as Props;
}

/** An island marker's props script beside it and the props it holds, decoded and as written. */
export function markerProps(marker: Element): { script: Element | null; props: Props; encoded: unknown } {
  const next = marker.nextElementSibling;
  const script = next?.tagName === "SCRIPT" && next.getAttribute("data-sf-props") === marker.id ? next : null;
  const encoded = script?.textContent ? JSON.parse(script.textContent) : {};
  return { script, props: decodeValue(encoded) as Props, encoded };
}

/** The mounter for an island whose module defines a custom element: importing the module is the whole mount, since the element the server already wrote upgrades itself once its definition runs. What the island's timing schedules, then, is the import. */
export const defineMounter: Mounter = () => undefined;

/** Whether the server rendered this island's own markup, which is what decides hydrating over mounting. Slot regions do not count: a module the server never evaluated still carries one per plan child it must offer, so an element holding nothing else was rendered by nobody. */
export function serverRendered(el: Element): boolean {
  return Array.from(el.childNodes).some((node) => !(node instanceof Element && node.tagName === "SF-S"));
}

/** An island the nearest island above it has not rendered. Mounting that one builds its regions from markup it copies out, so this element is about to be replaced by a copy of itself: anything mounted into it now is discarded and the copy carries the `data-sf-scheduled` a scan would leave. The parent's own mount reaches it instead. */
function awaitingAnAncestor(el: Element): boolean {
  const above = el.parentElement?.closest("sf-i");
  return above !== null && above !== undefined && !serverRendered(above);
}

function mountNow(entry: IslandEntry, moduleId: string, el: Element, props: Props): void {
  const hydrate = serverRendered(el);
  const island: Mounted = { entry, moduleId, handle: Promise.resolve(undefined), root: undefined, gone: false, props, regions: null, children: null, child: null };
  island.handle = entry
    .loader()
    .then((mod) => (island.gone ? undefined : entry.mount(mod, props, el, hydrate)))
    .then((value) => {
      if (island.gone) return undefined;
      island.root = value;
      el.setAttribute(MOUNTED, "");
      return value;
    })
    .catch((err) => {
      console.warn(`sf: mounting ${moduleId} failed`, err);
      return undefined;
    });
  mounted.set(el, island);
}

/** Ends every island under `root`, `root` itself included when it is a marker, before the caller takes those nodes out of the document: a mount still waiting on its timing is called off, one whose loader is in flight mounts nothing when it lands and a mounted one is handed to its entry's `unmount`, nested islands before the island around them. What was mounted there is forgotten, so `islandState` answers null and `patchIsland` false. Call it on every node removed by anything other than a mounted root's own render, since a root left in a detached element keeps running: its effects never clean up and whatever they hold stays held. */
export function discard(root: ParentNode): void {
  const markers = Array.from(root.querySelectorAll("sf-i"));
  if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
  for (const el of markers.reverse()) {
    pending.get(el)?.();
    pending.delete(el);
    const island = mounted.get(el);
    if (!island) continue;
    island.gone = true;
    mounted.delete(el);
    if (island.root !== undefined) island.entry.unmount?.(island.root, el);
  }
}

/** The props an island last took, the regions the last payload described inside it and the markup it gave the island's children, for an adapter placing its nested islands and its children. Null when nothing is mounted at `el`. */
export function islandState(el: Element): { props: Props; regions: unknown; children: string | null; child: TreeChild | null } | null {
  const island = mounted.get(el);
  return island ? { props: island.props, regions: island.regions, children: island.children, child: island.child } : null;
}

/** The tree root whose child region holds `node`: the island around the bare `<sf-s>` that is `node`'s parent, when its entry claims markers. Null anywhere else. */
export function treeRootOf(node: Node | null): Element | null {
  const slot = node?.parentNode;
  if (!(slot instanceof Element) || slot.tagName !== "SF-S" || slot.hasAttribute("data-sf-island") || slot.hasAttribute("data-sf-name") || slot.hasAttribute("data-sf-children")) return null;
  const root = slot.parentElement?.closest("sf-i");
  if (!root) return null;
  const entry = islands.get(root.getAttribute("data-sf-module") ?? "");
  return entry?.claims ? root : null;
}

/** Registers `marker`, the page a tree root renders in its child region, as mounted by that root: `islandState` answers for it and `patchIsland` on it re-renders the root with the page's new props. The root's adapter calls it once the marker is in the document. */
export function adoptTreeChild(marker: Element, root: Element): void {
  const moduleId = marker.getAttribute("data-sf-module") ?? "";
  const page = islands.get(moduleId);
  const tree = mounted.get(root);
  if (!page || !tree?.child || mounted.has(marker)) return;
  const entry: IslandEntry = {
    loader: page.loader,
    mount: () => undefined,
    patch: (_handle, _module, props, el) => {
      const state = mounted.get(el);
      const child = mounted.get(root)?.child;
      if (!state || !child) return;
      child.props = props;
      child.regions = state.regions;
      child.children = state.children;
      child.gen += 1;
      void rerender(root);
    },
    unmount: () => undefined,
  };
  mounted.set(marker, { entry, moduleId, handle: Promise.resolve(root), root, gone: false, props: tree.child.props, regions: tree.child.regions, children: tree.child.children, child: null });
  marker.setAttribute(SCHEDULED, "");
  marker.setAttribute(MOUNTED, "");
}

async function rerender(root: Element): Promise<boolean> {
  const island = mounted.get(root);
  if (!island?.entry.patch) return false;
  const handle = await island.handle;
  if (handle === undefined) return false;
  const mod = await island.entry.loader();
  island.entry.patch(handle, mod, island.props, root);
  return true;
}

/** Tree work still landing: a child handed to a root whose page module is still loading. */
const landing = new Set<Promise<boolean>>();

/** Resolves once every child handed to a tree root has been rendered or refused. */
export function treeSettled(): Promise<void> {
  return Promise.all(landing).then(() => undefined);
}

/** Hands a tree root what its child region shows from now on: the page module is loaded through the registry, what the region held is ended, then the root re-renders with the child. False when nothing is mounted at `root` or its mount failed, which leaves the caller to write the markup itself. */
export function setTreeChild(root: Element, child: TreeChild): Promise<boolean> {
  const work = (async () => {
    const island = mounted.get(root);
    if (!island) return false;
    const handle = await island.handle;
    if (handle === undefined || island.gone) return false;
    const page = child.module === null ? undefined : islands.get(child.module);
    if (page) child.component = await page.loader();
    child.instance = (island.child?.instance ?? 0) + 1;
    for (const slot of Array.from(root.querySelectorAll("sf-s:not([data-sf-island]):not([data-sf-name]):not([data-sf-children])"))) {
      if (slot.parentElement?.closest("sf-i") === root) discard(slot);
    }
    island.child = child;
    return rerender(root);
  })();
  landing.add(work);
  void work.finally(() => landing.delete(work));
  return work;
}

/** Records what a tree root's child region holds at hydration, before the root's adapter renders it. */
export function holdTreeChild(root: Element, child: TreeChild): void {
  const island = mounted.get(root);
  if (island) island.child = child;
}

/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. `regions` is what the payload behind this patch says about the islands inside it and `children` the markup it gives the island's children region, both read back by the adapter through `islandState`. `encoded` is `props` as the server encoded them, which a server island hands back in place of encoding `props` again. False when nothing is mounted there or the island's entry has no patcher. */
export async function patchIsland(el: Element, props: Props, regions: unknown = null, children: string | null = null, encoded?: unknown): Promise<boolean> {
  if (isServerIsland(el)) return patchServer(el, props, encoded);
  const island = mounted.get(el);
  if (!island?.entry.patch) return false;
  const handle = await island.handle;
  if (handle === undefined) return false;
  const mod = await island.entry.loader();
  island.props = props;
  island.regions = regions;
  island.children = children;
  island.entry.patch(handle, mod, props, el);
  return true;
}

function schedule(entry: IslandEntry, moduleId: string, el: Element, props: Props): void {
  switch (entry.when ?? "load") {
    case "load":
      mountNow(entry, moduleId, el, props);
      return;
    case "visible": {
      const observer = new IntersectionObserver((entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          observer.disconnect();
          pending.delete(el);
          mountNow(entry, moduleId, el, props);
        }
      });
      pending.set(el, () => observer.disconnect());
      observer.observe(el);
      return;
    }
    case "idle": {
      let off = false;
      const run = () => {
        pending.delete(el);
        if (!off) mountNow(entry, moduleId, el, props);
      };
      pending.set(el, () => {
        off = true;
      });
      const idle = (window as { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback;
      if (idle) {
        idle(run);
      } else {
        setTimeout(run, 1);
      }
      return;
    }
  }
}

/** Marks a marker some scan has taken, so a rescan leaves it alone however its timing is still waiting. What it does not say is that anything mounted: [`MOUNTED`] says that. */
const SCHEDULED = "data-sf-scheduled";

/** Marks a marker whose mounter has run. A `visible` or `idle` island carries [`SCHEDULED`] from the scan that took it and this one only once its timing fired, which is the difference a test reads. */
const MOUNTED = "data-sf-mounted";

/** Mounts every unmounted island marker under `root`, honoring each island's timing: the `data-sf-when` of the region a page or layout placed it in, else the registry's. Idempotent. */
export function scan(root: ParentNode): void {
  for (const el of Array.from(root.querySelectorAll(`sf-i:not([${SCHEDULED}])`))) {
    const moduleId = el.getAttribute("data-sf-module");
    if (!moduleId) continue;
    if (awaitingAnAncestor(el)) continue;
    if (el.parentElement?.closest("sf-s[data-sf-mode]")?.getAttribute("data-sf-mode") === "server") {
      el.setAttribute(SCHEDULED, "");
      el.setAttribute(MOUNTED, "");
      mountServer(el, moduleId, rawPropsFor(root, el.id));
      continue;
    }
    const tree = treeRootOf(el);
    if (tree && islands.get(tree.getAttribute("data-sf-module") ?? "")?.claims?.(el)) {
      el.setAttribute(SCHEDULED, "");
      continue;
    }
    const entry = islands.get(moduleId);
    if (!entry) {
      missing.add(moduleId);
      arm();
      continue;
    }
    el.setAttribute(SCHEDULED, "");
    const placed = el.parentElement?.closest("sf-s[data-sf-when]")?.getAttribute("data-sf-when") as MountTiming | null;
    schedule(placed ? { ...entry, when: placed } : entry, moduleId, el, propsFor(root, el.id));
  }
}

/** Module ids no registry knew when a scan reached them. A miss is not yet a defect: a mounted site registers its islands when its own entry module runs, which is after the shell's `boot` has already scanned the document. */
const missing = new Set<string>();
/** Entry modules already imported, so a site's islands register once however many payloads name them. */
const entries = new Set<string>();
let loading = 0;
let armed = false;

function report(): void {
  if (loading > 0) return;
  for (const moduleId of missing) {
    if (!islands.has(moduleId)) console.warn(`sf: no island registered for ${moduleId}`);
  }
  missing.clear();
}

/** Settles the misses once every entry module in the document has run. Module scripts are deferred, so they all execute before `DOMContentLoaded`, which has not fired while the state is `loading` or `interactive`. */
function arm(): void {
  if (armed) return;
  armed = true;
  const run = () => {
    armed = false;
    report();
  };
  if (document.readyState === "complete") queueMicrotask(run);
  else document.addEventListener("DOMContentLoaded", run, { once: true });
}

/** Imports an entry module once and rescans, so the islands it registers mount. Call it before the scan that will miss them, so a miss is not reported while its registration is in flight. */
export function loadEntry(src: string): void {
  if (entries.has(src)) return;
  entries.add(src);
  loading += 1;
  import(src)
    .then(() => scan(document))
    .catch((err) => {
      entries.delete(src);
      console.warn(`sf: loading ${src} failed`, err);
    })
    .finally(() => {
      loading -= 1;
      report();
    });
}

const filling = new WeakSet<Document>();

/** Scans the document and keeps scanning as streamed slots fill in. Calling it again scans again without listening twice. */
export function boot(): void {
  const run = () => scan(document);
  adopt();
  adoptLocale();
  adoptCatalog();
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", run, { once: true });
  } else {
    run();
  }
  if (!filling.has(document)) {
    filling.add(document);
    document.addEventListener("sf:fill", run);
  }
}

/** The mark a stylesheet the client owns carries, written by the server on a document and by `applyStyles` on a navigation. A link without it belongs to the document and is never taken away. */
const CSS_MARK = "data-sf-css";

/** Brings the owned stylesheets to exactly `hrefs`: links already there stay, ones no longer named go and new ones are added after everything else so their rules still win. Resolves when the new ones have loaded or after `timeout` so a href that never answers cannot hold a navigation open. */
export function applyStyles(hrefs: string[], timeout = 2000): Promise<void> {
  const head = document.head;
  const wanted = new Set(hrefs.map((href) => new URL(href, location.href).href));
  const held = new Map<string, HTMLLinkElement>();
  for (const link of Array.from(head.querySelectorAll<HTMLLinkElement>(`link[${CSS_MARK}]`))) {
    held.set(link.href, link);
  }
  for (const [href, link] of held) {
    if (!wanted.has(href)) link.remove();
  }
  const pending: Promise<void>[] = [];
  for (const href of hrefs) {
    if (held.has(new URL(href, location.href).href)) continue;
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.setAttribute(CSS_MARK, "");
    link.href = href;
    pending.push(
      new Promise<void>((done) => {
        const settle = () => done();
        link.addEventListener("load", settle, { once: true });
        link.addEventListener("error", settle, { once: true });
        setTimeout(settle, timeout);
      }),
    );
    head.appendChild(link);
  }
  return pending.length === 0 ? Promise.resolve() : Promise.all(pending).then(() => undefined);
}
