import { Head } from "./reader.js";
/** Sets the document's title and description meta from a payload's `H` row; a field the row left out is left alone. */
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
}
export interface NavigationOptions {
	/** When a link's payload is fetched ahead of its click: on hover, focus or touch; as the link enters the viewport; or never. A link's own `data-sf-prefetch` overrides it. Defaults to `"hover"`. */
	prefetch?: PrefetchTiming;
	/** How long a fetched payload answers a navigation before it is fetched again. Defaults to 30 seconds. */
	cacheMs?: number;
}
/** Fetches a same-origin route's payload ahead of a click so the navigation that follows applies it without a round trip. A payload already held or in flight is left alone. Resolves once the payload has arrived whole. */
export declare function prefetch(href: string, options?: NavigateOptions): Promise<void>;
/** Drops every held payload, which is what a mutation calls for. */
export declare function clearRouterCache(): void;
/** Revalidation after a mutation: drops the router cache, re-fetches the current route's payload and applies it, every kept island taking its new props in place and every kept region that is not an island replaced, so layouts and pages keep their DOM and their state while what they show follows the mutation. */
export declare function refresh(): Promise<void>;
/** Navigates to `href` by payload, from the document's current path unless `options` say otherwise. The eager wave is applied and history moves as soon as the sidecar arrives, deferred segments showing their fallbacks; each resolution fills its slot as it lands and the promise resolves once the payload has been applied whole. A navigation that changes only the query keeps the islands the page places again, unless `options.keep` says otherwise. An intercepted navigation opens in its slot without scrolling; anything else scrolls to the element its fragment names or to the top. A fragment of the page already showing scrolls without fetching, as does a step back or forward within that page. */
export declare function navigate(href: string, push?: boolean, options?: NavigateOptions): Promise<void>;
/** The page the document is showing, which is not always what the address bar says: an intercepted navigation puts the target's URL there while the page underneath stays. Empty before `enableNavigation` runs. */
export declare function currentDocumentPath(): string;
/** The page the document is showing, under another locale: its path with the current locale's prefix replaced by `to`. Nothing else is rewritten and a path given explicitly is used as it stands. This is what a language switcher links to, so choosing a language keeps the reader where they are instead of sending them wherever the switcher happens to live. */
export declare function localePath(to: string, from?: string): string;
/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links when they are hovered, focused or touched; or as they enter the viewport where one asks for that. It owns history from then on. */
export declare function enableNavigation(options?: NavigationOptions): void;
