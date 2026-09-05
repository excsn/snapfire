import { SfValue } from "./values.js";
/** A store key: the string it is, carrying the type of what it holds. */
export type StoreKey<T> = string & {
	readonly __store?: T;
};
/** `key<number>("cart/count")`: a typed name for a store key. The build reads it through an import, so a key declared in one module and used in another still lowers. */
export declare function key<T>(id: string): StoreKey<T>;
export type StoreListener = (value: unknown, key: string) => void;
/** What the key holds, or undefined when nothing has set it. */
export declare function get<T>(k: StoreKey<T>): T | undefined;
/** Writes the key and notifies its listeners, unless the value is the one already held. */
export declare function set<T>(k: StoreKey<T>, value: T): void;
/** Forgets the key, as though nothing had ever set it. */
export declare function clear<T>(k: StoreKey<T>): void;
/** Every key the store holds, for a test or a debugger. */
export declare function snapshot(): {
	[key: string]: unknown;
};
/** Calls `listener` whenever the key changes; the returned function stops it. */
export declare function subscribe(k: StoreKey<unknown> | string, listener: StoreListener): () => void;
/** Runs `work` with notifications collapsed: a listener hears once per key however many times it was written. Nested calls defer to the outermost. */
export declare function transaction(work: () => void): void;
/** A key computed from others, recomputed whenever one of them changes. */
export declare function derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[] | string[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void;
/** Shows `guess` at once, runs `remote`, and puts the key back as it was if it fails. What the server settles on arrives with the next payload, so a success leaves the guess in place for revalidation to replace. */
export declare function optimistic<
	T,
	R
>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R>;
/** Writes what a route seeded, in one transaction. The server is authoritative: a seeded key replaces whatever the browser held. */
export declare function seed(values: {
	[key: string]: SfValue;
}): void;
/** The document's seed, then any a streamed resolution left behind before this module loaded. From then on a resolution seeds the store as it arrives. Called on load and again by `boot`, since a document written after this module ran carries a seed nobody has read. */
export declare function adopt(): void;
