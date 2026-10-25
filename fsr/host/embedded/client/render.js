import { encodeValue } from "./values.js";
function escapeText(text) {
    return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function scriptSafeJson(value) {
    return JSON.stringify(encodeValue(value)).replace(/</g, "\\u003c");
}
function propsScript(node) {
    return node.encoded === undefined ? scriptSafeJson(node.props) : JSON.stringify(node.encoded).replace(/</g, "\\u003c");
}
export const PLACED_ATTR = "data-sf-placed";
export const PLACED_OPEN = "data-sf-l";
export const PLACED_END = "data-sf-l-end";
export const PLACED_ISLAND = "data-sf-island";
export const PLACED_TO = "data-sf-to";
export function nodeToHtml(node, ids) {
    switch(node.kind){
        case "text":
            return escapeText(node.text);
        case "raw":
            return node.html;
        case "seq":
            return node.children.map((c)=>nodeToHtml(c, ids)).join("");
        case "client":
            return clientHtml(node, ids, (children)=>children.map((c)=>nodeToHtml(c, ids)).join(""));
        case "pending":
            return `<div data-sf-slot="${node.slot}">${nodeToHtml(node.fallback, ids)}</div>`;
        case "placed":
            {
                const island = ids.island ? ` ${PLACED_ISLAND}="${escapeAttr(ids.island)}"` : "";
                const to = node.target.startsWith("#") ? ` ${PLACED_TO}="${escapeAttr(node.target.slice(1))}"` : "";
                return `<template ${PLACED_ATTR}="${escapeAttr(ids.segment ?? "")}"${island}${to}>${node.children.map((c)=>nodeToHtml(c, ids)).join("")}</template>`;
            }
        case "outlet":
            return "";
    }
}
function escapeAttr(value) {
    return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}
export function escapeKey(key) {
    return key.replace(/%/g, "%25").replace(/-/g, "%2D");
}
function subtreeAt(node, path) {
    let current = node;
    for (const idx of path){
        if (current.kind !== "seq" && current.kind !== "client") throw new Error("segment path walks through a node with no children");
        current = current.children[idx];
    }
    return current;
}
export function renderSegment(node, seg, ids) {
    const outer = ids.segment;
    ids.segment = seg.k;
    let out = `<!--sf-g:${escapeKey(seg.k)}-->`;
    const inner = seg.c.find((c)=>c.s === undefined && (c.p ?? []).length === 0);
    if (inner) {
        out += renderSegment(node, inner, ids);
    } else {
        const positioned = seg.c.filter((c)=>c.s === undefined && (c.p ?? []).length > 0).map((c)=>({
                path: c.p ?? [],
                seg: c
            }));
        out += renderPositioned(node, positioned, ids);
    }
    ids.segment = outer;
    return out + "<!--/sf-g-->";
}
function clientHtml(node, ids, inner) {
    const id = `sf-c${ids.next++}`;
    const outer = ids.island;
    const key = node.props[REGION_KEY];
    if (node.rendered) ids.island = typeof key === "string" ? key : id;
    const html = inner(node.children);
    ids.island = outer;
    return `<sf-i id="${id}" data-sf-module="${node.module}"${node.rendered ? ` ${SSR_ATTR}` : ""}>${html}</sf-i><script type="application/json" data-sf-props="${id}">${propsScript(node)}</script>`;
}
function renderPositioned(node, positioned, ids) {
    if (positioned.length === 0) return nodeToHtml(node, ids);
    const items = (children)=>children.map((child, idx)=>{
            const here = positioned.filter((c)=>c.path[0] === idx).map((c)=>({
                    path: c.path.slice(1),
                    seg: c.seg
                }));
            const exact = here.find((c)=>c.path.length === 0);
            return exact ? renderSegment(child, exact.seg, ids) : renderPositioned(child, here, ids);
        }).join("");
    if (node.kind === "seq") return items(node.children);
    if (node.kind === "client") return clientHtml(node, ids, items);
    return nodeToHtml(node, ids);
}
export const REGION_KEY = "$k";
export const SSR_ATTR = "data-sf-ssr";
export const CHILDREN_ATTR = "data-sf-children";
export function childrenOf(node, ids) {
    if (node.kind !== "client") return null;
    const html = node.children.map((c)=>nodeToHtml(c, ids)).join("");
    if (!html.includes(CHILDREN_ATTR)) return null;
    const template = document.createElement("template");
    template.innerHTML = html;
    const region = Array.from(template.content.querySelectorAll(`sf-s[${CHILDREN_ATTR}]`)).find((found)=>!found.parentElement?.closest("sf-i"));
    return region ? region.innerHTML : null;
}
export const MARKUP_ATTR = "data-sf-m";
export function markupOf(node, ids) {
    if (node.kind !== "client") return null;
    const html = node.children.map((c)=>nodeToHtml(c, ids)).join("");
    if (!html.includes(MARKUP_ATTR)) return null;
    const template = document.createElement("template");
    template.innerHTML = html;
    const out = {};
    for (const region of Array.from(template.content.querySelectorAll(`sf-s[${MARKUP_ATTR}], template[${MARKUP_ATTR}]`))){
        if (region.parentElement?.closest("sf-i")) continue;
        const n = region.getAttribute(MARKUP_ATTR) ?? "";
        if (!(n in out)) out[n] = region.innerHTML;
    }
    return out;
}
export function regionSources(node, ids) {
    const out = new Map();
    const walk = (n)=>{
        switch(n.kind){
            case "seq":
                n.children.forEach(walk);
                return;
            case "client":
                {
                    const key = n.props[REGION_KEY];
                    if (typeof key === "string") out.set(key, {
                        props: n.props,
                        encoded: n.encoded,
                        html: nodeToHtml(n, ids),
                        nested: regionSources(n, ids),
                        children: childrenOf(n, ids),
                        markup: markupOf(n, ids)
                    });
                    return;
                }
            case "pending":
                walk(n.fallback);
                return;
            default:
                return;
        }
    };
    if (node.kind === "client") {
        node.children.forEach(walk);
        return out;
    }
    walk(node);
    return out;
}
export { propsScript, scriptSafeJson };
export { subtreeAt };
//# sourceMappingURL=render.js.map
