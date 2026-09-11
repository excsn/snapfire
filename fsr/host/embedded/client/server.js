import { decodeValue, encodeValue } from "./values.js";
const STATE_PROP = "$s";
const islands = new WeakMap();
export function isServerIsland(el) {
    return islands.has(el);
}
export function mountServer(el, module, props) {
    const { [STATE_PROP]: state, ...own } = props;
    const island = {
        module,
        props: own,
        state: state ?? {},
        pending: false,
        listening: new Set()
    };
    islands.set(el, island);
    listen(el, island);
}
export async function patchServer(el, props) {
    const island = islands.get(el);
    if (!island) return false;
    const { [STATE_PROP]: state, ...own } = props;
    island.props = own;
    if (state !== undefined) island.state = state;
    await step(el, island, null, null);
    return true;
}
function eventsBound(el) {
    const types = new Set();
    for (const bound of Array.from(el.querySelectorAll("[data-sf-on]"))){
        for (const pair of (bound.getAttribute("data-sf-on") ?? "").split(" ")){
            const type = pair.split(":")[0];
            if (type) types.add(type);
        }
    }
    return types;
}
function listen(el, island) {
    for (const type of eventsBound(el)){
        if (island.listening.has(type)) continue;
        island.listening.add(type);
        el.addEventListener(type, (event)=>void fire(el, island, type, event));
    }
}
function handlerFor(el, target, type) {
    if (!(target instanceof Element)) return null;
    const bound = target.closest("[data-sf-on]");
    if (!bound || !el.contains(bound)) return null;
    for (const pair of (bound.getAttribute("data-sf-on") ?? "").split(" ")){
        const [event, index] = pair.split(":");
        if (event === type && index !== undefined) return Number(index);
    }
    return null;
}
async function fire(el, island, type, event) {
    const handler = handlerFor(el, event.target, type);
    if (handler === null) return;
    if (type === "submit") event.preventDefault();
    if (island.pending) return;
    const target = event.target;
    const detail = {
        target: {
            value: target?.value ?? null,
            checked: target?.checked ?? null,
            name: target?.name ?? null
        },
        key: event.key ?? null
    };
    await step(el, island, handler, detail);
}
async function step(el, island, handler, event) {
    island.pending = true;
    el.setAttribute("data-sf-pending", "");
    try {
        const headers = {
            "content-type": "application/json"
        };
        if (typeof window !== "undefined") headers["x-sf-from"] = `${window.location.pathname}${window.location.search}`;
        const body = JSON.stringify(encodeValue({
            props: island.props,
            state: island.state,
            handler,
            event
        }));
        const res = await fetch(`/_sf/island/${encodeURIComponent(island.module)}`, {
            method: "POST",
            headers,
            body
        });
        const text = await res.text();
        if (!res.ok) {
            console.warn(`sf: island ${island.module} step failed with ${res.status}: ${text}`);
            return;
        }
        const answer = decodeValue(JSON.parse(text));
        island.state = answer.state;
        morph(el, answer.html);
        listen(el, island);
    } finally{
        island.pending = false;
        el.removeAttribute("data-sf-pending");
    }
}
export function morph(el, html) {
    const template = document.createElement("template");
    template.innerHTML = html;
    morphChildren(el, template.content);
}
function keyOf(node) {
    return node instanceof Element ? node.getAttribute("data-sf-key") : null;
}
function alike(a, b) {
    if (a.nodeType !== b.nodeType) return false;
    if (a instanceof Element && b instanceof Element && a.tagName !== b.tagName) return false;
    return keyOf(a) === keyOf(b);
}
function morphChildren(from, to) {
    const old = Array.from(from.childNodes);
    let i = 0;
    for (const next of Array.from(to.childNodes)){
        const current = old[i];
        if (current && alike(current, next)) {
            morphNode(current, next);
            i += 1;
            continue;
        }
        const key = keyOf(next);
        const moved = key === null ? undefined : old.slice(i).find((candidate)=>keyOf(candidate) === key);
        if (moved) {
            from.insertBefore(moved, current ?? null);
            old.splice(old.indexOf(moved), 1);
            old.splice(i, 0, moved);
            morphNode(moved, next);
        } else {
            const imported = document.importNode(next, true);
            from.insertBefore(imported, current ?? null);
            old.splice(i, 0, imported);
        }
        i += 1;
    }
    for (const stale of old.slice(i))stale.remove();
}
function morphNode(current, next) {
    if (current.nodeType === Node.TEXT_NODE || current.nodeType === Node.COMMENT_NODE) {
        if (current.nodeValue !== next.nodeValue) current.nodeValue = next.nodeValue;
        return;
    }
    if (!(current instanceof Element) || !(next instanceof Element)) return;
    morphAttributes(current, next);
    if (current.tagName === "SF-I") return;
    morphChildren(current, next);
}
function morphAttributes(current, next) {
    for (const attr of Array.from(current.attributes)){
        if (!next.hasAttribute(attr.name)) current.removeAttribute(attr.name);
    }
    for (const attr of Array.from(next.attributes)){
        if (current.getAttribute(attr.name) !== attr.value) current.setAttribute(attr.name, attr.value);
    }
    const focused = typeof document !== "undefined" && document.activeElement === current;
    if (!focused && "value" in current && "value" in next) {
        const control = current;
        const wanted = next;
        if (control.value !== wanted.value) control.value = wanted.value;
        if ("checked" in wanted && control.checked !== wanted.checked) control.checked = wanted.checked;
    }
}
//# sourceMappingURL=server.js.map
