import { type AnchorHTMLAttributes, type ComponentType, type ReactElement, type ReactNode } from "react";
import { MountTiming, Mounter, Patcher, type Props, type Unmounter } from "./boot.js";
import { type PrefetchTiming } from "./navigator.js";
import { type StoreKey } from "./store.js";
export interface IslandProps {
	/** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
	when?: MountTiming;
	/** `server`: the island's events round-trip to the server, which re-renders it; no React root is mounted. */
	mode?: "server";
	/** The module that defines the custom element inside, imported at the island's timing rather than at load. The child is then an element, its markup written by the server, with nothing mounted over it. Not combinable with `mode`. */
	define?: string;
	children?: ReactNode;
}
/** Places its one child component as an island of its own: on the server the child renders inside an `<sf-s data-sf-island>` region as a nested island; in the browser this element adopts that region as it stands and never reconciles it, while the boot runtime mounts the child in its own root at the timing asked for. Lowered by the build, so the child is never rendered here.
*
* The region is claimed once, by the key the build splices in and after that only the island's own root writes inside it. A re-render hands the mounted root the props the parent just computed; a placement the parent has only now added takes its markup from the payload that added it or renders its child inline when no payload describes one. */
export declare function Island({ when, mode, children }: IslandProps): ReactElement;
export interface MountProps {
	/** The module id the registry knows the island under, `src/ui/Chart.vue#default` for one. */
	module: string;
	/** What the island is mounted with, re-applied as a patch when they change. */
	props?: Props;
	when?: MountTiming;
}
/**
* Places an island by module id rather than by component, for a React tree
* holding an island another framework mounts: the registry entry decides the
* mounter, so this writes the marker the boot runtime reads and never renders
* the child itself. `<Island>` is the one to use for a React child, since it
* adopts the region the server rendered; nothing rendered this one, so it is
* mounted fresh and patched from here whenever `props` change.
*/
export declare function Mount({ module, props, when }: MountProps): ReactElement;
/** `component` as a component that places it as an island with `options.when` and `options.mode` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`. */
export declare function island<P extends object>(component: ComponentType<P>, options?: {
	when?: MountTiming;
	mode?: "server";
}): (props: P) => ReactElement;
export interface SlotProps {
	/** The slot's name: a `slots/<name>` directory beside the layout or the slot a `page.<name>.tsx` under it renders into. */
	name: string;
	/** What the slot shows while nothing fills it. Rendered by the server, lowered by the build; never rendered here. */
	children?: ReactNode;
}
/** A named slot of a layout: the region a parallel route renders into or an intercepted route opens in. On the server it is `<sf-s data-sf-name>` around the segment or around the fallback children while nothing fills it; in the browser this element adopts the region as it stands and navigation fills and empties it without React reconciling it. */
export declare function Slot({ name }: SlotProps): ReactElement;
/** A store key as state: the value the store holds (or `initial` while nothing does) and a setter that writes the store. Every island reading the key re-renders, whichever root it is in. The server renders from the seed its loaders settled on, so the first paint and the hydration agree; the build lowers this call, so the key must be a literal or a `key()`. */
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
	/** Whether a segment whose key changed but whose module did not is morphed in place, keeping the islands its new markup places again, rather than replaced. Left out, the navigator keeps them when only the query changes. */
	keep?: boolean;
	/** When the link is marked `aria-current`: `"exact"`, the default, on the page its `href` names; `"prefix"` on that page and anything under it; `"none"` never. An `href` carrying a query or a fragment never matches. */
	match?: "exact" | "prefix" | "none";
	/** Which path the mark is judged against: `"url"`, the default, the address bar; `"document"`, the page beneath an open intercept, so a nav describing the section a drawer or a modal opened over stays where it was. The two differ only while an intercept is open. */
	current?: "url" | "document";
}
/** An `<a>` the navigator reads: `full`, `into`, `prefetch`, `native` and `keep` ride as `data-sf-*` attributes, `match` as the `data-sf-link` the navigator re-reads after each navigation and `current` as `data-sf-current` when it is the document's. */
export declare function Link({ full, into, prefetch, native, keep, match, current, ...rest }: LinkProps): ReactElement;
/** The values the server computed for an island's hoisted expressions, keyed `module|id@i.j`; see `useHoisted`. */
export type Hoisted = {
	readonly [key: string]: unknown;
};
/** The reader the build binds at the top of a component it rewrote: `r` in place of a render-path call whose inputs are props only, so hydration reads what the server rendered instead of computing it again; `l` around each JSX `.map` callback, so a read inside it knows its iteration. */
export interface HoistReader {
	/** The server's value for hoist `id` at the current loop indices or `compute()` when it recorded none. */
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
	/** The region key for the island placement `id` at the current loop indices, the same string the server wrote on the region. Placements are numbered apart from the hoists and marked `i`. */
	k(id: number): string;
	/** `element`, a keyed placement `id` of a component, under a provider whose path is the current one plus `c<id>`, so what that component keys sits below this placement and two placements of it key apart. */
	p(id: number, element: ReactElement): ReactElement;
}
/** The reader for the island being rendered, bound to `module`, whose keys are `module|id` or `module|id@i.j` under loops and keyed placements, the callers' first. */
export declare function useHoisted(module: string): HoistReader;
/** `element` under the hoisted table `table`, the way the mounter places an island under the table its props carried. */
export declare function withHoisted(table: Hoisted | null, element: ReactElement): ReactElement;
/** `component` as the layout the React adapter mounts as one tree with its page: `export default tree(Layout)`. The build registers the layout with the tree mounter, so the page renders inside the layout's root and React context set in the layout reaches it. In the browser the layout is the component itself. */
export declare function tree<P extends object>(component: ComponentType<P>): ComponentType<P>;
/** Whether a tree root renders `marker`, the island in its child region, itself: a page whose module the registry mounts with React. A layout under it, which holds a slot region, is a root of its own, as is anything another framework mounts or nothing registered. */
export declare function reactTreeClaims(marker: Element): boolean;
export declare const reactMounter: Mounter;
export declare const reactPatcher: Patcher;
/** Mounts a layout declared `tree(Layout)` as one root with its page: the page's module is loaded first, its marker and props read out of the child region and the whole hydrated at once. A child region holding anything else is adopted, as `reactMounter` adopts it. */
export declare const reactTreeMounter: Mounter;
/** Re-renders a tree root: with its own new props, with the page's new props or with the child the navigator handed it. */
export declare const reactTreePatcher: Patcher;
export declare const reactUnmounter: Unmounter;
