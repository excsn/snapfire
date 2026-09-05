import { decodeValue, SfValue } from "./values.js";

/** A store key: the string it is, carrying the type of what it holds. */
export type StoreKey<T> = string & { readonly __store?: T };

/** `key<number>("cart/count")`: a typed name for a store key. The build reads it through an import, so a key declared in one module and used in another still lowers. */
export function key<T>(id: string): StoreKey<T> {
  return id as StoreKey<T>;
}

export type StoreListener = (value: unknown, key: string) => void;

const values = new Map<string, unknown>();
const listeners = new Map<string, Set<StoreListener>>();

interface Derived {
  sources: string[];
  compute: (read: <T>(k: StoreKey<T>) => T | undefined) => unknown;
}

const derived = new Map<string, Derived>();

let depth = 0;
let dirtied: Set<string> | null = null;

function notify(k: string): void {
  if (dirtied) {
    dirtied.add(k);
    return;
  }
  dispatch(k);
}

function dispatch(k: string): void {
  for (const [id, entry] of derived) {
    if (id !== k && entry.sources.includes(k)) recompute(id);
  }
  const set = listeners.get(k);
  if (!set) return;
  for (const listener of Array.from(set)) listener(values.get(k), k);
}

function recompute(id: string): void {
  const entry = derived.get(id);
  if (!entry) return;
  write(id, entry.compute(<T,>(k: StoreKey<T>) => values.get(k) as T | undefined));
}

function write(k: string, value: unknown): void {
  if (values.has(k) && Object.is(values.get(k), value)) return;
  values.set(k, value);
  notify(k);
}

/** What the key holds, or undefined when nothing has set it. */
export function get<T>(k: StoreKey<T>): T | undefined {
  return values.get(k) as T | undefined;
}

/** Writes the key and notifies its listeners, unless the value is the one already held. */
export function set<T>(k: StoreKey<T>, value: T): void {
  write(k, value);
}

/** Forgets the key, as though nothing had ever set it. */
export function clear<T>(k: StoreKey<T>): void {
  if (!values.has(k)) return;
  values.delete(k);
  notify(k);
}

/** Every key the store holds, for a test or a debugger. */
export function snapshot(): { [key: string]: unknown } {
  return Object.fromEntries(values);
}

/** Calls `listener` whenever the key changes; the returned function stops it. */
export function subscribe(k: StoreKey<unknown> | string, listener: StoreListener): () => void {
  let set = listeners.get(k);
  if (!set) {
    set = new Set();
    listeners.set(k, set);
  }
  set.add(listener);
  return () => {
    set.delete(listener);
    if (set.size === 0) listeners.delete(k);
  };
}

/** Runs `work` with notifications collapsed: a listener hears once per key however many times it was written. Nested calls defer to the outermost. */
export function transaction(work: () => void): void {
  if (depth > 0) {
    work();
    return;
  }
  const own = new Set<string>();
  depth = 1;
  dirtied = own;
  try {
    work();
  } finally {
    depth = 0;
    dirtied = null;
  }
  for (const k of own) dispatch(k);
}

/** A key computed from others, recomputed whenever one of them changes. */
export function derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[] | string[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void {
  derived.set(k, { sources: sources as string[], compute: compute as Derived["compute"] });
  recompute(k);
}

/** Shows `guess` at once, runs `remote`, and puts the key back as it was if it fails. What the server settles on arrives with the next payload, so a success leaves the guess in place for revalidation to replace. */
export async function optimistic<T, R>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R> {
  const had = values.has(k);
  const before = values.get(k) as T;
  set(k, guess);
  try {
    return await remote();
  } catch (err) {
    if (had) {
      set(k, before);
    } else {
      clear(k);
    }
    throw err;
  }
}

/** Writes what a route seeded, in one transaction. The server is authoritative: a seeded key replaces whatever the browser held. */
export function seed(values: { [key: string]: SfValue }): void {
  transaction(() => {
    for (const [k, value] of Object.entries(values)) write(k, value);
  });
}

interface SeedGlobal {
  __sfSeed?: { [key: string]: unknown };
  __sfSeedApply?: (encoded: { [key: string]: unknown }) => void;
}

/** The document's seed, then any a streamed resolution left behind before this module loaded. From then on a resolution seeds the store as it arrives. Called on load and again by `boot`, since a document written after this module ran carries a seed nobody has read. */
export function adopt(): void {
  if (typeof document !== "undefined") {
    const script = document.querySelector("script[data-sf-store]");
    if (script?.textContent) {
      seed(decodeValue(JSON.parse(script.textContent)) as { [key: string]: SfValue });
    }
  }
  if (typeof globalThis === "undefined") return;
  const g = globalThis as SeedGlobal;
  const held = g.__sfSeed;
  g.__sfSeedApply = (encoded) => seed(decodeValue(encoded) as { [key: string]: SfValue });
  if (held) {
    delete g.__sfSeed;
    g.__sfSeedApply(held);
  }
}

adopt();
