import { SfValue } from "./values.js";
export type Props = {
	[key: string]: SfValue;
};
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
export declare function registerIsland(moduleId: string, entry: IslandEntry): void;
/** Registers a module the browser imports a function prop from, by the file's path as the build names it, so a module reference `file#export` among an island's props resolves to that export before the island mounts. */
export declare function registerModule(file: string, loader: () => Promise<unknown>): void;
/** Every island registered so far, by module id. */
export declare function registeredIslands(): ReadonlyMap<string, IslandEntry>;
/** An island marker's props script beside it and the props it holds, decoded and as written. */
export declare function markerProps(marker: Element): {
	script: Element | null;
	props: Props;
	encoded: unknown;
};
/** The mounter for an island whose module defines a custom element: importing the module is the whole mount, since the element the server already wrote upgrades itself once its definition runs. What the island's timing schedules, then, is the import. */
export declare const defineMounter: Mounter;
/** Whether the server rendered this island's own markup, which is what decides hydrating over mounting. Slot regions do not count: a module the server never evaluated still carries one per plan child it must offer, so an element holding nothing else was rendered by nobody. */
export declare function serverRendered(el: Element): boolean;
/** Ends every island under `root`, `root` itself included when it is a marker, before the caller takes those nodes out of the document: a mount still waiting on its timing is called off, one whose loader is in flight mounts nothing when it lands and a mounted one is handed to its entry's `unmount`, nested islands before the island around them. What was mounted there is forgotten, so `islandState` answers null and `patchIsland` false. Call it on every node removed by anything other than a mounted root's own render, since a root left in a detached element keeps running: its effects never clean up and whatever they hold stays held. */
export declare function discard(root: ParentNode): void;
/** Readies every island under `root` for a stay outside the document that may end with the nodes put back. A mounted island whose entry pauses is paused and kept. Any other island whose entry has an `unmount` is ended as `discard` ends it and its marker unmarked, so a later morph replaces it and a scan mounts it afresh; a server island and a mount still waiting on its timing the same. An island with no `unmount`, a custom element's, stays as it is: the browser disconnects the element and connects it again when the nodes come back. */
export declare function release(root: ParentNode): void;
/** Shows again every island under `root` that `release` paused, now that its nodes are back in the document. Settles once each adapter that says when has shown its island. */
export declare function resume(root: ParentNode): Promise<void>;
/** The props an island last took, the regions the last payload described inside it and the markup it gave the island's children, for an adapter placing its nested islands and its children. Null when nothing is mounted at `el`. */
export declare function islandState(el: Element): {
	props: Props;
	regions: unknown;
	children: string | null;
	markup: {
		[n: string]: string;
	} | null;
} | null;
/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. `regions` is what the payload behind this patch says about the islands inside it and `children` the markup it gives the island's children region, both read back by the adapter through `islandState`. `encoded` is `props` as the server encoded them, which a server island hands back in place of encoding `props` again. False when nothing is mounted there or the island's entry has no patcher. */
export declare function patchIsland(el: Element, props: Props, regions?: unknown, children?: string | null, encoded?: unknown, markup?: {
	[n: string]: string;
} | null): Promise<boolean>;
/** Mounts every unmounted island marker under `root`, honoring each island's timing: the `data-sf-when` of the region a page or layout placed it in, else the registry's. Idempotent. */
export declare function scan(root: ParentNode): void;
/** Imports an entry module once and rescans, so the islands it registers mount. Call it before the scan that will miss them, so a miss is not reported while its registration is in flight. */
export declare function loadEntry(src: string): void;
/** Scans the document and keeps scanning as streamed slots fill in. Calling it again scans again without listening twice. */
export declare function boot(): void;
/** Brings the owned stylesheets to exactly `hrefs`: `addStyles`, then `pruneStyles` once the new ones are in. */
export declare function applyStyles(hrefs: string[], timeout?: number): Promise<void>;
/** Adds each of `hrefs` the document does not hold yet, after everything else so their rules still win. Resolves when the new ones have loaded or after `timeout` so a href that never answers cannot hold a navigation open. */
export declare function addStyles(hrefs: string[], timeout?: number): Promise<void>;
/** Takes away each owned stylesheet `hrefs` does not name. A navigation runs it after the swap, so the page leaving keeps its styles for as long as it shows, a view transition's snapshot of it included. */
export declare function pruneStyles(hrefs: string[]): void;
