import { SfValue } from "./values.js";
/** A store key: the string it is, carrying the type of what it holds. */
export type StoreKey<T> = string & {
	readonly __store?: T;
};
/** `key<number>("cart/count")`: a typed name for a store key. The build reads it through an import, so a key declared in one module and used in another still lowers. */
export declare function key<T>(id: string): StoreKey<T>;
export type StoreListener = (value: unknown, key: string) => void;
/** What one segment of the route seeded: `k` its segment key, `p` the slot names from the root down to it and `v` the values. The store is the merge of every contribution it holds, a deeper segment winning a key an outer one also sets and, at one depth, the later slot name, so the merge comes out the same whatever order the segments arrived in. */
export interface Contribution {
	k: string;
	p: string[];
	v: {
		[key: string]: unknown;
	};
	/** The segment's defaults: under every segment's `v` in the merge and under a write an island holds, so a visitor's choice survives the route seeding again. */
	d?: {
		[key: string]: unknown;
	};
	/** On a promise, the keys a deferred segment will seed once it resolves; its `v` is empty and its seed replaces it. */
	w?: string[];
	/** The segment whose region holds the markup that carried this contribution, for a fragment's seed of a segment the document does not show: it stays while that segment does. */
	o?: string;
}
/** Whether a deferred segment has yet to seed `k`: a promise awaits it and outranks every contribution holding it, so the value the store holds now is about to be replaced. A derived key is pending exactly while a source it reads is. The server decides the same way which islands to hold. */
export declare function isPending(k: string): boolean;
/** Resolves once none of `keys` is pending, at once when none is. One promise per pending key set while it waits, so a component suspending on it is handed the same promise on every render. */
export declare function whenSettled(keys: string[]): Promise<void>;
/** Sets the slot order the merge uses and remerges, notifying every key that moved. `adopt` calls it with what the document carries; a test calls it directly. */
export declare function setSlotOrder(list: string[]): void;
/** What the key holds or undefined when nothing has set it. */
export declare function get<T>(k: StoreKey<T>): T | undefined;
/** Writes the key and notifies its listeners, unless the value is the one already held. The write stands until a segment seeds the key again. */
export declare function set<T>(k: StoreKey<T>, value: T): void;
/** Forgets the key, as though nothing had ever set it. */
export declare function clear<T>(k: StoreKey<T>): void;
/** Forgets every key without telling anyone, which is what a new document calls for: the listeners of the old one are gone with its roots and the derived keys stay registered for the next seed to feed. */
export declare function reset(): void;
/** Every key the store holds, for a test or a debugger. */
export declare function snapshot(): {
	[key: string]: unknown;
};
/** Calls `listener` whenever the key changes; the returned function stops it. */
export declare function subscribe(k: StoreKey<unknown> | string, listener: StoreListener): () => void;
/** Runs `work` with notifications collapsed: a listener hears once per key however many times it was written. Nested calls defer to the outermost. A `work` that throws still notifies what it wrote, since the writes stay in the store. */
export declare function transaction(work: () => void): void;
/** A key computed from others, recomputed whenever one of them or the locale's messages change. */
export declare function derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[] | string[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void;
/** Shows `guess` at once, runs `remote` and puts the key back as it was if it fails. What the server settles on arrives with the next payload, so a success leaves the guess in place for revalidation to replace. */
export declare function optimistic<
	T,
	R
>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R>;
/** Takes what the segments of a response seeded, each contribution replacing the one its segment held before, in one transaction. The server is authoritative for a value: a key a contribution's `v` names loses whatever an island wrote to it. A key only its `d` names keeps the island's write. */
export declare function contribute(list: Contribution[]): void;
/** Drops the contribution of every segment not in `segments`, which the navigator calls once a payload's tree is in: a segment leaves and its keys go with it, so a page that seeds nothing does not inherit what the page before it seeded. A contribution a segment's region holds stays while that segment does. The plain contribution `seed()` writes stays. */
export declare function retain(segments: Iterable<string>): void;
/** Takes the contributions of the segments in `segments` and of what their regions hold out of the store in one transaction and returns them: what a page carries while it is held, for `restore` to put back. */
export declare function take(segments: Iterable<string>): Contribution[];
/** Puts back what `take` returned, each contribution unless the store already holds a newer one for its segment, which the payload that brought the page back seeded. */
export declare function restore(list: Contribution[]): void;
/** Writes a whole map in one transaction, outside any segment: the plain contribution, patched by each call. The server is authoritative: a seeded key replaces whatever the browser held. */
export declare function seed(values: {
	[key: string]: SfValue;
}): void;
/** Decodes what a seed script or a `__sfStore` call carries: a list of contributions, each value map encoded as the payload encodes values. */
export declare function decodeContributions(encoded: unknown): Contribution[];
/** Every seed script under `root`, the document by default, that nothing has read yet, each marked once it is; then any a streamed resolution left behind before this module loaded. From then on a resolution seeds the store as it arrives. Called on load and again by `boot`, since a document written after this module ran carries a seed nobody has read. Called again after a fragment is swapped in, since a fragment carries the route's seed too. */
export declare function adopt(root?: ParentNode): void;
