import { adoptCatalog, adoptLocale } from "./locale.js";
import { isServerIsland, mountServer, patchServer } from "./server.js";
import { adopt } from "./store.js";
import { decodeValue } from "./values.js";
const mounted = new WeakMap();
const islands = new Map();
export function registerIsland(moduleId, entry) {
    islands.set(moduleId, entry);
}
export function registeredIslands() {
    return islands;
}
function propsFor(root, id) {
    const script = root.querySelector(`script[data-sf-props="${id}"]`) ?? document.querySelector(`script[data-sf-props="${id}"]`);
    if (!script || !script.textContent) return {};
    return decodeValue(JSON.parse(script.textContent));
}
export function serverRendered(el) {
    return Array.from(el.childNodes).some((node)=>!(node instanceof Element && node.tagName === "SF-S"));
}
function awaitingAnAncestor(el) {
    const above = el.parentElement?.closest("sf-i");
    return above !== null && above !== undefined && !serverRendered(above);
}
function mountNow(entry, moduleId, el, props) {
    const hydrate = serverRendered(el);
    const handle = entry.loader().then((mod)=>entry.mount(mod, props, el, hydrate)).catch((err)=>{
        console.warn(`sf: mounting ${moduleId} failed`, err);
        return undefined;
    });
    mounted.set(el, {
        entry,
        moduleId,
        handle,
        props,
        regions: null
    });
}
export function islandState(el) {
    const island = mounted.get(el);
    return island ? {
        props: island.props,
        regions: island.regions
    } : null;
}
export async function patchIsland(el, props, regions = null) {
    if (isServerIsland(el)) return patchServer(el, props);
    const island = mounted.get(el);
    if (!island?.entry.patch) return false;
    const handle = await island.handle;
    if (handle === undefined) return false;
    const mod = await island.entry.loader();
    island.props = props;
    island.regions = regions;
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
                        mountNow(entry, moduleId, el, props);
                    }
                });
                observer.observe(el);
                return;
            }
        case "idle":
            {
                const idle = window.requestIdleCallback;
                if (idle) {
                    idle(()=>mountNow(entry, moduleId, el, props));
                } else {
                    setTimeout(()=>mountNow(entry, moduleId, el, props), 1);
                }
                return;
            }
    }
}
export function scan(root) {
    for (const el of Array.from(root.querySelectorAll("sf-i:not([data-sf-mounted])"))){
        const moduleId = el.getAttribute("data-sf-module");
        if (!moduleId) continue;
        if (awaitingAnAncestor(el)) continue;
        if (el.parentElement?.closest("sf-s[data-sf-mode]")?.getAttribute("data-sf-mode") === "server") {
            el.setAttribute("data-sf-mounted", "");
            mountServer(el, moduleId, propsFor(root, el.id));
            continue;
        }
        const entry = islands.get(moduleId);
        if (!entry) {
            missing.add(moduleId);
            arm();
            continue;
        }
        el.setAttribute("data-sf-mounted", "");
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
