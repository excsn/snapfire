import { decodeValue } from "./values.js";
export function key(id) {
    return id;
}
const PLAIN = "$seed";
const contributions = new Map();
let merged = new Map();
const writes = new Map();
const derivedValues = new Map();
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
    for (const listener of Array.from(set))listener(effective(k), k);
}
function recompute(id) {
    const entry = derived.get(id);
    if (!entry) return;
    const before = effective(id);
    const value = entry.compute((k)=>effective(k));
    derivedValues.set(id, value);
    if (!Object.is(before, value)) notify(id);
}
function effective(k) {
    if (writes.has(k)) return writes.get(k);
    if (derivedValues.has(k)) return derivedValues.get(k);
    return merged.get(k);
}
function has(k) {
    return writes.has(k) || derivedValues.has(k) || merged.has(k);
}
let slotOrder = [];
function order(a, b) {
    if (a.p.length !== b.p.length) return a.p.length - b.p.length;
    for(let i = 0; i < a.p.length; i++){
        if (a.p[i] === b.p[i]) continue;
        const [x, y] = [
            slotOrder.indexOf(a.p[i]),
            slotOrder.indexOf(b.p[i])
        ];
        if (x !== y) return x - y;
        return a.p[i] < b.p[i] ? -1 : 1;
    }
    return 0;
}
function samePath(a, b) {
    return a.length === b.length && a.every((name, i)=>name === b[i]);
}
export function isPending(k) {
    for (const promise of contributions.values()){
        if (!promise.w?.includes(k)) continue;
        const outranked = Array.from(contributions.values()).some((c)=>c.k !== promise.k && k in c.v && order(c, promise) >= 0);
        if (!outranked) return true;
    }
    return false;
}
const waiters = new Set();
const settling = new Map();
export function whenSettled(keys) {
    const waiting = keys.filter(isPending);
    if (waiting.length === 0) return Promise.resolve();
    const id = waiting.slice().sort().join("\u0000");
    const held = settling.get(id);
    if (held) return held;
    const promise = new Promise((resolve)=>waiters.add({
            keys: waiting,
            resolve
        }));
    settling.set(id, promise);
    void promise.then(()=>settling.delete(id));
    return promise;
}
function release() {
    for (const waiter of Array.from(waiters)){
        if (waiter.keys.some(isPending)) continue;
        waiters.delete(waiter);
        waiter.resolve();
    }
}
export function setSlotOrder(list) {
    slotOrder = list.slice();
    transaction(()=>{
        const touched = new Set();
        for (const c of contributions.values())for (const k of Object.keys(c.v))touched.add(k);
        remerge(touched);
    });
    release();
}
function remerge(touched) {
    const before = new Map();
    for (const k of touched)before.set(k, effective(k));
    const next = new Map();
    for (const c of Array.from(contributions.values()).sort(order)){
        for (const [k, value] of Object.entries(c.v))next.set(k, value);
    }
    merged = next;
    for (const k of touched){
        if (!Object.is(before.get(k), effective(k)) || before.has(k) !== has(k)) notify(k);
    }
}
export function get(k) {
    return effective(k);
}
export function set(k, value) {
    if (has(k) && Object.is(effective(k), value)) return;
    writes.set(k, value);
    notify(k);
}
export function clear(k) {
    if (!has(k)) return;
    writes.delete(k);
    derivedValues.delete(k);
    for (const c of contributions.values())delete c.v[k];
    merged.delete(k);
    notify(k);
}
export function reset() {
    contributions.clear();
    merged = new Map();
    writes.clear();
    derivedValues.clear();
    release();
}
export function snapshot() {
    const out = {};
    for (const k of merged.keys())out[k] = effective(k);
    for (const k of derivedValues.keys())out[k] = effective(k);
    for (const k of writes.keys())out[k] = effective(k);
    return out;
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
    const had = has(k);
    const before = effective(k);
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
export function contribute(list) {
    transaction(()=>{
        const touched = new Set();
        for (const c of list){
            const old = contributions.get(c.k);
            if (old) for (const k of Object.keys(old.v))touched.add(k);
            for (const k of Object.keys(c.v)){
                touched.add(k);
                writes.delete(k);
            }
            const before = old ?? Array.from(contributions.values()).find((held)=>held.k !== c.k && samePath(held.p, c.p));
            const values = c.w && before ? before.v : c.v;
            contributions.set(c.k, {
                k: c.k,
                p: c.p.slice(),
                v: {
                    ...values
                },
                ...c.w ? {
                    w: c.w.slice()
                } : {}
            });
        }
        remerge(touched);
    });
    release();
}
export function retain(segments) {
    const keep = new Set(segments);
    keep.add(PLAIN);
    transaction(()=>{
        const touched = new Set();
        for (const [segment, c] of Array.from(contributions)){
            if (keep.has(segment)) continue;
            for (const k of Object.keys(c.v))touched.add(k);
            contributions.delete(segment);
        }
        if (touched.size > 0) remerge(touched);
    });
    release();
}
export function seed(values) {
    const held = contributions.get(PLAIN);
    contribute([
        {
            k: PLAIN,
            p: [],
            v: {
                ...held?.v ?? {},
                ...values
            }
        }
    ]);
}
export function decodeContributions(encoded) {
    if (!Array.isArray(encoded)) return [];
    return encoded.map((item)=>{
        const entry = item;
        return {
            k: String(entry.k ?? ""),
            p: Array.isArray(entry.p) ? entry.p.map(String) : [],
            v: decodeValue(entry.v ?? {}) ?? {},
            ...Array.isArray(entry.w) ? {
                w: entry.w.map(String)
            } : {}
        };
    });
}
export function adopt(root) {
    if (typeof document !== "undefined") {
        if (root === undefined) {
            const carried = document.documentElement?.getAttribute("data-sf-slot-order") ?? null;
            let list = [];
            if (carried !== null) {
                try {
                    const parsed = JSON.parse(carried);
                    if (Array.isArray(parsed)) list = parsed.map(String);
                } catch  {
                    list = [];
                }
            }
            if (list.length !== slotOrder.length || list.some((name, i)=>name !== slotOrder[i])) setSlotOrder(list);
        }
        for (const script of Array.from((root ?? document).querySelectorAll("script[data-sf-store]:not([data-sf-adopted])"))){
            if (script.textContent) {
                contribute(decodeContributions(JSON.parse(script.textContent)));
            }
            script.setAttribute("data-sf-adopted", "");
        }
    }
    if (typeof globalThis === "undefined") return;
    const g = globalThis;
    const held = g.__sfSeed;
    g.__sfSeedApply = (encoded)=>contribute(decodeContributions(encoded));
    if (held) {
        delete g.__sfSeed;
        for (const encoded of held)g.__sfSeedApply(encoded);
    }
}
adopt();
//# sourceMappingURL=store.js.map
