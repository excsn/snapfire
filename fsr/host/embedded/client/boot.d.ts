import { SfValue } from "./values.js";
export type Props = {
	[key: string]: SfValue;
};
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
export declare function registerIsland(moduleId: string, entry: IslandEntry): void;
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
/** The props an island last took, the regions the last payload described inside it and the markup it gave the island's children, for an adapter placing its nested islands and its children. Null when nothing is mounted at `el`. */
export declare function islandState(el: Element): {
	props: Props;
	regions: unknown;
	children: string | null;
	child: TreeChild | null;
} | null;
/** The tree root whose child region holds `node`: the island around the bare `<sf-s>` that is `node`'s parent, when its entry claims markers. Null anywhere else. */
export declare function treeRootOf(node: Node | null): Element | null;
/** Registers `marker`, the page a tree root renders in its child region, as mounted by that root: `islandState` answers for it and `patchIsland` on it re-renders the root with the page's new props. The root's adapter calls it once the marker is in the document. */
export declare function adoptTreeChild(marker: Element, root: Element): void;
/** Resolves once every child handed to a tree root has been rendered or refused. */
export declare function treeSettled(): Promise<void>;
/** Hands a tree root what its child region shows from now on: the page module is loaded through the registry, what the region held is ended, then the root re-renders with the child. False when nothing is mounted at `root` or its mount failed, which leaves the caller to write the markup itself. */
export declare function setTreeChild(root: Element, child: TreeChild): Promise<boolean>;
/** Records what a tree root's child region holds at hydration, before the root's adapter renders it. */
export declare function holdTreeChild(root: Element, child: TreeChild): void;
/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. `regions` is what the payload behind this patch says about the islands inside it and `children` the markup it gives the island's children region, both read back by the adapter through `islandState`. `encoded` is `props` as the server encoded them, which a server island hands back in place of encoding `props` again. False when nothing is mounted there or the island's entry has no patcher. */
export declare function patchIsland(el: Element, props: Props, regions?: unknown, children?: string | null, encoded?: unknown): Promise<boolean>;
/** Mounts every unmounted island marker under `root`, honoring each island's timing: the `data-sf-when` of the region a page or layout placed it in, else the registry's. Idempotent. */
export declare function scan(root: ParentNode): void;
/** Imports an entry module once and rescans, so the islands it registers mount. Call it before the scan that will miss them, so a miss is not reported while its registration is in flight. */
export declare function loadEntry(src: string): void;
/** Scans the document and keeps scanning as streamed slots fill in. Calling it again scans again without listening twice. */
export declare function boot(): void;
/** Brings the owned stylesheets to exactly `hrefs`: links already there stay, ones no longer named go and new ones are added after everything else so their rules still win. Resolves when the new ones have loaded or after `timeout` so a href that never answers cannot hold a navigation open. */
export declare function applyStyles(hrefs: string[], timeout?: number): Promise<void>;
