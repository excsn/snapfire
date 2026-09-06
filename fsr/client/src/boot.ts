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
  mounted.set(el, { entry, moduleId, handle });
}

/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. False when nothing is mounted there or the island's entry has no patcher. */
export async function patchIsland(el: Element, props: Props): Promise<boolean> {
  if (isServerIsland(el)) return patchServer(el, props);
  const island = mounted.get(el);
  if (!island?.entry.patch) return false;
  const handle = await island.handle;
  if (handle === undefined) return false;
  const mod = await island.entry.loader();
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
