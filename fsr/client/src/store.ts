import { decodeValue, SfValue } from "./values.js";

/** A store key: the string it is, carrying the type of what it holds. */
export type StoreKey<T> = string & { readonly __store?: T };

/** `key<number>("cart/count")`: a typed name for a store key. The build reads it through an import, so a key declared in one module and used in another still lowers. */
export function key<T>(id: string): StoreKey<T> {
  return id as StoreKey<T>;
}

export type StoreListener = (value: unknown, key: string) => void;

/** What one segment of the route seeded: `k` its segment key, `p` the slot names from the root down to it and `v` the values. The store is the merge of every contribution it holds, a deeper segment winning a key an outer one also sets and, at one depth, the later slot name, so the merge comes out the same whatever order the segments arrived in. */
export interface Contribution {
  k: string;
  p: string[];
  v: { [key: string]: unknown };
}

/** The contribution `seed()` writes: a plain map of values, outermost of all, patched by each call. */
const PLAIN = "$seed";

const contributions = new Map<string, Contribution>();
/** The merge of `contributions`, kept current. */
let merged = new Map<string, unknown>();
/** What islands wrote, over the merge; a contribution that lands for a key clears its write. */
const writes = new Map<string, unknown>();
const derivedValues = new Map<string, unknown>();
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
  for (const listener of Array.from(set)) listener(effective(k), k);
}

function recompute(id: string): void {
  const entry = derived.get(id);
  if (!entry) return;
  const before = effective(id);
  const value = entry.compute(<T,>(k: StoreKey<T>) => effective(k) as T | undefined);
  derivedValues.set(id, value);
  if (!Object.is(before, value)) notify(id);
}

/** The value a key reads as: an island's write, else a derived value, else the merge. */
function effective(k: string): unknown {
  if (writes.has(k)) return writes.get(k);
  if (derivedValues.has(k)) return derivedValues.get(k);
  return merged.get(k);
}

function has(k: string): boolean {
  return writes.has(k) || derivedValues.has(k) || merged.has(k);
}

/** A deeper contribution is later; at one depth the slot names decide. The server merges by the same rule. */
function order(a: Contribution, b: Contribution): number {
  if (a.p.length !== b.p.length) return a.p.length - b.p.length;
  for (let i = 0; i < a.p.length; i++) {
    if (a.p[i] !== b.p[i]) return a.p[i] < b.p[i] ? -1 : 1;
  }
  return 0;
}

/** Recomputes the merge and notifies every key in `touched` whose effective value moved. */
function remerge(touched: Set<string>): void {
  const before = new Map<string, unknown>();
  for (const k of touched) before.set(k, effective(k));
  const next = new Map<string, unknown>();
  for (const c of Array.from(contributions.values()).sort(order)) {
    for (const [k, value] of Object.entries(c.v)) next.set(k, value);
  }
  merged = next;
  for (const k of touched) {
    if (!Object.is(before.get(k), effective(k)) || before.has(k) !== has(k)) notify(k);
  }
}

/** What the key holds or undefined when nothing has set it. */
export function get<T>(k: StoreKey<T>): T | undefined {
  return effective(k) as T | undefined;
}

/** Writes the key and notifies its listeners, unless the value is the one already held. The write stands until a segment seeds the key again. */
export function set<T>(k: StoreKey<T>, value: T): void {
  if (has(k) && Object.is(effective(k), value)) return;
  writes.set(k, value);
  notify(k);
}

/** Forgets the key, as though nothing had ever set it. */
export function clear<T>(k: StoreKey<T>): void {
  if (!has(k)) return;
  writes.delete(k);
  derivedValues.delete(k);
  for (const c of contributions.values()) delete c.v[k];
  merged.delete(k);
  notify(k);
}

/** Forgets every key without telling anyone, which is what a new document calls for: the listeners of the old one are gone with its roots and the derived keys stay registered for the next seed to feed. */
export function reset(): void {
  contributions.clear();
  merged = new Map();
  writes.clear();
  derivedValues.clear();
}

