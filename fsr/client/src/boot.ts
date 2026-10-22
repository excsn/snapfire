import { enableEmbeds } from "./embed.js";
import { adoptCatalog, adoptLocale } from "./locale.js";
import { loadDeclaredScripts, loadMarkedScript } from "./script.js";
import { settlePlaced, unplaced } from "./places.js";
import { enablePwa } from "./pwa.js";
import { REGION_KEY } from "./render.js";
import { endServer, isServerIsland, mountServer, patchServer } from "./server.js";
import { adopt, isPending, whenSettled } from "./store.js";
import { decodeValue, isRef, SfValue } from "./values.js";

export type Props = { [key: string]: SfValue };
export type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean) => unknown;
/** Re-renders a mounted island in place with new props; `handle` is what the mounter returned. */
export type Patcher = (handle: unknown, module: unknown, props: Props, el: Element) => void;
/** Ends a mounted island, running whatever its framework runs when a root goes away; `handle` is what the mounter returned. */
export type Unmounter = (handle: unknown, el: Element) => void;
/** With `paused` true, hides a mounted island in place while its DOM is out of the document, its state kept and its effects stopped. With `paused` false, shows it again, returning a promise that settles once the island is shown when the framework shows it later. False when the framework cannot pause, which leaves the caller to end the island instead. */
export type Pauser = (handle: unknown, el: Element, paused: boolean) => boolean | Promise<void>;

export type MountTiming = "load" | "visible" | "idle";

export interface IslandEntry {
  loader: () => Promise<unknown>;
  mount: Mounter;
  /** When hydration happens: immediately, when scrolled into view or when the main thread is idle. Defaults to "load". Per island, not per page. */
  when?: MountTiming;
  patch?: Patcher;
  /** Called by `discard` for an island whose marker is leaving the document. Left out, the root is dropped as it stands. */
  unmount?: Unmounter;
  /** Called by `release` for an island whose page activity holds and by `resume` when the page comes back. Left out, `release` ends the island. */
  pause?: Pauser;
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
  /** The markup the payload behind the last patch gave the island's markup prop regions, by region number. */
  markup: { [n: string]: string } | null;
}

const mounted = new WeakMap<Element, Mounted>();

/** Islands `release` paused, which `resume` shows again. */
const paused = new WeakSet<Element>();

/** How to call off a mount whose timing has not fired, by marker. */
const pending = new WeakMap<Element, () => void>();

const islands = new Map<string, IslandEntry>();

export function registerIsland(moduleId: string, entry: IslandEntry): void {
  islands.set(moduleId, entry);
}

const modules = new Map<string, () => Promise<unknown>>();

/** Registers a module the browser imports a function prop from, by the file's path as the build names it, so a module reference `file#export` among an island's props resolves to that export before the island mounts. */
export function registerModule(file: string, loader: () => Promise<unknown>): void {
  modules.set(file, loader);
}

