import { type ComponentType, type ReactElement, type ReactNode } from "react";
import { MountTiming, Mounter, Patcher } from "./boot.js";
export interface IslandProps {
	/** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
	when?: MountTiming;
	children?: ReactNode;
}
/** Places its one child component as an island of its own: on the server the child renders inside an `<sf-s data-sf-island>` region as a nested island; in the browser this element adopts that region as it stands and never reconciles it, and the boot runtime mounts the child in its own root at the timing asked for. Lowered by the build, so the child is never rendered here. */
export declare function Island({ when }: IslandProps): ReactElement;
/** `component` as a component that places it as an island with `options.when` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`. */
export declare function island<P extends object>(component: ComponentType<P>, options?: {
	when?: MountTiming;
}): (props: P) => ReactElement;
export declare const reactMounter: Mounter;
export declare const reactPatcher: Patcher;
