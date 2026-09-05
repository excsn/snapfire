import { type AnchorHTMLAttributes, type ComponentType, type ReactElement, type ReactNode } from "react";
import { MountTiming, Mounter, Patcher } from "./boot.js";
import type { PrefetchTiming } from "./navigator.js";
import { type StoreKey } from "./store.js";
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
export interface SlotProps {
	/** The slot's name: a `slots/<name>` directory beside the layout, or the slot a `page.<name>.tsx` under it renders into. */
	name: string;
	/** What the slot shows while nothing fills it. Rendered by the server, lowered by the build; never rendered here. */
	children?: ReactNode;
}
/** A named slot of a layout: the region a parallel route renders into, or an intercepted route opens in. On the server it is `<sf-s data-sf-name>` around the segment, or around the fallback children while nothing fills it; in the browser this element adopts the region as it stands, and navigation fills and empties it without React reconciling it. */
export declare function Slot({ name }: SlotProps): ReactElement;
/** A store key as state: the value the store holds, or `initial` while nothing does, and a setter that writes the store. Every island reading the key re-renders, whichever root it is in. The server renders from the seed its loaders settled on, so the first paint and the hydration agree; the build lowers this call, so the key must be a literal or a `key()`. */
export declare function useStore<T>(k: StoreKey<T>, initial: T): [T, (next: T) => void];
export interface LinkProps extends AnchorHTMLAttributes<HTMLAnchorElement> {
	/** Always the document's rendering of the target, never an intercept into a slot. */
	full?: boolean;
	/** Renders the target into this slot of the nearest live layout that declares it, whether or not the server would intercept from here. */
	into?: string;
	/** Whether the navigator fetches the target ahead of a click. */
	prefetch?: PrefetchTiming;
	/** Leaves the click to the browser: a full document load. */
	native?: boolean;
}
/** An `<a>` the navigator reads: `full`, `into`, `prefetch` and `native` ride as `data-sf-*` attributes. */
export declare function Link({ full, into, prefetch, native, ...rest }: LinkProps): ReactElement;
export declare const reactMounter: Mounter;
export declare const reactPatcher: Patcher;
