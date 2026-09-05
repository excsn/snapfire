import { SfValue } from "./values.js";
export type Props = {
	[key: string]: SfValue;
};
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
export declare function registerIsland(moduleId: string, entry: IslandEntry): void;
/** Every island registered so far, by module id. */
export declare function registeredIslands(): ReadonlyMap<string, IslandEntry>;
/** Re-renders the island mounted at `el` with `props`, in place, keeping its DOM and its state. False when nothing is mounted there or the island's entry has no patcher. */
export declare function patchIsland(el: Element, props: Props): Promise<boolean>;
/** Mounts every unmounted island marker under `root`, honoring each island's timing: the `data-sf-when` of the region a page or layout placed it in, else the registry's. Idempotent. */
export declare function scan(root: ParentNode): void;
/** Scans the document and keeps scanning as streamed slots fill in. Calling it again scans again without listening twice. */
export declare function boot(): void;