/** Every key the store holds, for a test or a debugger. */
export function snapshot(): { [key: string]: unknown } {
  const out: { [key: string]: unknown } = {};
  for (const k of merged.keys()) out[k] = effective(k);
  for (const k of derivedValues.keys()) out[k] = effective(k);
  for (const k of writes.keys()) out[k] = effective(k);
  return out;
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

/** Runs `work` with notifications collapsed: a listener hears once per key however many times it was written. Nested calls defer to the outermost. A `work` that throws still notifies what it wrote, since the writes stay in the store. */
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
    for (const k of own) dispatch(k);
  }
}

/** A key computed from others, recomputed whenever one of them changes. */
export function derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[] | string[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void {
  derived.set(k, { sources: sources as string[], compute: compute as Derived["compute"] });
  recompute(k);
}

/** Shows `guess` at once, runs `remote` and puts the key back as it was if it fails. What the server settles on arrives with the next payload, so a success leaves the guess in place for revalidation to replace. */
export async function optimistic<T, R>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R> {
  const had = has(k);
  const before = effective(k) as T;
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

/** Takes what the segments of a response seeded, each contribution replacing the one its segment held before, in one transaction. The server is authoritative: a key a contribution names loses whatever an island wrote to it. */
export function contribute(list: Contribution[]): void {
  transaction(() => {
    const touched = new Set<string>();
    for (const c of list) {
      const old = contributions.get(c.k);
      if (old) for (const k of Object.keys(old.v)) touched.add(k);
      for (const k of Object.keys(c.v)) {
        touched.add(k);
        writes.delete(k);
      }
      contributions.set(c.k, { k: c.k, p: c.p.slice(), v: { ...c.v } });
    }
    remerge(touched);
  });
}

/** Drops the contribution of every segment not in `segments`, which the navigator calls once a payload's tree is in: a segment leaves and its keys go with it, so a page that seeds nothing does not inherit what the page before it seeded. The plain contribution `seed()` writes stays. */
export function retain(segments: Iterable<string>): void {
  const keep = new Set(segments);
  keep.add(PLAIN);
  transaction(() => {
    const touched = new Set<string>();
    for (const [segment, c] of Array.from(contributions)) {
      if (keep.has(segment)) continue;
      for (const k of Object.keys(c.v)) touched.add(k);
      contributions.delete(segment);
    }
    if (touched.size > 0) remerge(touched);
  });
}

/** Writes a whole map in one transaction, outside any segment: the plain contribution, patched by each call. The server is authoritative: a seeded key replaces whatever the browser held. */
export function seed(values: { [key: string]: SfValue }): void {
  const held = contributions.get(PLAIN);
  contribute([{ k: PLAIN, p: [], v: { ...(held?.v ?? {}), ...values } }]);
}

interface SeedGlobal {
  __sfSeed?: unknown[];
  __sfSeedApply?: (encoded: unknown) => void;
}

/** Decodes what a seed script or a `__sfStore` call carries: a list of contributions, each value map encoded as the payload encodes values. */
export function decodeContributions(encoded: unknown): Contribution[] {
  if (!Array.isArray(encoded)) return [];
  return encoded.map((item) => {
    const entry = item as { k?: unknown; p?: unknown; v?: unknown };
    return {
      k: String(entry.k ?? ""),
      p: Array.isArray(entry.p) ? entry.p.map(String) : [],
      v: (decodeValue((entry.v ?? {}) as SfValue) ?? {}) as { [key: string]: unknown },
    };
  });
}

/** Every seed script under `root`, the document by default, that nothing has read yet, each marked once it is; then any a streamed resolution left behind before this module loaded. From then on a resolution seeds the store as it arrives. Called on load and again by `boot`, since a document written after this module ran carries a seed nobody has read. Called again after a fragment is swapped in, since a fragment carries the route's seed too. */
export function adopt(root?: ParentNode): void {
  if (typeof document !== "undefined") {
    for (const script of Array.from((root ?? document).querySelectorAll("script[data-sf-store]:not([data-sf-adopted])"))) {
      if (script.textContent) {
        contribute(decodeContributions(JSON.parse(script.textContent)));
      }
      script.setAttribute("data-sf-adopted", "");
    }
  }
  if (typeof globalThis === "undefined") return;
  const g = globalThis as SeedGlobal;
  const held = g.__sfSeed;
  g.__sfSeedApply = (encoded) => contribute(decodeContributions(encoded));
  if (held) {
    delete g.__sfSeed;
    for (const encoded of held) g.__sfSeedApply(encoded);
  }
}

adopt();
