import { SfValue } from "./values.js";
export type Props = {
	[key: string]: SfValue;
};
export type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean) => unknown;
export type MountTiming = "load" | "visible" | "idle";
export interface IslandEntry {
	loader: () => Promise<unknown>;
	mount: Mounter;
	/** When hydration happens: immediately, when scrolled into view, or when the main thread is idle. Defaults to "load". Per island, not per page. */
	when?: MountTiming;
}
export declare function registerIsland(moduleId: string, entry: IslandEntry): void;
/** Every island registered so far, by module id. */
export declare function registeredIslands(): ReadonlyMap<string, IslandEntry>;
/** Mounts every unmounted island marker under `root`, honoring each island's timing. Idempotent. */
export declare function scan(root: ParentNode): void;
/** Scans the document and keeps scanning as streamed slots fill in. */
export declare function boot(): void;
