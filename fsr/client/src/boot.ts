import { adoptCatalog, adoptLocale } from "./locale.js";
import { isServerIsland, mountServer, patchServer } from "./server.js";
import { adopt } from "./store.js";
import { decodeValue, SfValue } from "./values.js";

export type Props = { [key: string]: SfValue };
export type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean) => unknown;
/** Re-renders a mounted island in place with new props; `handle` is what the mounter returned. */
export type Patcher = (handle: unknown, module: unknown, props: Props, el: Element) => void;

export type MountTiming = "load" | "visible" | "idle";

export interface IslandEntry {
  loader: () => Promise<unknown>;
  mount: Mounter;
  /** When hydration happens: immediately, when scrolled into view, or when the main thread is idle. Defaults to "load". Per island, not per page. */
  when?: MountTiming;
  patch?: Patcher;
}

interface Mounted {
  entry: IslandEntry;
  moduleId: string;
  handle: Promise<unknown>;
  /** The props the island last mounted or patched with. */
  props: Props;
  /** What the payload behind the last patch said about the regions inside this island, for the adapter to hand its nested islands. */
  regions: unknown;
}

const mounted = new WeakMap<Element, Mounted>();

const islands = new Map<string, IslandEntry>();

export function registerIsland(moduleId: string, entry: IslandEntry): void {
  islands.set(moduleId, entry);
}

/** Every island registered so far, by module id. */
export function registeredIslands(): ReadonlyMap<string, IslandEntry> {
  return islands;
}

function propsFor(root: ParentNode, id: string): Props {
  const script = root.querySelector(`script[data-sf-props="${id}"]`) ?? document.querySelector(`script[data-sf-props="${id}"]`);
  if (!script || !script.textContent) return {};
  return decodeValue(JSON.parse(script.textContent)) as Props;
}

function mountNow(entry: IslandEntry, moduleId: string, el: Element, props: Props): void {
  const hydrate = el.childNodes.length > 0;
  const handle = entry
    .loader()
    .then((mod) => entry.mount(mod, props, el, hydrate))
    .catch((err) => {
      console.warn(`sf: mounting ${moduleId} failed`, err);
      return undefined;
    });
  mounted.set(el, { entry, moduleId, handle, props, regions: null });
}

/** The props an island last took and the regions the last payload described inside it, for an adapter placing its nested islands. Null when nothing is mounted at `el`. */
export function islandState(el: Element): { props: Props; regions: unknown } | null {
  const island = mounted.get(el);
  return island ? { props: island.props, regions: island.regions } : null;
}

/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. `regions` is what the payload behind this patch says about the islands inside it, which the adapter reads back through `islandState`. False when nothing is mounted there or the island's entry has no patcher. */
export async function patchIsland(el: Element, props: Props, regions: unknown = null): Promise<boolean> {
  if (isServerIsland(el)) return patchServer(el, props);
  const island = mounted.get(el);
  if (!island?.entry.patch) return false;
  const handle = await island.handle;
  if (handle === undefined) return false;
  const mod = await island.entry.loader();
  island.props = props;
  island.regions = regions;
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
          mountNow(entry, moduleId, el, props);
        }
      });
      observer.observe(el);
      return;
    }
    case "idle": {
      const idle = (window as { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback;
      if (idle) {
        idle(() => mountNow(entry, moduleId, el, props));
      } else {
        setTimeout(() => mountNow(entry, moduleId, el, props), 1);
      }
      return;
    }
  }
}

/** Mounts every unmounted island marker under `root`, honoring each island's timing: the `data-sf-when` of the region a page or layout placed it in, else the registry's. Idempotent. */
export function scan(root: ParentNode): void {
  for (const el of Array.from(root.querySelectorAll("sf-i:not([data-sf-mounted])"))) {
    const moduleId = el.getAttribute("data-sf-module");
    if (!moduleId) continue;
    if (el.parentElement?.closest("sf-s[data-sf-mode]")?.getAttribute("data-sf-mode") === "server") {
      el.setAttribute("data-sf-mounted", "");
      mountServer(el, moduleId, propsFor(root, el.id));
      continue;
    }
    const entry = islands.get(moduleId);
    if (!entry) {
      missing.add(moduleId);
      arm();
      continue;
    }
    el.setAttribute("data-sf-mounted", "");
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

/** Brings the owned stylesheets to exactly `hrefs`: links already there stay, ones no longer named go, and new ones are added after everything else so their rules still win. Resolves when the new ones have loaded, or after `timeout` so a href that never answers cannot hold a navigation open. */
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
