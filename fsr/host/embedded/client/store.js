import { decodeValue } from "./values.js";
export function key(id) {
    return id;
}
const values = new Map();
const listeners = new Map();
const derived = new Map();
let depth = 0;
let dirtied = null;
function notify(k) {
    if (dirtied) {
        dirtied.add(k);
        return;
    }
    dispatch(k);
}
function dispatch(k) {
    for (const [id, entry] of derived){
        if (id !== k && entry.sources.includes(k)) recompute(id);
    }
    const set = listeners.get(k);
    if (!set) return;
    for (const listener of Array.from(set))listener(values.get(k), k);
}
function recompute(id) {
    const entry = derived.get(id);
    if (!entry) return;
    write(id, entry.compute((k)=>values.get(k)));
}
function write(k, value) {
    if (values.has(k) && Object.is(values.get(k), value)) return;
    values.set(k, value);
    notify(k);
}
export function get(k) {
    return values.get(k);
}
export function set(k, value) {
    write(k, value);
}
export function clear(k) {
    if (!values.has(k)) return;
    values.delete(k);
    notify(k);
}
export function reset() {
    values.clear();
}
export function snapshot() {
    return Object.fromEntries(values);
}
export function subscribe(k, listener) {
    let set = listeners.get(k);
    if (!set) {
        set = new Set();
        listeners.set(k, set);
    }
    set.add(listener);
    return ()=>{
        set.delete(listener);
        if (set.size === 0) listeners.delete(k);
    };
}
export function transaction(work) {
    if (depth > 0) {
        work();
        return;
    }
    const own = new Set();
    depth = 1;
    dirtied = own;
    try {
        work();
    } finally{
        depth = 0;
        dirtied = null;
        for (const k of own)dispatch(k);
    }
}
export function derive(k, sources, compute) {
    derived.set(k, {
        sources: sources,
        compute: compute
    });
    recompute(k);
}
export async function optimistic(k, guess, remote) {
    const had = values.has(k);
    const before = values.get(k);
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
export function seed(values) {
    transaction(()=>{
        for (const [k, value] of Object.entries(values))write(k, value);
    });
}
export function adopt() {
    if (typeof document !== "undefined") {
        const script = document.querySelector("script[data-sf-store]");
        if (script?.textContent) {
            seed(decodeValue(JSON.parse(script.textContent)));
        }
    }
    if (typeof globalThis === "undefined") return;
    const g = globalThis;
    const held = g.__sfSeed;
    g.__sfSeedApply = (encoded)=>seed(decodeValue(encoded));
    if (held) {
        delete g.__sfSeed;
        g.__sfSeedApply(held);
    }
}
adopt();
//# sourceMappingURL=store.js.map
