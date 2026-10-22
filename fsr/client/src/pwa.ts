import { key, set, type StoreKey } from "./store.js";

/** Whether the browser reports a connection, kept current from its `online` and `offline` events. */
export const online: StoreKey<boolean> = key<boolean>("sf/online");

/** True once a newer build's worker has taken over this page; the next full page load is that build. */
export const updateReady: StoreKey<boolean> = key<boolean>("sf/update-ready");

const watched = new WeakSet<Document>();

/** Keeps `online` current and registers the worker `<meta name="sf:worker">` names, which the host writes outside development under `[pwa]`. When a newer worker takes over a page an older one served, sets `updateReady` and raises `sf:update-ready` on the document. `boot` calls it. */
export function enablePwa(): void {
  if (typeof document === "undefined" || watched.has(document)) return;
  watched.add(document);
  const nav = globalThis.navigator as Navigator | undefined;
  set(online, nav?.onLine ?? true);
  globalThis.addEventListener?.("online", () => set(online, true));
  globalThis.addEventListener?.("offline", () => set(online, false));
  const src = document.querySelector('meta[name="sf:worker"]')?.getAttribute("content");
  if (!src || nav === undefined || !("serviceWorker" in nav)) return;
  const served = nav.serviceWorker.controller !== null;
  nav.serviceWorker.addEventListener("controllerchange", () => {
    if (!served) return;
    set(updateReady, true);
    document.dispatchEvent(new CustomEvent("sf:update-ready"));
  });
  nav.serviceWorker.register(src, { scope: "/" }).catch((error: unknown) => console.warn("fsr: the service worker did not register", error));
}
