import { applyStyles, loadEntry, patchIsland, scan } from "./boot.js";
import { catalog, currentLocale, setCatalog, setLocale } from "./locale.js";
import { linesOf, parseRow } from "./reader.js";
import { escapeKey, nodeToHtml, regionSources, renderSegment, scriptSafeJson, subtreeAt } from "./render.js";
import { seed, transaction } from "./store.js";
let current = null;
const ids = {
    next: 0
};
function moduleOf(key) {
    const q = key.indexOf("?");
    return q === -1 ? key : key.slice(0, q);
}
function findRegion(key) {
    const open = `sf-g:${escapeKey(key)}`;
    const walker = document.createTreeWalker(document.documentElement.parentNode ?? document, NodeFilter.SHOW_COMMENT);
    let start = null;
    let depth = 0;
    for(let node = walker.nextNode(); node; node = walker.nextNode()){
        const text = node.data;
        if (!start) {
            if (text === open) start = node;
        } else if (text.startsWith("sf-g:")) {
            depth++;
        } else if (text === "/sf-g") {
            if (depth === 0) return {
                start,
                end: node
            };
            depth--;
        }
    }
    return null;
}
function replaceRegion(region, html) {
    const parent = region.start.parentNode;
    if (!(parent instanceof Element)) return false;
    const template = document.createElement("template");
    template.innerHTML = html;
    parent.insertBefore(template.content, region.start);
    const range = document.createRange();
    range.setStartBefore(region.start);
    range.setEndAfter(region.end);
    range.deleteContents();
    return true;
}
function fillSlot(slot, node, key) {
    const el = document.querySelector(`[data-sf-slot="${slot}"]`);
    if (!el) return;
    const template = document.createElement("template");
    const html = nodeToHtml(node, ids);
    template.innerHTML = key === null ? html : `<!--sf-g:${escapeKey(key)}-->${html}<!--/sf-g-->`;
    el.replaceWith(template.content);
}
function keyOfSlot(seg, slot) {
    if (seg.s === slot) return seg.k;
    for (const child of seg.c){
        const found = keyOfSlot(child, slot);
        if (found !== null) return found;
    }
    return null;
}
function pendingOf(node, slot) {
    if (node.kind === "pending") return node.slot === slot ? node : null;
    if (node.kind === "seq" || node.kind === "client") {
        for (const child of node.children){
            const found = pendingOf(child, slot);
            if (found) return found;
        }
    }
    return null;
}
const fallbacks = new WeakMap();
function removeChild(old) {
    const region = findRegion(old.k);
    if (region) {
        const parent = region.start.parentNode;
        let node = region.start;
        while(node){
            const next = node.nextSibling;
            node.parentNode?.removeChild(node);
            if (node === region.end) break;
            node = next;
        }
        if (parent instanceof Element && parent.hasAttribute("data-sf-name")) parent.innerHTML = fallbacks.get(parent) ?? "";
        return true;
    }
    if (old.s === undefined) return false;
    const el = document.querySelector(`[data-sf-slot="${old.s}"]`);
    if (!el) return false;
    el.remove();
    return true;
}
function namedSlotOf(region, name) {
    const island = islandOf(region);
    if (!island) return null;
    for (const slot of Array.from(island.el.querySelectorAll(`sf-s[data-sf-name="${name}"]`))){
        if (slot.parentElement?.closest("sf-i") === island.el) return slot;
    }
    return null;
}
function replaceChild(old, html) {
    const region = findRegion(old.k);
    if (region) return replaceRegion(region, html);
    if (old.s === undefined) return false;
    const el = document.querySelector(`[data-sf-slot="${old.s}"]`);
    if (!el) return false;
    const template = document.createElement("template");
    template.innerHTML = html;
    el.replaceWith(template.content);
    return true;
}
function islandOf(region) {
    for(let n = region.start.nextSibling; n && n !== region.end; n = n.nextSibling){
        if (n instanceof Element && n.tagName === "SF-I") {
            const next = n.nextSibling;
            const script = next instanceof Element && next.tagName === "SCRIPT" && next.getAttribute("data-sf-props") === n.id ? next : null;
            return {
                el: n,
                script
            };
        }
    }
    return null;
}
function patchProps(region, node) {
    if (node.kind !== "client") return;
    const island = islandOf(region);
    if (!island) return;
    const json = scriptSafeJson(node.props);
    if (island.script?.textContent === json) return;
    if (island.script) island.script.textContent = json;
    void patchIsland(island.el, node.props, regionSources(node, ids));
}
function diff(oldSeg, newSeg, newNode, force) {
    const swap = ()=>replaceChild(oldSeg, renderSegment(newNode, newSeg, ids));
    const paired = moduleOf(oldSeg.k) === moduleOf(newSeg.k);
    const same = paired && oldSeg.d !== undefined && oldSeg.d === newSeg.d;
    let key = oldSeg.k;
    if (oldSeg.k !== newSeg.k) {
        if (!same) {
            if (replaceChild(oldSeg, renderSegment(newNode, newSeg, ids))) return true;
            if (!paired) return false;
        }
        const region = findRegion(oldSeg.k);
        if (!region) return false;
        region.start.data = `sf-g:${escapeKey(newSeg.k)}`;
        key = newSeg.k;
    }
    const named = newSeg.c.every((c)=>c.n !== undefined) && oldSeg.c.every((c)=>c.n !== undefined);
    if (!named && oldSeg.c.length !== newSeg.c.length) return swap();
    if (newNode.kind === "client") {
        if (!same) {
            const region = findRegion(key);
            if (region) patchProps(region, newNode);
        }
    } else if (newSeg.c.length === 0 && (oldSeg.d !== undefined && newSeg.d !== undefined ? !same : force)) {
        return swap();
    }
    const keep = newSeg.keep ?? [];
    const carried = [];
    if (named) {
        for (const oldChild of oldSeg.c){
            if (newSeg.c.some((c)=>c.n === oldChild.n)) continue;
            if (keep.includes(oldChild.n ?? "")) {
                carried.push(oldChild);
                continue;
            }
            if (!removeChild(oldChild)) return false;
        }
    }
    for(let i = 0; i < newSeg.c.length; i++){
        const newChild = newSeg.c[i];
        const oldChild = named ? oldSeg.c.find((c)=>c.n === newChild.n) : oldSeg.c[i];
        if (!oldChild) {
            const region = findRegion(newSeg.k);
            const slot = region && newChild.n !== undefined ? namedSlotOf(region, newChild.n) : null;
            if (!slot) return false;
            if (!fallbacks.has(slot)) fallbacks.set(slot, slot.innerHTML);
            if (newChild.s !== undefined) {
                const pending = pendingOf(newNode, newChild.s);
                if (!pending) return false;
                slot.innerHTML = nodeToHtml(pending, ids);
            } else {
                slot.innerHTML = renderSegment(subtreeAt(newNode, newChild.p ?? []), newChild, ids);
            }
            continue;
        }
        if (newChild.s !== undefined) {
            const pending = pendingOf(newNode, newChild.s);
            if (!pending || !replaceChild(oldChild, nodeToHtml(pending, ids))) return false;
            continue;
        }
        if (!diff(oldChild, newChild, subtreeAt(newNode, newChild.p ?? []), force)) return false;
    }
    newSeg.c.push(...carried);
    return true;
}
function interceptSlot(seg) {
    if (seg.keep?.includes("content")) {
        return seg.c.find((c)=>c.n !== undefined && !seg.keep?.includes(c.n))?.n ?? null;
    }
    for (const child of seg.c){
        const found = interceptSlot(child);
        if (found !== null) return found;
    }
    return null;
}
let openSlot = null;
let currentPath = "";
let documentPath = "";
export function applyHead(head) {
    if (head.title !== undefined) document.title = head.title;
    if (head.description !== undefined) {
        let meta = document.head.querySelector('meta[name="description"]');
        if (!meta) {
            meta = document.createElement("meta");
            meta.setAttribute("name", "description");
            document.head.appendChild(meta);
        }
        meta.setAttribute("content", head.description);
    }
}
async function eagerOf(rows) {
    let tree = null;
    const eager = {
        heads: [],
        seeds: [],
        locale: null,
        catalog: null,
        entry: null,
        styles: []
    };
    for(;;){
        const { done, value: line } = await rows.next();
        if (done) return null;
        const row = parseRow(line);
        switch(row.tag){
            case "V":
                break;
            case "N":
                tree = row.tree;
                break;
            case "H":
                eager.heads.push(row.head);
                break;
            case "T":
                eager.seeds.push(row.seed);
                break;
            case "L":
                eager.locale = row.locale;
                break;
            case "E":
                eager.entry = row.entry;
                break;
            case "C":
                eager.styles = row.styles;
                break;
            case "D":
                eager.catalog = row.catalog;
                break;
            case "G":
                return tree === null ? null : {
                    ...eager,
                    tree,
                    segments: row.segments
                };
            case "S":
                return null;
        }
    }
}
function applyEager(eager, force) {
    if (!current) return false;
    transaction(()=>{
        for (const values of eager.seeds)seed(values);
    });
    if (!diff(current, eager.segments, eager.tree, force)) return false;
    current = eager.segments;
    openSlot = interceptSlot(eager.segments);
    for (const head of eager.heads)applyHead(head);
    if (eager.locale !== null) {
        if (eager.catalog !== null) setCatalog(eager.locale, eager.catalog);
        setLocale(eager.locale);
    }
    if (eager.entry !== null) loadEntry(eager.entry);
    scan(document);
    watchLinks(document);
    return true;
}
let generation = 0;
async function drain(rows, segments, gen) {
    try {
        for await (const line of rows){
            if (gen !== generation) return;
            const row = parseRow(line);
            if (row.tag === "S") {
                fillSlot(row.slot, row.node, keyOfSlot(segments, row.slot));
                scan(document);
                watchLinks(document);
            } else if (row.tag === "H") {
                applyHead(row.head);
            } else if (row.tag === "T") {
                seed(row.seed);
            }
        }
    } catch (err) {
        console.warn("sf: a streamed payload stopped applying", err);
    }
}
class Feed {
    lines = [];
    done = false;
    at = 0;
    ok;
    settle = ()=>{};
    waiters = [];
    constructor(){
        this.ok = new Promise((resolve)=>{
            this.settle = resolve;
        });
    }
    open(ok) {
        this.settle(ok);
    }
    push(line) {
        this.lines.push(line);
        this.wake();
    }
    finish() {
        this.done = true;
        this.at = performance.now();
        this.settle(false);
        this.wake();
    }
    wake() {
        const waiting = this.waiters;
        this.waiters = [];
        for (const wake of waiting)wake();
    }
    async whole() {
        while(!this.done){
            await new Promise((resolve)=>this.waiters.push(resolve));
        }
    }
    async *read() {
        for(let i = 0;; i++){
            while(i >= this.lines.length){
                if (this.done) return;
                await new Promise((resolve)=>this.waiters.push(resolve));
            }
            yield this.lines[i];
        }
    }
}
function fetchFeed(url, headers) {
    const feed = new Feed();
    void (async ()=>{
        try {
            const res = await fetch(payloadUrl(url), {
                headers
            });
            feed.open(res.ok);
            if (!res.ok) return;
            for await (const line of linesOf(res))feed.push(line);
        } catch  {} finally{
            feed.finish();
        }
    })();
    return feed;
}
const cache = new Map();
let cacheMs = 30_000;
function fresh(feed) {
    return !feed.done || performance.now() - feed.at < cacheMs;
}
function askFor(options) {
    if (options.full) return {
        from: null,
        into: null
    };
    if (options.into) return {
        from: null,
        into: options.into
    };
    return {
        from: currentPath,
        into: null
    };
}
function askOf(anchor) {
    return {
        full: anchor.hasAttribute("data-sf-full"),
        into: anchor.getAttribute("data-sf-into") ?? undefined
    };
}
function headersOf(ask) {
    const headers = {};
    if (ask.from !== null) headers["x-sf-from"] = ask.from;
    if (ask.into !== null) headers["x-sf-into"] = ask.into;
    const held = currentLocale();
    if (held && catalog(held) !== null) headers["x-sf-catalog"] = held;
    return headers;
}
function payloadUrl(url) {
    return `${url.pathname}${url.search}${url.search ? "&" : "?"}__payload`;
}
function cacheKey(url, ask) {
    return `${ask.from ?? ""}|${ask.into ?? ""}|${url.pathname}${url.search}`;
}
function fetchPayload(url, ask) {
    const key = cacheKey(url, ask);
    const feed = fetchFeed(url, headersOf(ask));
    cache.set(key, feed);
    void feed.ok.then((ok)=>{
        if (!ok && cache.get(key) === feed) cache.delete(key);
    });
    return feed;
}
function payloadFor(url, ask) {
    const held = cache.get(cacheKey(url, ask));
    if (held && fresh(held)) return held;
    return fetchPayload(url, ask);
}
let fallbackPrefetch = "hover";
function timingOf(anchor) {
    const own = anchor.getAttribute("data-sf-prefetch");
    return own === "hover" || own === "viewport" || own === "none" ? own : fallbackPrefetch;
}
let watched = new WeakSet();
let viewport = null;
function resetViewport() {
    viewport?.disconnect();
    viewport = null;
    watched = new WeakSet();
}
function watchLinks(root) {
    if (typeof IntersectionObserver !== "function") return;
    viewport ??= new IntersectionObserver((entries)=>{
        for (const entry of entries){
            if (!entry.isIntersecting) continue;
            viewport?.unobserve(entry.target);
            void prefetch(entry.target.getAttribute("href") ?? "", askOf(entry.target));
        }
    });
    for (const anchor of Array.from(root.querySelectorAll("a[href]"))){
        if (watched.has(anchor) || timingOf(anchor) !== "viewport") continue;
        watched.add(anchor);
        viewport.observe(anchor);
    }
}
export async function prefetch(href, options = {}) {
    const url = new URL(href, window.location.href);
    if (url.origin !== window.location.origin) return;
    await payloadFor(url, askFor(options)).whole();
}
export function clearRouterCache() {
    cache.clear();
}
export async function refresh() {
    const bail = ()=>window.location.reload();
    if (!current) return bail();
    cache.clear();
    const gen = ++generation;
    const feed = fetchFeed(new URL(window.location.href), headersOf({
        from: null,
        into: openSlot
    }));
    if (!await feed.ok) return bail();
    const rows = feed.read();
    const eager = await eagerOf(rows).catch(()=>null);
    if (gen !== generation) return;
    if (eager) await applyStyles(eager.styles);
    if (gen !== generation) return;
    if (!eager || !patch(eager, true)) return bail();
    await drain(rows, eager.segments, gen);
}
function patch(eager, force) {
    try {
        return applyEager(eager, force);
    } catch  {
        return false;
    }
}
export async function navigate(href, push = true, options = {}) {
    const url = new URL(href, window.location.href);
    const gen = ++generation;
    const feed = payloadFor(url, askFor(options));
    if (!await feed.ok) {
        if (gen === generation) window.location.assign(href);
        return;
    }
    const rows = feed.read();
    const eager = await eagerOf(rows).catch(()=>null);
    if (gen !== generation) return;
    if (eager) await applyStyles(eager.styles);
    if (gen !== generation) return;
    if (!eager || !patch(eager, false)) {
        window.location.assign(href);
        return;
    }
    if (push) history.pushState(null, "", href);
    currentPath = `${url.pathname}${url.search}`;
    if (openSlot === null) {
        documentPath = currentPath;
        window.scrollTo(0, 0);
    }
    await drain(rows, eager.segments, gen);
}
export function currentDocumentPath() {
    return documentPath;
}
export function localePath(to, from) {
    const path = from ?? documentPath ?? "";
    const at = path || (typeof window === "undefined" ? "/" : `${window.location.pathname}${window.location.search}`);
    const cut = at.indexOf("?");
    const search = cut === -1 ? "" : at.slice(cut);
    let rest = cut === -1 ? at : at.slice(0, cut);
    const tag = currentLocale();
    if (tag) {
        if (rest === `/${tag}`) rest = "";
        else if (rest.startsWith(`/${tag}/`)) rest = rest.slice(tag.length + 1);
    }
    if (rest === "/") rest = "";
    return `/${to}${rest}${search}`;
}
function linkOf(target) {
    const anchor = target?.closest?.("a[href]") ?? null;
    if (!anchor || anchor.hasAttribute("data-sf-native")) return null;
    return anchor;
}
export function enableNavigation(options = {}) {
    const g = globalThis;
    g.__sf = Object.assign(g.__sf ?? {}, {
        refresh
    });
    const sidecar = document.querySelector("script[data-sf-segments]");
    if (sidecar?.textContent) {
        current = JSON.parse(sidecar.textContent);
    }
    openSlot = null;
    currentPath = `${window.location.pathname}${window.location.search}`;
    documentPath = currentPath;
    if (options.cacheMs !== undefined) cacheMs = options.cacheMs;
    document.addEventListener("click", (event)=>{
        if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
            return;
        }
        const anchor = linkOf(event.target);
        if (!anchor) return;
        const href = anchor.getAttribute("href") ?? "";
        const url = new URL(href, window.location.href);
        if (url.origin !== window.location.origin) return;
        event.preventDefault();
        void navigate(url.pathname + url.search, true, askOf(anchor));
    });
    fallbackPrefetch = options.prefetch ?? "hover";
    resetViewport();
    const warm = (event)=>{
        const anchor = linkOf(event.target);
        if (!anchor || timingOf(anchor) !== "hover") return;
        void prefetch(anchor.getAttribute("href") ?? "", askOf(anchor));
    };
    document.addEventListener("mouseover", warm);
    document.addEventListener("focusin", warm);
    document.addEventListener("touchstart", warm, {
        passive: true
    });
    watchLinks(document);
    document.addEventListener("sf:fill", ()=>watchLinks(document));
    window.addEventListener("popstate", ()=>{
        void navigate(window.location.pathname + window.location.search, false);
    });
}
//# sourceMappingURL=navigator.js.map
