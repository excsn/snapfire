import { type Component } from "vue";
import { type MountTiming, type Mounter, type Patcher, type Props } from "./boot.js";
import { type StoreKey } from "./store.js";
export declare const vueMounter: Mounter;
export declare const vuePatcher: Patcher;
/** The neutral store as a Vue ref: `const region = useStore(regionKey, "all")`, readable and writable, following every other island that shares the key. Call it in `setup`, so the subscription ends with the component. */
export declare function useStore<T>(key: StoreKey<T>, initial: T): {
	value: T;
};
/** What [`Mount`] takes: the module id the registry knows the island under, the props it is mounted with and re-patched from, and the timing that schedules it. */
export interface MountProps {
	module: string;
	props?: Props;
	when?: MountTiming;
}
/**
* Places an island by module id inside a Vue tree, for a component holding
* one another framework mounts: the registry entry decides the mounter, so
* this writes the marker the boot runtime reads and never renders the child
* itself. Nothing rendered it on the server, so it is mounted fresh and
* patched from here whenever its props change.
*/
export declare const Mount: Component;
