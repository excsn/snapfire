import { enableEmbeds } from "./embed.js";
import { adoptCatalog, adoptLocale } from "./locale.js";
import { loadDeclaredScripts, loadMarkedScript } from "./script.js";
import { settlePlaced, unplaced } from "./places.js";
import { enablePwa } from "./pwa.js";
import { REGION_KEY } from "./render.js";
import { endServer, isServerIsland, mountServer, patchServer } from "./server.js";
import { adopt, isPending, whenSettled } from "./store.js";
import { decodeValue, isRef } from "./values.js";
const mounted = new WeakMap();
const paused = new WeakSet();
const pending = new WeakMap();
const islands = new Map();
export function registerIsland(moduleId, entry) {
    islands.set(moduleId, entry);
}
const modules = new Map();
export function registerModule(file, loader) {
    modules.set(file, loader);
}
async function resolveModuleRefs(props) {
    const refs = [];
    const walk = (value, set)=>{
        if (isRef(value)) {
            if (value.kind === "module") refs.push({
                id: value.id,
                set
            });
            return;
        }
        if (Array.isArray(value)) {
            value.forEach((item, i)=>walk(item, (v)=>value[i] = v));
            return;
        }
        if (value && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
            const record = value;
            for (const key of Object.keys(record))walk(record[key], (v)=>record[key] = v);
        }
    };
    for (const key of Object.keys(props))walk(props[key], (v)=>props[key] = v);
    if (refs.length === 0) return props;
    await Promise.all(refs.map(async ({ id, set })=>{
        const at = id.lastIndexOf("#");
        const file = at < 0 ? id : id.slice(0, at);
        const name = at < 0 ? "default" : id.slice(at + 1);
        const loader = modules.get(file);
        if (!loader) {
            console.warn(`sf: no module registered for ${file}`);
            return;
        }
        const mod = await loader();
        set(mod[name]);
    }));
    return props;
}
export function registeredIslands() {
    return islands;
}
function rawPropsFor(root, id) {
    const script = root.querySelector(`script[data-sf-props="${id}"]`) ?? document.querySelector(`script[data-sf-props="${id}"]`);
    if (!script || !script.textContent) return {};
    return JSON.parse(script.textContent);
}
function propsFor(root, id) {
    return decodeValue(rawPropsFor(root, id));
}
export function markerProps(marker) {
    const next = marker.nextElementSibling;
    const script = next?.tagName === "SCRIPT" && next.getAttribute("data-sf-props") === marker.id ? next : null;
    const encoded = script?.textContent ? JSON.parse(script.textContent) : {};
    return {
        script,
        props: decodeValue(encoded),
        encoded
    };
}
export const defineMounter = ()=>undefined;
export function serverRendered(el) {
    return Array.from(el.childNodes).some((node)=>!(node instanceof Element && node.tagName === "SF-S"));
}
function awaitingAnAncestor(el) {
    const above = el.parentElement?.closest("sf-i");
    return above !== null && above !== undefined && !serverRendered(above) && !above.hasAttribute(MOUNTED);
}
let lastMount = Promise.resolve();
const fresh = new WeakSet();
function mountNow(entry, moduleId, el, props) {
    const hydrate = !fresh.has(el) && serverRendered(el);
    fresh.delete(el);
    const island = {
        entry,
        moduleId,
        handle: Promise.resolve(undefined),
        root: undefined,
        gone: false,
        props,
        regions: null,
        children: null,
        markup: null
    };
    const ready = Promise.all([
        entry.loader(),
        resolveModuleRefs(props)
    ]);
    const turn = lastMount.then(()=>ready);
    island.handle = turn.then(([mod, resolved])=>{
        island.props = resolved;
        return island.gone ? undefined : entry.mount(mod, resolved, el, hydrate);
    }).then((value)=>{
        if (island.gone) return undefined;
        island.root = value;
        el.setAttribute(MOUNTED, "");
        return value;
    }).catch((err)=>{
        console.warn(`sf: mounting ${moduleId} failed`, err);
        return undefined;
    });
    lastMount = island.handle;
    mounted.set(el, island);
}
export function discard(root) {
    const markers = Array.from(root.querySelectorAll("sf-i"));
    if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
    for (const el of markers.reverse()){
        pending.get(el)?.();
        pending.delete(el);
        paused.delete(el);
        endServer(el);
        const island = mounted.get(el);
        if (!island) continue;
        island.gone = true;
        mounted.delete(el);
        if (island.root !== undefined) island.entry.unmount?.(island.root, el);
    }
}
export function release(root) {
    const markers = Array.from(root.querySelectorAll("sf-i"));
    if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
    for (const el of markers.reverse()){
        const island = mounted.get(el);
        if (island && !island.entry.unmount) continue;
        if (island && island.root !== undefined && island.entry.pause?.(island.root, el, true)) {
            paused.add(el);
            continue;
        }
        discard(el);
        el.removeAttribute(SCHEDULED);
        el.removeAttribute(MOUNTED);
    }
}
export function resume(root) {
    const markers = Array.from(root.querySelectorAll("sf-i"));
    if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
    const shown = [];
    for (const el of markers){
        if (!paused.delete(el)) continue;
        const island = mounted.get(el);
        const showing = island && island.root !== undefined ? island.entry.pause?.(island.root, el, false) : undefined;
        if (showing instanceof Promise) shown.push(showing);
    }
    return Promise.all(shown).then(()=>{});
}
export function islandState(el) {
    const island = mounted.get(el);
    return island ? {
        props: island.props,
        regions: island.regions,
        children: island.children,
        markup: island.markup
    } : null;
}
export async function patchIsland(el, props, regions = null, children = null, encoded, markup = null) {
    if (isServerIsland(el)) return patchServer(el, props, encoded);
    const island = mounted.get(el);
    if (!island?.entry.patch) return false;
    const handle = await island.handle;
    if (handle === undefined) return false;
    const mod = await island.entry.loader();
    props = await resolveModuleRefs(props);
    island.props = props;
    island.regions = regions;
    island.children = children;
    island.markup = markup;
    island.entry.patch(handle, mod, props, el);
    return true;
}
function schedule(entry, moduleId, el, props) {
    switch(entry.when ?? "load"){
        case "load":
            mountNow(entry, moduleId, el, props);
            return;
        case "visible":
            {
                const observer = new IntersectionObserver((entries)=>{
                    if (entries.some((e)=>e.isIntersecting)) {
                        observer.disconnect();
                        pending.delete(el);
                        mountNow(entry, moduleId, el, props);
                    }
                });
                pending.set(el, ()=>observer.disconnect());
                observer.observe(el);
                return;
            }
        case "idle":
            {
                let off = false;
                const run = ()=>{
                    pending.delete(el);
                    if (!off) mountNow(entry, moduleId, el, props);
                };
                pending.set(el, ()=>{
                    off = true;
                });
                const idle = window.requestIdleCallback;
                if (idle) {
                    idle(run);
                } else {
                    setTimeout(run, 1);
                }
                return;
            }
    }
}
const SCHEDULED = "data-sf-scheduled";
const MOUNTED = "data-sf-mounted";
function islandName(el, props) {
    const key = props[REGION_KEY];
    return typeof key === "string" ? key : el.id;
}
export function mountedIsland(name) {
    return Array.from(document.querySelectorAll(`sf-i[${MOUNTED}]`)).some((el)=>el.id === name || markerProps(el).props[REGION_KEY] === name);
}
export function remount(name) {
    for (const el of Array.from(document.querySelectorAll("sf-i"))){
        if (el.id !== name && markerProps(el).props[REGION_KEY] !== name) continue;
        discard(el);
        el.removeAttribute(SCHEDULED);
        el.removeAttribute(MOUNTED);
        fresh.add(el);
        if (el.parentNode) scan(el.parentNode);
        return;
    }
}
export function scan(root) {
    settlePlaced();
    const orphans = unplaced();
    const streaming = document.querySelector("[data-sf-slot]") !== null;
    for (const el of Array.from(root.querySelectorAll(`sf-i:not([${SCHEDULED}])`))){
        const moduleId = el.getAttribute("data-sf-module");
        if (!moduleId) continue;
        if (awaitingAnAncestor(el)) continue;
        if (el.parentElement?.closest("sf-s[data-sf-mode]")?.getAttribute("data-sf-mode") === "server") {
            el.setAttribute(SCHEDULED, "");
            const raw = {
                ...rawPropsFor(root, el.id) ?? {}
            };
            const keys = awaited(decodeValue(raw[AWAITS] ?? null));
            delete raw[AWAITS];
            afterSettled(el, keys, ()=>{
                el.setAttribute(MOUNTED, "");
                mountServer(el, moduleId, raw);
            });
            continue;
        }
        const entry = islands.get(moduleId);
        if (!entry) {
            missing.add(moduleId);
            arm();
            continue;
        }
        const props = propsFor(root, el.id);
        const runs = orphans.get(islandName(el, props));
        if (runs && streaming) continue;
        if (runs) {
            for (const piece of runs)for (const n of piece.nodes)n.remove();
            fresh.add(el);
        }
        el.setAttribute(SCHEDULED, "");
        const placed = el.parentElement?.closest("sf-s[data-sf-when]")?.getAttribute("data-sf-when");
        const keys = awaited(props[AWAITS]);
        delete props[AWAITS];
        afterSettled(el, keys, ()=>schedule(placed ? {
                ...entry,
                when: placed
            } : entry, moduleId, el, props));
    }
}
const AWAITS = "$aw";
function awaited(value) {
    return Array.isArray(value) ? value.map(String) : [];
}
function afterSettled(el, keys, then) {
    if (!keys.some(isPending)) {
        then();
        return;
    }
    let off = false;
    pending.set(el, ()=>{
        off = true;
    });
    void whenSettled(keys).then(()=>{
        if (off) return;
        pending.delete(el);
        then();
    });
}
const missing = new Set();
const entries = new Set();
let loading = 0;
let armed = false;
function report() {
    if (loading > 0) return;
    for (const moduleId of missing){
        if (!islands.has(moduleId)) console.warn(`sf: no island registered for ${moduleId}`);
    }
    missing.clear();
}
function arm() {
    if (armed) return;
    armed = true;
    const run = ()=>{
        armed = false;
        report();
    };
    if (document.readyState === "complete") queueMicrotask(run);
    else document.addEventListener("DOMContentLoaded", run, {
        once: true
    });
}
export function loadEntry(src) {
    if (entries.has(src)) return;
    entries.add(src);
    loading += 1;
    import(src).then(()=>scan(document)).catch((err)=>{
        entries.delete(src);
        console.warn(`sf: loading ${src} failed`, err);
    }).finally(()=>{
        loading -= 1;
        report();
    });
}
const filling = new WeakSet();
export function boot() {
    const g = globalThis;
    g.__sf = Object.assign(g.__sf ?? {}, {
        script: loadMarkedScript
    });
    const run = ()=>{
        scan(document);
        loadDeclaredScripts(document);
    };
    adopt();
    adoptLocale();
    adoptCatalog();
    enableEmbeds();
    enablePwa();
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", run, {
            once: true
        });
    } else {
        run();
    }
    if (!filling.has(document)) {
        filling.add(document);
        document.addEventListener("sf:fill", run);
    }
}
const CSS_MARK = "data-sf-css";
export function applyStyles(hrefs, timeout = 2000) {
    return addStyles(hrefs, timeout).then(()=>pruneStyles(hrefs));
}
export function addStyles(hrefs, timeout = 2000) {
    const head = document.head;
    const held = new Set(Array.from(head.querySelectorAll(`link[${CSS_MARK}]`)).map((link)=>link.href));
    const pending = [];
    for (const href of hrefs){
        if (held.has(new URL(href, location.href).href)) continue;
        const link = document.createElement("link");
        link.rel = "stylesheet";
        link.setAttribute(CSS_MARK, "");
        link.href = href;
        pending.push(new Promise((done)=>{
            const settle = ()=>done();
            link.addEventListener("load", settle, {
                once: true
            });
            link.addEventListener("error", settle, {
                once: true
            });
            setTimeout(settle, timeout);
        }));
        head.appendChild(link);
    }
    return pending.length === 0 ? Promise.resolve() : Promise.all(pending).then(()=>undefined);
}
export function pruneStyles(hrefs) {
    const wanted = new Set(hrefs.map((href)=>new URL(href, location.href).href));
    for (const link of Array.from(document.head.querySelectorAll(`link[${CSS_MARK}]`))){
        if (!wanted.has(link.href)) link.remove();
    }
}
//# sourceMappingURL=boot.js.map
