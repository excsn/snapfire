import { type AnchorHTMLAttributes, type ComponentType, type ReactElement, type ReactNode } from "react";
import { MountTiming, Mounter, Patcher } from "./boot.js";
import type { PrefetchTiming } from "./navigator.js";
import { type StoreKey } from "./store.js";
export interface IslandProps {
	/** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
	when?: MountTiming;
	/** `server`: the island's events round-trip to the server, which re-renders it; no React root is mounted. */
	mode?: "server";
	children?: ReactNode;
}
/** Places its one child component as an island of its own: on the server the child renders inside an `<sf-s data-sf-island>` region as a nested island; in the browser this element adopts that region as it stands and never reconciles it, and the boot runtime mounts the child in its own root at the timing asked for. Lowered by the build, so the child is never rendered here. */
export declare function Island({ when, mode }: IslandProps): ReactElement;
/** `component` as a component that places it as an island with `options.when` and `options.mode` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`. */
export declare function island<P extends object>(component: ComponentType<P>, options?: {
	when?: MountTiming;
	mode?: "server";
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
/** The document's locale as the application spells it, `fr_FR` or `fr`. The server renders it from the request, so the first paint and the hydration agree; a navigation that changes it re-renders every island reading it. The build lowers this call. */
export declare function useLocale(): string;
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
/** The values the server computed for an island's hoisted expressions, keyed `module|id@i.j`; see `useHoisted`. */
export type Hoisted = {
	readonly [key: string]: unknown;
};
/** The reader the build binds at the top of a component it rewrote: `r` in place of a render-path call whose inputs are props only, so hydration reads what the server rendered instead of computing it again; `l` around each JSX `.map` callback, so a read inside it knows its iteration. */
export interface HoistReader {
	/** The server's value for hoist `id` at the current loop indices, or `compute()` when it recorded none. */
	r<T>(id: number, compute: () => T): T;
	/** `f` with its index argument pushed onto the loop path while it runs. */
	l<
		A extends unknown[],
		R
	>(f: (...args: A) => R): (...args: A) => R;
	/** The element for a static subtree: `hit` with the server's inner markup for chunk `id` when the table holds it, else `miss`, the original JSX. */
	c(id: number, hit: (html: {
		__html: string;
	}) => ReactElement, miss: () => ReactElement): ReactElement;
}
/** The reader for the island being rendered, bound to `module`, whose keys are `module|id` or `module|id@i.j` under loops, the callers' loops first. */
export declare function useHoisted(module: string): HoistReader;
/** `element` under the hoisted table `table`, the way the mounter places an island under the table its props carried. */
export declare function withHoisted(table: Hoisted | null, element: ReactElement): ReactElement;
export declare const reactMounter: Mounter;
export declare const reactPatcher: Patcher;
