import type { Mounter, Patcher } from "./boot.js";
import { type StoreKey } from "./store.js";
export declare const vueMounter: Mounter;
export declare const vuePatcher: Patcher;
/** The neutral store as a Vue ref: `const region = useStore(regionKey, "all")`, readable and writable, following every other island that shares the key. */
export declare function useStore<T>(key: StoreKey<T>, initial: T): {
	value: T;
};
