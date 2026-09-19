import { adoptCatalog, adoptLocale } from "./locale.js";
import { isServerIsland, mountServer, patchServer } from "./server.js";
import { adopt } from "./store.js";
import { decodeValue } from "./values.js";
const mounted = new WeakMap();
const pending = new WeakMap();
const islands = new Map();
export function registerIsland(moduleId, entry) {
    islands.set(moduleId, entry);
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
    return above !== null && above !== undefined && !serverRendered(above);
}
function mountNow(entry, moduleId, el, props) {
    const hydrate = serverRendered(el);
    const island = {
        entry,
        moduleId,
        handle: Promise.resolve(undefined),
        root: undefined,
        gone: false,
        props,
        regions: null,
        children: null,
        child: null
    };
    island.handle = entry.loader().then((mod)=>island.gone ? undefined : entry.mount(mod, props, el, hydrate)).then((value)=>{
        if (island.gone) return undefined;
        island.root = value;
        el.setAttribute(MOUNTED, "");
        return value;
    }).catch((err)=>{
        console.warn(`sf: mounting ${moduleId} failed`, err);
        return undefined;
    });
    mounted.set(el, island);
}
export function discard(root) {
    const markers = Array.from(root.querySelectorAll("sf-i"));
    if (root instanceof Element && root.tagName === "SF-I") markers.unshift(root);
    for (const el of markers.reverse()){
        pending.get(el)?.();
        pending.delete(el);
        const island = mounted.get(el);
        if (!island) continue;
        island.gone = true;
        mounted.delete(el);
        if (island.root !== undefined) island.entry.unmount?.(island.root, el);
    }
}
export function islandState(el) {
    const island = mounted.get(el);
    return island ? {
        props: island.props,
        regions: island.regions,
        children: island.children,
        child: island.child
    } : null;
}
export function treeRootOf(node) {
    const slot = node?.parentNode;
    if (!(slot instanceof Element) || slot.tagName !== "SF-S" || slot.hasAttribute("data-sf-island") || slot.hasAttribute("data-sf-name") || slot.hasAttribute("data-sf-children")) return null;
    const root = slot.parentElement?.closest("sf-i");
    if (!root) return null;
    const entry = islands.get(root.getAttribute("data-sf-module") ?? "");
    return entry?.claims ? root : null;
}
export function adoptTreeChild(marker, root) {
    const moduleId = marker.getAttribute("data-sf-module") ?? "";
    const page = islands.get(moduleId);
    const tree = mounted.get(root);
    if (!page || !tree?.child || mounted.has(marker)) return;
    const entry = {
        loader: page.loader,
        mount: ()=>undefined,
        patch: (_handle, _module, props, el)=>{
            const state = mounted.get(el);
            const child = mounted.get(root)?.child;
            if (!state || !child) return;
            child.props = props;
            child.regions = state.regions;
            child.children = state.children;
            child.gen += 1;
            void rerender(root);
        },
        unmount: ()=>undefined
    };
    mounted.set(marker, {
        entry,
        moduleId,
        handle: Promise.resolve(root),
        root,
        gone: false,
        props: tree.child.props,
        regions: tree.child.regions,
        children: tree.child.children,
        child: null
    });
    marker.setAttribute(SCHEDULED, "");
    marker.setAttribute(MOUNTED, "");
}
async function rerender(root) {
    const island = mounted.get(root);
    if (!island?.entry.patch) return false;
    const handle = await island.handle;
    if (handle === undefined) return false;
    const mod = await island.entry.loader();
    island.entry.patch(handle, mod, island.props, root);
    return true;
}
const landing = new Set();
export function treeSettled() {
    return Promise.all(landing).then(()=>undefined);
}
export function setTreeChild(root, child) {
    const work = (async ()=>{
        const island = mounted.get(root);
        if (!island) return false;
        const handle = await island.handle;
        if (handle === undefined || island.gone) return false;
        const page = child.module === null ? undefined : islands.get(child.module);
        if (page) child.component = await page.loader();
        child.instance = (island.child?.instance ?? 0) + 1;
        for (const slot of Array.from(root.querySelectorAll("sf-s:not([data-sf-island]):not([data-sf-name]):not([data-sf-children])"))){
            if (slot.parentElement?.closest("sf-i") === root) discard(slot);
        }
        island.child = child;
        return rerender(root);
    })();
    landing.add(work);
    void work.finally(()=>landing.delete(work));
    return work;
}
export function holdTreeChild(root, child) {
    const island = mounted.get(root);
    if (island) island.child = child;
}
export async function patchIsland(el, props, regions = null, children = null, encoded) {
    if (isServerIsland(el)) return patchServer(el, props, encoded);
    const island = mounted.get(el);
    if (!island?.entry.patch) return false;
    const handle = await island.handle;
    if (handle === undefined) return false;
    const mod = await island.entry.loader();
    island.props = props;
    island.regions = regions;
    island.children = children;
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
export function scan(root) {
    for (const el of Array.from(root.querySelectorAll(`sf-i:not([${SCHEDULED}])`))){
        const moduleId = el.getAttribute("data-sf-module");
        if (!moduleId) continue;
        if (awaitingAnAncestor(el)) continue;
        if (el.parentElement?.closest("sf-s[data-sf-mode]")?.getAttribute("data-sf-mode") === "server") {
            el.setAttribute(SCHEDULED, "");
            el.setAttribute(MOUNTED, "");
            mountServer(el, moduleId, rawPropsFor(root, el.id));
            continue;
        }
        const tree = treeRootOf(el);
        if (tree && islands.get(tree.getAttribute("data-sf-module") ?? "")?.claims?.(el)) {
            el.setAttribute(SCHEDULED, "");
            continue;
        }
        const entry = islands.get(moduleId);
        if (!entry) {
            missing.add(moduleId);
            arm();
            continue;
        }
        el.setAttribute(SCHEDULED, "");
        const placed = el.parentElement?.closest("sf-s[data-sf-when]")?.getAttribute("data-sf-when");
        schedule(placed ? {
            ...entry,
            when: placed
        } : entry, moduleId, el, propsFor(root, el.id));
    }
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
    const run = ()=>scan(document);
    adopt();
    adoptLocale();
    adoptCatalog();
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
    const head = document.head;
    const wanted = new Set(hrefs.map((href)=>new URL(href, location.href).href));
    const held = new Map();
    for (const link of Array.from(head.querySelectorAll(`link[${CSS_MARK}]`))){
        held.set(link.href, link);
    }
    for (const [href, link] of held){
        if (!wanted.has(href)) link.remove();
    }
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
//# sourceMappingURL=boot.js.map
