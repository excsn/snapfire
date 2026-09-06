import type { Props } from "./boot.js";
/** True when `el` is mounted as a server island. */
export declare function isServerIsland(el: Element): boolean;
/** Mounts `el` as a server island with `props`, whose `$s` is the state the server rendered from. Listens for every event its markup binds. */
export declare function mountServer(el: Element, module: string, props: Props): void;
/** Gives a mounted server island new props, the way navigation gives a browser island new props: the server renders it again from them and the state it holds, and the markup is patched in. */
export declare function patchServer(el: Element, props: Props): Promise<boolean>;
/** Patches `el`'s children to match `html`, touching only what differs: text by content, elements by tag and position or by `data-sf-key`, attributes by name. A focused form control keeps its value. A nested island is left as it stands. */
export declare function morph(el: Element, html: string): void;
