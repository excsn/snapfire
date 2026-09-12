/** The one method this adapter calls on htmx: the library's own re-scan of a subtree. */
export interface HtmxProcessor {
	process(element: Element): void;
}
/**
* Makes htmx and the client aware of each other's markup, both directions.
* After htmx settles a swap the client reads any store seed the fragment
* carried and mounts any island it placed; after the navigator applies a
* payload htmx processes what it wrote, without which a form or an anchor
* reached by a soft navigation is markup htmx never saw and the browser
* follows it natively.
*
* Returns the function that takes the three listeners off again.
*/
export declare function bindHtmx(htmx: HtmxProcessor): () => void;
