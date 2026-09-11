import { encodeValue } from "./values.js";
function escapeText(text) {
    return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function scriptSafeJson(value) {
    return JSON.stringify(encodeValue(value)).replace(/</g, "\\u003c");
}
export function nodeToHtml(node, ids) {
    switch(node.kind){
        case "text":
            return escapeText(node.text);
        case "raw":
            return node.html;
        case "seq":
            return node.children.map((c)=>nodeToHtml(c, ids)).join("");
        case "client":
            {
                const id = `sf-c${ids.next++}`;
                const inner = node.ssr ? nodeToHtml(node.ssr, ids) : node.children.map((c)=>nodeToHtml(c, ids)).join("");
                const props = scriptSafeJson(node.props);
                return `<sf-i id="${id}" data-sf-module="${node.module}">${inner}</sf-i>` + `<script type="application/json" data-sf-props="${id}">${props}</script>`;
            }
        case "pending":
            return `<div data-sf-slot="${node.slot}">${nodeToHtml(node.fallback, ids)}</div>`;
    }
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
    return out + "<!--/sf-g-->";
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
    if (node.kind === "client" && !node.ssr) {
        const id = `sf-c${ids.next++}`;
        return `<sf-i id="${id}" data-sf-module="${node.module}">${items(node.children)}</sf-i><script type="application/json" data-sf-props="${id}">${scriptSafeJson(node.props)}</script>`;
    }
    return nodeToHtml(node, ids);
}
export const REGION_KEY = "$k";
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
                        html: nodeToHtml(n, ids),
                        nested: regionSources(n, ids)
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
        if (node.ssr) walk(node.ssr);
        else node.children.forEach(walk);
        return out;
    }
    walk(node);
    return out;
}
export { scriptSafeJson };
export { subtreeAt };
//# sourceMappingURL=render.js.map