/** `props` with every module reference, `file#export`, replaced by the export it names, imported through the module's registered loader; a reference to a module no registry knew stays as it is and is warned about once. */
async function resolveModuleRefs(props: Props): Promise<Props> {
  const refs: { id: string; set: (value: SfValue) => void }[] = [];
  const walk = (value: SfValue, set: (value: SfValue) => void): void => {
    if (isRef(value)) {
      if (value.kind === "module") refs.push({ id: value.id, set });
      return;
    }
    if (Array.isArray(value)) {
      value.forEach((item, i) => walk(item, (v) => ((value as SfValue[])[i] = v)));
      return;
    }
    if (value && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
      const record = value as { [key: string]: SfValue };
      for (const key of Object.keys(record)) walk(record[key], (v) => (record[key] = v));
    }
  };
  for (const key of Object.keys(props)) walk(props[key], (v) => (props[key] = v));
  if (refs.length === 0) return props;
  await Promise.all(
    refs.map(async ({ id, set }) => {
      const at = id.lastIndexOf("#");
      const file = at < 0 ? id : id.slice(0, at);
      const name = at < 0 ? "default" : id.slice(at + 1);
      const loader = modules.get(file);
      if (!loader) {
        console.warn(`sf: no module registered for ${file}`);
        return;
      }
      const mod = (await loader()) as { [key: string]: SfValue };
      set(mod[name]);
    }),
  );
  return props;
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

/** The mount before the next one, so islands mount in the order they were scheduled, document order for a scan, whatever order their modules land in. A framework's module state then advances across a page's islands in the order the server rendered them, as it does across the components of one app. Modules still load in parallel. */
let lastMount: Promise<unknown> = Promise.resolve();

/** Islands `remount` asked to mount afresh rather than hydrate the markup they hold. */
const fresh = new WeakSet<Element>();

function mountNow(entry: IslandEntry, moduleId: string, el: Element, props: Props): void {
  const hydrate = !fresh.has(el) && serverRendered(el);
  fresh.delete(el);
  const island: Mounted = { entry, moduleId, handle: Promise.resolve(undefined), root: undefined, gone: false, props, regions: null, children: null, markup: null };
  const ready = Promise.all([entry.loader(), resolveModuleRefs(props)]);
  const turn = lastMount.then(() => ready);
  island.handle = turn
    .then(([mod, resolved]) => {
      island.props = resolved;
      return island.gone ? undefined : entry.mount(mod, resolved, el, hydrate);
    })
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
  lastMount = island.handle;
  mounted.set(el, island);
}

/** Ends every island under `root`, `root` itself included when it is a marker, before the caller takes those nodes out of the document: a mount still waiting on its timing is called off, one whose loader is in flight mounts nothing when it lands and a mounted one is handed to its entry's `unmount`, nested islands before the island around them. What was mounted there is forgotten, so `islandState` answers null and `patchIsland` false. Call it on every node removed by anything other than a mounted root's own render, since a root left in a detached element keeps running: its effects never clean up and whatever they hold stays held. */
export function discard(root: ParentNode): void {
  const markers = Array.from(root.querySelectorAll("sf-i"));
  if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
  for (const el of markers.reverse()) {
    pending.get(el)?.();
    pending.delete(el);
    paused.delete(el);
    endServer(el);
    const island = mounted.get(el);
    if (!island) continue;
    island.gone = true;
    mounted.delete(el);
    if (island.root !== undefined) island.entry.unmount?.(island.root, el);
  }
}

/** Readies every island under `root` for a stay outside the document that may end with the nodes put back. A mounted island whose entry pauses is paused and kept. Any other island whose entry has an `unmount` is ended as `discard` ends it and its marker unmarked, so a later morph replaces it and a scan mounts it afresh; a server island and a mount still waiting on its timing the same. An island with no `unmount`, a custom element's, stays as it is: the browser disconnects the element and connects it again when the nodes come back. */
export function release(root: ParentNode): void {
  const markers = Array.from(root.querySelectorAll("sf-i"));
  if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
  for (const el of markers.reverse()) {
    const island = mounted.get(el);
    if (island && !island.entry.unmount) continue;
    if (island && island.root !== undefined && island.entry.pause?.(island.root, el, true)) {
      paused.add(el);
      continue;
    }
    discard(el);
    el.removeAttribute(SCHEDULED);
    el.removeAttribute(MOUNTED);
  }
}

/** Shows again every island under `root` that `release` paused, now that its nodes are back in the document. Settles once each adapter that says when has shown its island. */
export function resume(root: ParentNode): Promise<void> {
  const markers = Array.from(root.querySelectorAll("sf-i"));
  if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
  const shown: Promise<void>[] = [];
  for (const el of markers) {
    if (!paused.delete(el)) continue;
    const island = mounted.get(el);
    const showing = island && island.root !== undefined ? island.entry.pause?.(island.root, el, false) : undefined;
    if (showing instanceof Promise) shown.push(showing);
  }
  return Promise.all(shown).then(() => {});
}

/** The props an island last took, the regions the last payload described inside it and the markup it gave the island's children, for an adapter placing its nested islands and its children. Null when nothing is mounted at `el`. */
export function islandState(el: Element): { props: Props; regions: unknown; children: string | null; markup: { [n: string]: string } | null } | null {
  const island = mounted.get(el);
  return island ? { props: island.props, regions: island.regions, children: island.children, markup: island.markup } : null;
}

/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. `regions` is what the payload behind this patch says about the islands inside it and `children` the markup it gives the island's children region, both read back by the adapter through `islandState`. `encoded` is `props` as the server encoded them, which a server island hands back in place of encoding `props` again. False when nothing is mounted there or the island's entry has no patcher. */
export async function patchIsland(el: Element, props: Props, regions: unknown = null, children: string | null = null, encoded?: unknown, markup: { [n: string]: string } | null = null): Promise<boolean> {
  if (isServerIsland(el)) return patchServer(el, props, encoded);
  const island = mounted.get(el);
  if (!island?.entry.patch) return false;
  const handle = await island.handle;
  if (handle === undefined) return false;
  const mod = await island.entry.loader();
  props = await resolveModuleRefs(props);
  island.props = props;
  island.regions = regions;
  island.children = children;
  island.markup = markup;
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

/** The name placed content gives the island that placed it: its region key, else its id. */
function islandName(el: Element, props: Props): string {
  const key = props[REGION_KEY];
  return typeof key === "string" ? key : el.id;
}

/** Whether the island named `name`, by its region key or else its id, is mounted in the document. */
export function mountedIsland(name: string): boolean {
  return Array.from(document.querySelectorAll(`sf-i[${MOUNTED}]`)).some((el) => el.id === name || markerProps(el).props[REGION_KEY] === name);
}

/** Ends the island named `name` (its region key or else its id) and mounts it again afresh. */
export function remount(name: string): void {
  for (const el of Array.from(document.querySelectorAll("sf-i"))) {
    if (el.id !== name && markerProps(el).props[REGION_KEY] !== name) continue;
    discard(el);
    el.removeAttribute(SCHEDULED);
    el.removeAttribute(MOUNTED);
    fresh.add(el);
    if (el.parentNode) scan(el.parentNode);
    return;
  }
}

/** Mounts every unmounted island marker under `root`, honoring each island's timing: the `data-sf-when` of the region a page or layout placed it in, else the registry's. An island with placed content whose place has not arrived waits while a streamed slot is still pending, which is the only way the place can still come. Once nothing can bring it, the content is dropped and the island mounts afresh, since a framework cannot hydrate content it finds nowhere. Idempotent. */
export function scan(root: ParentNode): void {
  settlePlaced();
  const orphans = unplaced();
  const streaming = document.querySelector("[data-sf-slot]") !== null;
  for (const el of Array.from(root.querySelectorAll(`sf-i:not([${SCHEDULED}])`))) {
    const moduleId = el.getAttribute("data-sf-module");
    if (!moduleId) continue;
    if (awaitingAnAncestor(el)) continue;
    if (el.parentElement?.closest("sf-s[data-sf-mode]")?.getAttribute("data-sf-mode") === "server") {
      el.setAttribute(SCHEDULED, "");
      const raw = { ...((rawPropsFor(root, el.id) ?? {}) as { [key: string]: unknown }) };
      const keys = awaited(decodeValue((raw[AWAITS] ?? null) as SfValue));
      delete raw[AWAITS];
      afterSettled(el, keys, () => {
        el.setAttribute(MOUNTED, "");
        mountServer(el, moduleId, raw);
      });
      continue;
    }
    const entry = islands.get(moduleId);
    if (!entry) {
      missing.add(moduleId);
      arm();
      continue;
    }
    const props = propsFor(root, el.id);
    const runs = orphans.get(islandName(el, props));
    if (runs && streaming) continue;
    if (runs) {
      for (const piece of runs) for (const n of piece.nodes) n.remove();
      fresh.add(el);
    }
    el.setAttribute(SCHEDULED, "");
    const placed = el.parentElement?.closest("sf-s[data-sf-when]")?.getAttribute("data-sf-when") as MountTiming | null;
    const keys = awaited(props[AWAITS]);
    delete props[AWAITS];
    afterSettled(el, keys, () => schedule(placed ? { ...entry, when: placed } : entry, moduleId, el, props));
  }
}

/** The props key the server writes on an island it held: the store keys it read that a deferred segment had yet to seed. */
const AWAITS = "$aw";

function awaited(value: unknown): string[] {
  return Array.isArray(value) ? value.map(String) : [];
}

/** Runs `then` once none of `keys` is pending, at once when none is. The server held this island's markup until those seeds land, so mounting it before would show a value about to be replaced. `discard` calls the wait off. */
function afterSettled(el: Element, keys: string[], then: () => void): void {
  if (!keys.some(isPending)) {
    then();
    return;
  }
  let off = false;
  pending.set(el, () => {
    off = true;
  });
  void whenSettled(keys).then(() => {
    if (off) return;
    pending.delete(el);
    then();
  });
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
  const g = globalThis as { __sf?: Record<string, unknown> };
  g.__sf = Object.assign(g.__sf ?? {}, { script: loadMarkedScript });
  const run = () => {
    scan(document);
    loadDeclaredScripts(document);
  };
  adopt();
  adoptLocale();
  adoptCatalog();
  enableEmbeds();
  enablePwa();
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

/** The mark a stylesheet the client owns carries, written by the server on a document and by `addStyles` on a navigation. A link without it belongs to the document and is never taken away. */
const CSS_MARK = "data-sf-css";

/** Brings the owned stylesheets to exactly `hrefs`: `addStyles`, then `pruneStyles` once the new ones are in. */
export function applyStyles(hrefs: string[], timeout = 2000): Promise<void> {
  return addStyles(hrefs, timeout).then(() => pruneStyles(hrefs));
}

/** Adds each of `hrefs` the document does not hold yet, after everything else so their rules still win. Resolves when the new ones have loaded or after `timeout` so a href that never answers cannot hold a navigation open. */
export function addStyles(hrefs: string[], timeout = 2000): Promise<void> {
  const head = document.head;
  const held = new Set(Array.from(head.querySelectorAll<HTMLLinkElement>(`link[${CSS_MARK}]`)).map((link) => link.href));
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

/** Takes away each owned stylesheet `hrefs` does not name. A navigation runs it after the swap, so the page leaving keeps its styles for as long as it shows, a view transition's snapshot of it included. */
export function pruneStyles(hrefs: string[]): void {
  const wanted = new Set(hrefs.map((href) => new URL(href, location.href).href));
  for (const link of Array.from(document.head.querySelectorAll<HTMLLinkElement>(`link[${CSS_MARK}]`))) {
    if (!wanted.has(link.href)) link.remove();
  }
}
