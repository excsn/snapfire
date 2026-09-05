import { adoptLocale } from "./locale.js";
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
    const entry = islands.get(moduleId);
    if (!entry) {
      console.warn(`sf: no island registered for ${moduleId}`);
      continue;
    }
    el.setAttribute("data-sf-mounted", "");
    const placed = el.parentElement?.closest("sf-s[data-sf-when]")?.getAttribute("data-sf-when") as MountTiming | null;
    schedule(placed ? { ...entry, when: placed } : entry, moduleId, el, propsFor(root, el.id));
  }
}

const filling = new WeakSet<Document>();

/** Scans the document and keeps scanning as streamed slots fill in. Calling it again scans again without listening twice. */
export function boot(): void {
  const run = () => scan(document);
  adopt();
  adoptLocale();
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
