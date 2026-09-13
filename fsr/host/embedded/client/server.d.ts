import type { Props } from "./boot.js";
/** True when `el` is mounted as a server island. */
export declare function isServerIsland(el: Element): boolean;
/**
* Mounts `el` as a server island with `encoded`, the props script as the
* server wrote it, whose `$s` is the state it rendered from. Both are kept
* encoded and handed back untouched: this browser is a courier for a
* component that runs on the server, and decoding a double here would hand
* back an integer, since JavaScript has one number type and the tag is the
* only thing that says which this was. Listens for every event its markup
* binds.
*/
export declare function mountServer(el: Element, module: string, encoded: unknown): void;
/** Gives a mounted server island new props, the way navigation gives a browser island new props: the server renders it again from them and the state it holds, and the markup is patched in. */
export declare function patchServer(el: Element, props: Props): Promise<boolean>;
/** Patches `el`'s children to match `html`, touching only what differs: text by content, elements by tag and position or by `data-sf-key`, attributes by name. A focused form control keeps its value. A nested island is left as it stands. */
export declare function morph(el: Element, html: string): void;
