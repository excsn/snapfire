import { type Component, type Snippet } from "svelte";
import { type IslandEntry, type MountTiming, type Mounter, type Patcher, type Props, type Unmounter } from "./boot.js";
import { type StoreKey } from "./store.js";
import { type LinkOptions } from "./link.js";
import { type PictureOptions } from "./picture.js";
import { embedParts, type EmbedOptions } from "./embed.js";
/** One node of an island's children as `Pieces.svelte` renders it in the island's tree. */
export type TreePiece = TreeIsland | TreeElement | TreeOpaque | TreeHtml;
/** A Svelte island rendered in the tree: hydrated where the server rendered it, else rendered once its timing fires. */
export interface TreeIsland {
	kind: "island";
	id: string;
	moduleId: string;
	region: string | null;
	when: string | null;
	mode: string | null;
	ssr: boolean;
	props: Props;
	children: TreePiece[] | null;
	entry: IslandEntry | undefined;
	timing: MountTiming;
}
/** An element holding a Svelte island of the tree, rebuilt around it. */
export interface TreeElement {
	kind: "element";
	tag: string;
	attrs: {
		[name: string]: string;
	};
	children: TreePiece[];
}
/** An element whose content Svelte never renders: kept as the server wrote it and morphed when a patch changes it. */
export interface TreeOpaque {
	kind: "opaque";
	tag: string;
	attrs: {
		[name: string]: string;
	};
	html: string;
	/** The element the server wrote, whose nodes a mount takes. */
	from: Element;
}
/** A run of text and comments. */
export interface TreeHtml {
	kind: "html";
	html: string;
}
/** Takes the snippet that renders an island's children in its tree, the one `Pieces.svelte` exports. Called by the island registry the build writes. */
export declare function registerTree(tree: Snippet): void;
/** What `Pieces.svelte` lists: the pieces it was given, else the children of the island around it. */
export declare function held(given: () => TreePiece[] | undefined): () => TreePiece[];
/** The attachment `Pieces.svelte` puts on an element it keeps as the server wrote it: the content stays while hydrating, is taken from the server's element on a fresh render and morphed when a patch changes it. The islands inside it are scanned and ended once the element leaves the document. */
export declare function opaque(): (piece: TreeOpaque) => (el: Element) => () => void;
/** What `Island.svelte` renders a Svelte island of the tree with: its props and its component, loaded already when the server rendered it, else once its timing fires. Sets the contexts the island's own children and `useLinkStatus` read. */
export declare function nested(piece: () => TreeIsland): {
	readonly component: Component<{
		[key: string]: unknown;
	}> | null;
	readonly props: {
		[key: string]: unknown;
	};
	attach: (el: Element) => () => void;
};
export declare const svelteMounter: Mounter;
export declare const sveltePatcher: Patcher;
export declare const svelteUnmounter: Unmounter;
/** A Svelte store over the neutral store: `const count = useStore(countKey, 0)`, then `$count` reads it and `$count = 3` writes it, in runes and legacy mode alike, following every other island that shares the key. */
export declare function useStore<T>(key: StoreKey<T>, initial: T): {
	subscribe: (run: (value: T) => void) => () => void;
	set: (value: T) => void;
	update: (fn: (value: T) => T) => void;
};
/** The document's locale as a Svelte store, following every navigation that changes it: `$locale`. */
export declare function useLocale(): {
	subscribe: (run: (value: string) => void) => () => void;
};
/** What FSR's `Link` component calls as it initialises, so `useLinkStatus` in a component inside it reads the anchor it rendered. */
export declare function holdLink(anchor: () => Element | null): void;
/** Whether the navigation a click on the enclosing link started is still in flight, as a Svelte store of `{ pending }`: FSR's `Link` this component renders inside, else the link the server placed this island in. It turns false once the page the link names is shown. Call it while the component initialises. */
export declare function useLinkStatus(): {
	subscribe: (run: (value: {
		pending: boolean;
	}) => void) => () => void;
};
/** The attributes of an anchor the navigator reads, to spread on an `<a>`: `<a {...link({ href: "/next" })}>`. */
export declare function link(options: LinkOptions): Record<string, unknown>;
/** The `<img>` and, when the image policy derives variants, the `<source>`s the server writes for the same options: an `<img {...parts.img}>` inside a `<picture>` holding one `<source>` per entry of `parts.sources`, or the `<img>` alone when `sources` is null. */
export declare function picture(options: PictureOptions): {
	img: Record<string, unknown>;
	sources: Record<string, unknown>[] | null;
};
/** The attributes of the placeholder the server writes for a video hosted elsewhere: a `<div {...parts.outer}>` holding a `<button {...parts.button}>` with the still, the title and the play mark. */
export declare function embed(options: EmbedOptions): ReturnType<typeof embedParts>;
/** What the Svelte `Mount` the build supplies attaches to its `<sf-s data-sf-island>`: the marker for `module` with its props inside it, mounted by the registry's entry the way the React and Vue `Mount` do theirs and patched in place when the props change. */
export declare function mount(module: string, props?: Props, _when?: MountTiming): (region: Element) => () => void;
/**
* An island another framework mounts, placed by module id as an attachment:
* `<div {@attach island("src/ui/Chart.tsx#default", { label })}></div>`. The
* registry entry decides the mounter, so this writes the marker the boot
* runtime reads and never renders the island itself. When the props change
* Svelte runs the attachment again, which patches the mounted island in place.
* An element taken out of the document ends the islands inside it.
*/
export declare function island(module: string, props?: Props, when?: MountTiming): (element: Element) => () => void;
