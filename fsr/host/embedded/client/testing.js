import { createRoot, hydrateRoot } from "react-dom/client";
import { boot, registeredIslands } from "./boot.js";
import { setLocale } from "./locale.js";
import { applyHead, clearRouterCache, enableNavigation } from "./navigator.js";
import { withHoisted } from "./react.js";
import { reset, seed } from "./store.js";
import { decodeValue, encodeValue } from "./values.js";
export { f64 } from "./values.js";
function sf() {
    const s = globalThis.__sf;
    if (!s) throw new Error("@snapfire/fsr-client/testing runs under `fsr test` only");
    return s;
}
const mocks = new Map();
export function ctx(mock = {}) {
    const methods = [];
    for (const [service, table] of Object.entries(mock.services ?? {})){
        for (const method of Object.keys(table))methods.push(`${service}.${method}`);
    }
    const natives = [];
    for (const [module, table] of Object.entries(mock.native ?? {})){
        for (const method of Object.keys(table))natives.push(`${module}.${method}`);
    }
    const spec = {
        session: encodeValue(mock.session ?? {}),
        params: mock.params ?? {},
        query: mock.query ?? {},
        input: mock.input === undefined ? null : encodeValue(mock.input),
        identity: mock.identity ?? null,
        locale: mock.locale ?? null,
        path: mock.path ?? null,
        methods,
        natives
    };
    const id = sf().ctx(JSON.stringify(spec));
    for (const [service, table] of Object.entries(mock.services ?? {})){
        for (const [method, fn] of Object.entries(table))mocks.set(`${id}:${service}.${method}`, fn);
    }
    for (const [module, table] of Object.entries(mock.native ?? {})){
        for (const [method, fn] of Object.entries(table))mocks.set(`${id}:native:${module}.${method}`, fn);
    }
    return {
        id,
        locale: sf().locale(id),
        get session () {
            return decodeValue(JSON.parse(sf().session(id)));
        },
        get trace () {
            return {
                calls: decodeValue(JSON.parse(sf().calls(id)))
            };
        }
    };
}
function callMock(key, args) {
    const fn = mocks.get(key);
    if (!fn) {
        console.error(`no mock for ${key}`);
        throw new Error(`no mock for ${key}`);
    }
    const result = fn(decodeValue(JSON.parse(args)));
    if (result !== null && typeof result === "object" && typeof result.then === "function") {
        throw new Error(`the mock for ${key} returned a promise; a mock answers synchronously`);
    }
    return JSON.stringify(encodeValue(result));
}
const cases = [];
export function test(name, body) {
    cases.push({
        name,
        body
    });
}
Object.assign(globalThis, {
    __sf_call: callMock,
    __sf_tests: ()=>cases.map((c)=>c.name),
    __sf_run: (i)=>{
        const run = Promise.resolve().then(()=>cases[i].body());
        run.catch(()=>{});
        return run;
    }
});
export class AssertionError extends Error {
    constructor(message){
        super(message);
        this.name = "AssertionError";
    }
}
export function show(value, depth = 0) {
    if (typeof value === "bigint") return `${value}n`;
    if (typeof value === "string") return JSON.stringify(value);
    if (value === null || typeof value !== "object") return String(value);
    if (typeof value.nodeType === "number") {
        const el = value;
        return `<${el.nodeName.toLowerCase()}${el.id ? `#${el.id}` : ""}${typeof el.className === "string" && el.className ? `.${el.className.split(" ").join(".")}` : ""}>`;
    }
    if (depth > 6) return "…";
    if (Array.isArray(value)) return `[${value.map((v)=>show(v, depth + 1)).join(", ")}]`;
    if (value instanceof Uint8Array) return `Uint8Array(${value.length})`;
    const entries = Object.entries(value).map(([k, v])=>`${JSON.stringify(k)}: ${show(v, depth + 1)}`);
    return `{ ${entries.join(", ")} }`;
}
export function equal(a, b) {
    if (Object.is(a, b)) return true;
    if (typeof a !== typeof b || a === null || b === null || typeof a !== "object") return false;
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i)=>equal(x, b[i]));
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every((k)=>Object.prototype.hasOwnProperty.call(b, k) && equal(a[k], b[k]));
}
export const assert = {
    ok (value, message) {
        if (!value) throw new AssertionError(message ?? `assert.ok: ${show(value)}`);
    },
    equal (actual, expected, message) {
        if (!equal(actual, expected)) throw new AssertionError(`${message ?? "assert.equal"}\n  actual:   ${show(actual)}\n  expected: ${show(expected)}`);
    },
    throws (run, match) {
        try {
            run();
        } catch (e) {
            matchError(e, match);
            return;
        }
        throw new AssertionError("assert.throws: nothing was thrown");
    },
    async rejects (run, match) {
        try {
            await (typeof run === "function" ? run() : run);
        } catch (e) {
            matchError(e, match);
            return;
        }
        throw new AssertionError("assert.rejects: the promise resolved");
    }
};
function matchError(e, match) {
    if (match === undefined) return;
    const text = e instanceof Error ? `${e.kind ?? ""} ${e.message}` : String(e);
    const hit = typeof match === "string" ? text.includes(match) : match.test(text);
    if (!hit) throw new AssertionError(`expected an error matching ${show(match instanceof RegExp ? String(match) : match)}, got ${show(text.trim())}`);
}
export function settle() {
    return sf().idle();
}
export function advance(ms) {
    return sf().advance(ms);
}
async function moduleOf(type) {
    for (const [id, entry] of registeredIslands()){
        const mod = await entry.loader();
        if (mod === type) return id;
    }
    return null;
}
export async function render(element, options = {}) {
    sf().use(options.ctx?.id ?? 0);
    setLocale(options.ctx?.locale ?? sf().locale(0));
    const container = document.createElement("div");
    document.body.appendChild(container);
    const module = options.hydrate === false ? null : await moduleOf(element.type);
    const rendered = module === null ? null : sf().render(module, JSON.stringify(encodeValue(element.props)));
    let root;
    let hydrated = null;
    if (rendered !== null) {
        const { html, hoisted } = JSON.parse(rendered);
        container.innerHTML = html;
        root = hydrateRoot(container, withHoisted(decodeValue(hoisted), element));
        hydrated = module;
    } else {
        root = createRoot(container);
        root.render(element);
    }
    await settle();
    return {
        container,
        root,
        hydrated,
        unmount () {
            root.unmount();
            container.remove();
        }
    };
}
export async function load(path, options = {}) {
    sf().use(options.ctx?.id ?? 0);
    let res = await fetch(path);
    for(let hops = 0; hops < 5 && res.status >= 300 && res.status < 400; hops++){
        const to = res.headers.get("location");
        if (!to) break;
        path = to;
        res = await fetch(path);
    }
    const html = await res.text();
    if (!/<!doctype/i.test(html.slice(0, 256))) throw new AssertionError(`load ${show(path)}: HTTP ${res.status}: ${html.trim()}`);
    sf().load(html, path);
    clearRouterCache();
    reset();
    const late = applyFills();
    boot();
    enableNavigation();
    for (const run of late)run();
    await settle();
    return {
        status: res.status,
        path
    };
}
function applyFills() {
    const late = [];
    for (const template of Array.from(document.querySelectorAll("template[data-sf-fill]"))){
        const id = template.getAttribute("data-sf-fill");
        const slot = document.querySelector(`[data-sf-slot="${id}"]`);
        const script = template.nextElementSibling;
        if (slot) slot.replaceWith(template.content);
        template.remove();
        if (script?.tagName !== "SCRIPT" || !script.textContent?.startsWith("__sfFill(")) continue;
        for (const call of script.textContent.split(";__sf").slice(1)){
            const open = call.indexOf("(");
            const body = call.slice(open + 1, call.lastIndexOf(")"));
            if (call.startsWith("Head(")) late.push(()=>applyHead(JSON.parse(body)));
            if (call.startsWith("Store(")) late.push(()=>seed(decodeValue(JSON.parse(body))));
        }
        script.remove();
    }
    return late;
}
function normalise(text) {
    return text.replace(/\s+/g, " ").trim();
}
function ownText(el) {
    let out = "";
    for (const node of Array.from(el.childNodes)){
        if (node.nodeType === 3) out += node.textContent ?? "";
    }
    return normalise(out);
}
function matches(text, matcher) {
    return typeof matcher === "string" ? text === matcher : matcher.test(text);
}
function all(root, pick) {
    return Array.from(root.querySelectorAll("*")).filter(pick);
}
function one(what, found) {
    if (found.length === 1) return found[0];
    if (found.length === 0) throw new AssertionError(`no element ${what}`);
    throw new AssertionError(`${found.length} elements ${what}: ${found.map((el)=>`<${el.tagName.toLowerCase()}>`).join(", ")}`);
}
export const screen = {
    getByText (matcher, root = document.body) {
        return one(`with text ${show(matcher instanceof RegExp ? String(matcher) : matcher)}`, screen.getAllByText(matcher, root));
    },
    queryByText (matcher, root = document.body) {
        const found = screen.getAllByText(matcher, root);
        return found.length === 0 ? null : found[0];
    },
    getAllByText (matcher, root = document.body) {
        return all(root, (el)=>matches(ownText(el), matcher));
    },
    getByLabelText (matcher, root = document.body) {
        const labelled = all(root, (el)=>matches(normalise(el.getAttribute("aria-label") ?? ""), matcher));
        const byLabel = Array.from(root.querySelectorAll("label")).filter((label)=>matches(ownText(label), matcher) || matches(normalise(label.textContent ?? ""), matcher)).flatMap((label)=>{
            const target = label.getAttribute("for");
            const control = target ? root.querySelector(`#${CSS.escape(target)}`) : label.querySelector("input, select, textarea, button");
            return control ? [
                control
            ] : [];
        });
        return one(`labelled ${show(matcher instanceof RegExp ? String(matcher) : matcher)}`, [
            ...labelled,
            ...byLabel
        ]);
    },
    getByPlaceholderText (matcher, root = document.body) {
        return one(`with placeholder ${show(String(matcher))}`, all(root, (el)=>matches(el.getAttribute("placeholder") ?? "", matcher)));
    },
    getByTestId (id, root = document.body) {
        return one(`with data-testid ${show(id)}`, all(root, (el)=>el.getAttribute("data-testid") === id));
    }
};
function setValue(el, value) {
    if (el.tagName === "SELECT") {
        const option = Array.from(el.querySelectorAll("option")).find((o)=>String(o.value) === value);
        if (!option) throw new AssertionError(`fireEvent.change: no option with value ${show(value)}`);
        option.selected = true;
        return;
    }
    const proto = Object.getPrototypeOf(el);
    const descriptor = Object.getOwnPropertyDescriptor(proto, "value");
    if (descriptor?.set) {
        descriptor.set.call(el, value);
    } else {
        el.value = value;
    }
}
export const fireEvent = {
    click (el) {
        el.dispatchEvent(new MouseEvent("click", {
            bubbles: true,
            cancelable: true
        }));
        return settle();
    },
    change (el, value) {
        setValue(el, value);
        el.dispatchEvent(new Event("input", {
            bubbles: true
        }));
        el.dispatchEvent(new Event("change", {
            bubbles: true
        }));
        return settle();
    },
    submit (el) {
        const form = el instanceof HTMLFormElement ? el : el.closest("form");
        if (!form) throw new AssertionError("fireEvent.submit: no form");
        form.dispatchEvent(new Event("submit", {
            bubbles: true,
            cancelable: true
        }));
        return settle();
    },
    keyDown (el, key) {
        el.dispatchEvent(new KeyboardEvent("keydown", {
            key,
            bubbles: true,
            cancelable: true
        }));
        return settle();
    }
};
//# sourceMappingURL=testing.js.map
