import { type MountTiming, type Mounter, type Patcher, type Props, type Unmounter } from "./boot.js";
import { type StoreKey } from "./store.js";
import { type LinkOptions } from "./link.js";
import { type PictureOptions } from "./picture.js";
import { embedParts, type EmbedOptions } from "./embed.js";
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
