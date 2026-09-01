import { decodeValue, SfValue } from "./values.js";

export type Props = { [key: string]: SfValue };
export type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean) => unknown;

export type MountTiming = "load" | "visible" | "idle";

export interface IslandEntry {
  loader: () => Promise<unknown>;
  mount: Mounter;
  /** When hydration happens: immediately, when scrolled into view, or when the main thread is idle. Defaults to "load". Per island, not per page. */
  when?: MountTiming;
}

const islands = new Map<string, IslandEntry>();

export function registerIsland(moduleId: string, entry: IslandEntry): void {
  islands.set(moduleId, entry);
}

function propsFor(root: ParentNode, id: string): Props {
  const script = root.querySelector(`script[data-sf-props="${id}"]`) ?? document.querySelector(`script[data-sf-props="${id}"]`);
  if (!script || !script.textContent) return {};
  return decodeValue(JSON.parse(script.textContent)) as Props;
}

function mountNow(entry: IslandEntry, moduleId: string, el: Element, props: Props): void {
  const hydrate = el.childNodes.length > 0;
  entry
    .loader()
    .then((mod) => entry.mount(mod, props, el, hydrate))
    .catch((err) => console.warn(`sf: mounting ${moduleId} failed`, err));
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

/** Mounts every unmounted island marker under `root`, honoring each island's timing. Idempotent. */
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
    schedule(entry, moduleId, el, propsFor(root, el.id));
  }
}

/** Scans the document and keeps scanning as streamed slots fill in. */
export function boot(): void {
  const run = () => scan(document);
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", run);
  } else {
    run();
  }
  document.addEventListener("sf:fill", run);
}
