import { type Props } from "./boot.js";
/** True when `el` is mounted as a server island. */
export declare function isServerIsland(el: Element): boolean;
/**
* Mounts `el` as a server island with `encoded`, the props script as the
* server wrote it, whose `$s` is the state it rendered from. Both are kept
* encoded and handed back untouched: this browser is a courier for a
* component that runs on the server and decoding a double here would hand
* back an integer, since JavaScript has one number type and the tag is the
* only thing that says which this was. Listens for every event its markup
* binds.
*/
export declare function mountServer(el: Element, module: string, encoded: unknown): void;
/** Gives a mounted server island new props, the way navigation gives a browser island new props: the server renders it again from them and the state it holds, then the markup is patched in. `encoded` is the props as the server wrote them, handed back as they are; props the server never wrote, a page's own, are encoded here. */
export declare function patchServer(el: Element, props: Props, encoded?: unknown): Promise<boolean>;
/** Patches `el`'s children to match `html`, touching only what differs: text by content, elements by tag and position or by key, attributes by name. An element's key is its `data-sf-key`; an island's region is keyed by the region key the build wrote, so a region that moved takes its mounted island with it. A focused form control keeps its value. A nested island's marker and children are left as they stand; when the props script after it changed, the island mounted there takes the new props. */
export declare function morph(el: Element, html: string): void;
/** What a morph asks of its caller. `nested` settles an island marker the new markup places again, along with the props script after it, which the walk leaves alone. `adopt` answers a keyed new node none of the siblings carries with a node from elsewhere to move in. Null has the new one imported. `drop` is told of each node the walk is about to remove, before it goes. */
export interface MorphHooks {
	nested: (current: Element, next: Element) => void;
	adopt?: (key: string) => Node | null;
	drop?: (node: Node) => void;
}
/** Patches `old`, a run of `parent`'s children, to match `fresh` by the rules of `morph`. What is new once the run is used up goes in before `end`. A node of the run that something moved to another parent is no longer part of it. */
export declare function morphNodes(parent: Node, old: Node[], fresh: Node[], end: Node | null, hooks: MorphHooks): void;
/** Patches `current` to match `next`: attributes by name, then children by the rules of `morph`. */
export declare function morphElement(current: Element, next: Element, hooks: MorphHooks): void;
