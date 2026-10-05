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
}
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
/** A key computed from others, recomputed whenever one of them changes. */
export declare function derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[] | string[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void;
/** Shows `guess` at once, runs `remote` and puts the key back as it was if it fails. What the server settles on arrives with the next payload, so a success leaves the guess in place for revalidation to replace. */
export declare function optimistic<
	T,
	R
>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R>;
/** Takes what the segments of a response seeded, each contribution replacing the one its segment held before, in one transaction. The server is authoritative: a key a contribution names loses whatever an island wrote to it. */
export declare function contribute(list: Contribution[]): void;
/** Drops the contribution of every segment not in `segments`, which the navigator calls once a payload's tree is in: a segment leaves and its keys go with it, so a page that seeds nothing does not inherit what the page before it seeded. The plain contribution `seed()` writes stays. */
export declare function retain(segments: Iterable<string>): void;
/** Writes a whole map in one transaction, outside any segment: the plain contribution, patched by each call. The server is authoritative: a seeded key replaces whatever the browser held. */
export declare function seed(values: {
	[key: string]: SfValue;
}): void;
/** Decodes what a seed script or a `__sfStore` call carries: a list of contributions, each value map encoded as the payload encodes values. */
export declare function decodeContributions(encoded: unknown): Contribution[];
/** Every seed script under `root`, the document by default, that nothing has read yet, each marked once it is; then any a streamed resolution left behind before this module loaded. From then on a resolution seeds the store as it arrives. Called on load and again by `boot`, since a document written after this module ran carries a seed nobody has read. Called again after a fragment is swapped in, since a fragment carries the route's seed too. */
export declare function adopt(root?: ParentNode): void;
