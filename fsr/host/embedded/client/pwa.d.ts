import { type StoreKey } from "./store.js";
/** Whether the browser reports a connection, kept current from its `online` and `offline` events. */
export declare const online: StoreKey<boolean>;
/** True once a newer build's worker has taken over this page; the next full page load is that build. */
export declare const updateReady: StoreKey<boolean>;
/** Keeps `online` current and registers the worker `<meta name="sf:worker">` names, which the host writes outside development under `[pwa]`. When a newer worker takes over a page an older one served, sets `updateReady` and raises `sf:update-ready` on the document. `boot` calls it. */
export declare function enablePwa(): void;
