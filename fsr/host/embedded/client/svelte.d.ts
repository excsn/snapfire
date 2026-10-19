import { type Mounter, type Patcher, type Unmounter } from "./boot.js";
import { type StoreKey } from "./store.js";
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
