/** The one method this adapter calls on htmx: the library's own re-scan of a subtree. */
export interface HtmxProcessor {
	process(element: Element): void;
}
export declare function bindHtmx(htmx: HtmxProcessor): () => void;
