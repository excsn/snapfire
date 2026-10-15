import { Head } from "./reader.js";
/** Sets the document's title and description meta, its theme and its scheme from a payload's `H` row and loads the scripts it lists, each once per document; a field the row left out is left alone. */
export declare function applyHead(head: Head): void;
export type PrefetchTiming = "hover" | "viewport" | "none";
export interface NavigateOptions {
	/** The document's rendering of the target, never an intercept. */
	full?: boolean;
	/** Renders the target into this slot of the nearest live layout that declares it. */
	into?: string;
	/** Replaces the current history entry rather than adding one. */
	replace?: boolean;
	/** Whether a segment whose key changed but whose module did not is morphed in place, keeping every island its new markup places again with its DOM and its state, rather than replaced. Defaults to true when the target has the current pathname, which means only the query changed. Otherwise it defaults to false. */
	keep?: boolean;
	/** Whether the window scrolls to the element the target's fragment names or to the top when it names none. Defaults to true; false leaves the window where it is. */
	scroll?: boolean;
	/** Whether the swap runs inside a view transition, over the document's `transitions` setting. */
	transition?: boolean;
	/** Whether the navigation holds the regions it replaces and puts back one held for where it goes, over the document's `activity` setting. */
	activity?: boolean;
}
export interface NavigationOptions {
	/** When a link's payload is fetched ahead of its click: on hover, focus or touch; as the link enters the viewport; or never. A link's own `data-sf-prefetch` overrides it. Defaults to `"hover"`. */
	prefetch?: PrefetchTiming;
	/** How long a fetched payload answers a navigation before it is fetched again. Defaults to 30 seconds. */
	cacheMs?: number;
	/** Whether a navigation swaps the page inside a view transition, `document.startViewTransition`, so the browser animates from the old page to the new one. A link's own `data-sf-transition` overrides it. Skipped where the browser has no view transitions, when the visitor prefers reduced motion and when the tab is hidden. Defaults to false. */
	transitions?: boolean;
	/** Whether a navigation keeps the page it leaves, out of the document, so a later navigation back to it, by a link or through history, puts it back as the visitor left it: its DOM, a custom element's state, a video's position and, through history, the window's scroll. An island whose framework would keep running while detached, React's and Vue's, is unmounted instead and mounts afresh when the page comes back. A link's own `data-sf-activity` overrides it. A mutation, a refresh and a move of the session drop what is held. Defaults to false. */
	activity?: boolean;
	/** How many pages `activity` holds, the least recently left dropped first. Defaults to 10. */
	activityLimit?: number;
}
/** Fetches a same-origin route's payload ahead of a click so the navigation that follows applies it without a round trip. A payload already held or in flight is left alone, and so is the page being shown, whose link the pointer rests on after every view transition. Resolves once the payload has arrived whole. */
export declare function prefetch(href: string, options?: NavigateOptions): Promise<void>;
/** Drops every held payload, which is what a mutation calls for. */
export declare function clearRouterCache(): void;
/** Revalidation after a mutation: drops the router cache, re-fetches the current route's payload and applies it, every kept island taking its new props in place and every kept region that is not an island replaced, so layouts and pages keep their DOM and their state while what they show follows the mutation. */
export declare function refresh(): Promise<void>;
/** Navigates to `href` by payload, from the document's current path unless `options` say otherwise. The eager wave is applied and history moves as soon as the sidecar arrives, deferred segments showing their fallbacks; each resolution fills its slot as it lands and the promise resolves once the payload has been applied whole. A navigation that changes only the query keeps the islands the page places again, unless `options.keep` says otherwise. An intercepted navigation opens in its slot without scrolling, as does one whose `options.scroll` is false; anything else scrolls to the element its fragment names or to the top. A fragment of the page already showing scrolls without fetching, as does a step back or forward within that page. */
export declare function navigate(href: string, push?: boolean, options?: NavigateOptions): Promise<void>;
/** The page the document is showing, which is not always what the address bar says: an intercepted navigation puts the target's URL there while the page underneath stays. Empty before `enableNavigation` runs. */
export declare function currentDocumentPath(): string;
/** The path and search in the address bar as the navigator last set them: the target of the last navigation, intercepted or not. Empty before `enableNavigation` runs. */
export declare function currentAddressPath(): string;
/** Brings every `<a data-sf-link>` under `root` to the page it is judged against: the address for most, the page beneath an open intercept for one that says `data-sf-current="document"`. The server writes the mark at first paint; this keeps it right across a navigation, which re-renders the page segment and leaves the layout holding the nav alone. */
export declare function markLinks(root?: ParentNode): void;
/** The page the document is showing, under another locale: its path with the current locale's prefix replaced by `to`. Nothing else is rewritten and a path given explicitly is used as it stands. This is what a language switcher links to, so choosing a language keeps the reader where they are instead of sending them wherever the switcher happens to live. */
export declare function localePath(to: string, from?: string): string;
/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links when they are hovered, focused or touched; or as they enter the viewport where one asks for that. It owns history from then on. A second call on the same document, which a mounted site's entry module makes when a payload imports it, keeps the spine, the paths and the listeners the first one installed and changes only the options it names. */
export declare function enableNavigation(options?: NavigationOptions): void;
