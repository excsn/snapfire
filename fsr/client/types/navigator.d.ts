export type PrefetchTiming = "hover" | "none";
export interface NavigationOptions {
	/** Whether a link's payload is fetched ahead of its click (on hover, focus or touch) or never. Defaults to `"hover"`. */
	prefetch?: PrefetchTiming;
	/** How long a fetched payload answers a navigation before it is fetched again. Defaults to 30 seconds. */
	cacheMs?: number;
}
/** Fetches a same-origin route's payload ahead of a click so the navigation that follows applies it without a round trip. A payload already held or in flight is left alone. */
export declare function prefetch(href: string): Promise<void>;
/** Drops every held payload, which is what a mutation calls for. */
export declare function clearRouterCache(): void;
/** Revalidation after a mutation: drops the router cache, re-fetches the current route's payload and applies it, every kept island taking its new props in place and every kept region that is not an island replaced, so layouts and pages keep their DOM and their state while what they show follows the mutation. */
export declare function refresh(): Promise<void>;
export declare function navigate(href: string, push?: boolean): Promise<void>;
/** Reads the sidecar the server embedded, intercepts same-origin link clicks, prefetches links as they are hovered, focused or touched and owns history from then on. */
export declare function enableNavigation(options?: NavigationOptions): void;
