import { boot, discard, registeredIslands } from "./boot.js";
import { advance, AssertionError, settle, sf, show } from "./harness.js";
import { clearAllMocks, fn, isMockFunction, resetAllMocks, resetAssertions, restoreAllMocks, SETTLED, spyOn, verifyAssertions } from "./expect.js";
import { setLocale } from "./locale.js";
import { applyHead, clearRouterCache, enableNavigation } from "./navigator.js";
import { prettyDOM, waitFor, within } from "./queries.js";
import { reset, seed } from "./store.js";
import { decodeValue, encodeValue } from "./values.js";
export { f64 } from "./values.js";
export { advance, AssertionError, settle, show } from "./harness.js";
export { clearAllMocks, expect, fn, isMockFunction, resetAllMocks, restoreAllMocks, spyOn } from "./expect.js";
export { configure, getDefaultNormalizer, logRoles, prettyDOM, screen, TestingLibraryElementError, waitFor, waitForElementToBeRemoved, within } from "./queries.js";
export { createEvent, fireEvent, userEvent } from "./events.js";
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
        for (const [method, answer] of Object.entries(table))mocks.set(`${id}:${service}.${method}`, answer);
    }
    for (const [module, table] of Object.entries(mock.native ?? {})){
        for (const [method, answer] of Object.entries(table))mocks.set(`${id}:native:${module}.${method}`, answer);
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
    const answer = mocks.get(key);
    if (!answer) {
        console.error(`no mock for ${key}`);
        throw new Error(`no mock for ${key}`);
    }
    const result = answer(decodeValue(JSON.parse(args)));
    if (result !== null && typeof result === "object" && typeof result.then === "function") {
        const settled = result[SETTLED];
        if (!settled) throw new Error(`the mock for ${key} returned a promise; a mock answers synchronously or through a mock function's mockResolvedValue`);
        if (!settled.ok) throw settled.value;
        return JSON.stringify(encodeValue(settled.value));
    }
    return JSON.stringify(encodeValue(result));
}
function block(name, parent, mode) {
    return {
        name,
        parent,
        mode,
        beforeAll: [],
        afterAll: [],
        beforeEach: [],
        afterEach: [],
        entered: false,
        failure: null
    };
}
const root = block("", null, "run");
let current = root;
const cases = [];
const entered = [];
function chain(from) {
    const out = [];
    for(let at = from; at; at = at.parent)out.unshift(at);
    return out;
}
function fullName(c) {
    return [
        ...chain(c.block).slice(1).map((b)=>b.name),
        c.name
    ].join(" > ");
}
function anyOnly() {
    return cases.some((c)=>c.mode === "only" || chain(c.block).some((b)=>b.mode === "only"));
}
function modeOf(c, only) {
    if (c.mode === "todo") return "todo";
    const blocks = chain(c.block);
    if (c.mode === "skip" || blocks.some((b)=>b.mode === "skip")) return "skip";
    if (only && c.mode !== "only" && !blocks.some((b)=>b.mode === "only")) return "skip";
    return "run";
}
function invoke(body) {
    if (body.length > 0) {
        return new Promise((resolve, reject)=>{
            const done = (error)=>error === undefined || error === null ? resolve(undefined) : reject(error);
            done.fail = (error)=>reject(error ?? new Error("done.fail()"));
            try {
                body(done);
            } catch (e) {
                reject(e);
            }
        });
    }
    return Promise.resolve().then(()=>body());
}
async function runCase(c) {
    const blocks = chain(c.block);
    for (const b of blocks){
        if (b.entered) continue;
        b.entered = true;
        entered.push(b);
        try {
            for (const hook of b.beforeAll)await invoke(hook);
        } catch (error) {
            b.failure = {
                error
            };
        }
    }
    const failed = blocks.find((b)=>b.failure);
    if (failed?.failure) throw failed.failure.error;
    resetAssertions();
    let failure = null;
    try {
        for (const b of blocks)for (const hook of b.beforeEach)await invoke(hook);
        if (c.body) await invoke(c.body);
        verifyAssertions();
    } catch (error) {
        failure = {
            error
        };
    }
    for (const b of [
        ...blocks
    ].reverse()){
        for (const hook of b.afterEach){
            try {
                await invoke(hook);
            } catch (error) {
                failure ??= {
                    error
                };
            }
        }
    }
    if (failure) throw failure.error;
}
async function finish() {
    const failures = [];
    for (const b of [
        ...entered
    ].reverse()){
        for (const hook of b.afterAll){
            try {
                await invoke(hook);
            } catch (error) {
                failures.push(`${b.name || "the file"}: ${error instanceof Error ? error.message : show(error)}`);
            }
        }
    }
    if (failures.length > 0) throw new AssertionError(`afterAll failed\n${failures.join("\n")}`);
}
Object.assign(globalThis, {
    __sf_call: callMock,
    __sf_tests: ()=>cases.map(fullName),
    __sf_modes: ()=>{
        const only = anyOnly();
        return cases.map((c)=>modeOf(c, only));
    },
    __sf_run: (i)=>{
        const run = runCase(cases[i]);
        run.catch(()=>{});
        return run;
    },
    __sf_finish: ()=>{
        const run = finish();
        run.catch(()=>{});
        return run;
    }
});
function rowsOf(table, values) {
    if (Array.isArray(table) && Object.prototype.hasOwnProperty.call(table, "raw")) {
        const headings = String(table[0]).split("|").map((h)=>h.trim()).filter(Boolean);
        const rows = [];
        for(let i = 0; i < values.length; i += headings.length){
            const row = {};
            headings.forEach((heading, j)=>row[heading] = values[i + j]);
            rows.push([
                row
            ]);
        }
        return rows;
    }
    if (!Array.isArray(table)) throw new Error("each takes an array of rows or a template table");
    return table.map((row)=>Array.isArray(row) ? row : [
            row
        ]);
}
function titled(name, args, index) {
    let next = 0;
    let out = name.replace(/%([sdifjoOp#$%])/g, (whole, flag)=>{
        if (flag === "%") return "%";
        if (flag === "#") return String(index);
        if (flag === "$") return String(index + 1);
        if (next >= args.length) return whole;
        const value = args[next++];
        switch(flag){
            case "s":
                return typeof value === "string" ? value : show(value);
            case "d":
            case "i":
                return typeof value === "bigint" ? String(value) : String(Math.trunc(Number(value)));
            case "f":
                return String(Number(value));
            case "j":
                return JSON.stringify(value);
            default:
                return show(value);
        }
    });
    const row = args.length === 1 && typeof args[0] === "object" && args[0] !== null ? args[0] : null;
    if (row) {
        out = out.replace(/\$([A-Za-z_][\w.]*)/g, (whole, path)=>{
            let at = row;
            for (const key of path.split(".")){
                if (at === null || typeof at !== "object" || !(key in at)) return whole;
                at = at[key];
            }
            return typeof at === "string" ? at : show(at);
        });
    }
    return out;
}
function eachOf(register) {
    return (table, ...values)=>(name, body)=>{
            rowsOf(table, values).forEach((args, index)=>register(titled(name, args, index), ()=>body(...args)));
        };
}
function registerAs(mode) {
    return (name, body)=>{
        cases.push({
            name,
            block: current,
            body: body ?? null,
            mode: body ? mode : "todo"
        });
    };
}
function inverted(body) {
    return async ()=>{
        let threw = false;
        try {
            await invoke(body);
        } catch  {
            threw = true;
        }
        if (!threw) throw new AssertionError("test.fails: the test passed");
    };
}
export const test = Object.assign(registerAs("run"), {
    only: Object.assign(registerAs("only"), {
        each: eachOf(registerAs("only"))
    }),
    skip: Object.assign(registerAs("skip"), {
        each: eachOf(registerAs("skip"))
    }),
    todo: (name)=>registerAs("todo")(name),
    each: eachOf(registerAs("run")),
    skipIf: (condition)=>condition ? registerAs("skip") : registerAs("run"),
    runIf: (condition)=>condition ? registerAs("run") : registerAs("skip"),
    fails: (name, body)=>registerAs("run")(name, body ? inverted(body) : undefined),
    concurrent: registerAs("run")
});
export const it = test;
export const xit = test.skip;
export const xtest = test.skip;
export const fit = test.only;
function groupAs(mode) {
    return (name, body)=>{
        const outer = current;
        current = block(name, outer, mode);
        try {
            const returned = body();
            if (returned && typeof returned.then === "function") throw new Error(`describe(${JSON.stringify(name)}): a describe body runs at once and returns nothing; put what it awaits in a test or a hook`);
        } finally{
            current = outer;
        }
    };
}
function describeEachOf(group) {
    return (table, ...values)=>(name, body)=>{
            rowsOf(table, values).forEach((args, index)=>group(titled(name, args, index), ()=>body(...args)));
        };
}
export const describe = Object.assign(groupAs("run"), {
    only: Object.assign(groupAs("only"), {
        each: describeEachOf(groupAs("only"))
    }),
    skip: Object.assign(groupAs("skip"), {
        each: describeEachOf(groupAs("skip"))
    }),
    each: describeEachOf(groupAs("run")),
    skipIf: (condition)=>condition ? groupAs("skip") : groupAs("run"),
    runIf: (condition)=>condition ? groupAs("run") : groupAs("skip"),
    concurrent: groupAs("run")
});
export const xdescribe = describe.skip;
export const fdescribe = describe.only;
export function beforeAll(body, _timeout) {
    current.beforeAll.push(body);
}
export function afterAll(body, _timeout) {
    current.afterAll.push(body);
}
export function beforeEach(body, _timeout) {
    current.beforeEach.push(body);
}
export function afterEach(body, _timeout) {
    current.afterEach.push(body);
}
export const vi = {
    fn,
    spyOn,
    isMockFunction,
    mocked: (value)=>value,
    clearAllMocks,
    resetAllMocks,
    restoreAllMocks,
    useFakeTimers () {
        return vi;
    },
    useRealTimers () {
        return vi;
    },
    isFakeTimers: ()=>true,
    advanceTimersByTime: (ms)=>advance(ms),
    advanceTimersByTimeAsync: (ms)=>advance(ms),
    waitFor: (callback, options)=>waitFor(callback, options)
};
export const jest = vi;
export function equal(a, b) {
    if (Object.is(a, b)) return true;
    const [big, num] = typeof a === "bigint" ? [
        a,
        b
    ] : [
        b,
        a
    ];
    if (typeof big === "bigint" && typeof num === "number" && Number.isInteger(num) && BigInt(num) === big) return true;
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
    match (actual, pattern, message) {
        const hit = typeof actual === "string" && (typeof pattern === "string" ? actual.includes(pattern) : pattern.test(actual));
        if (!hit) throw new AssertionError(`${message ?? "assert.match"}\n  actual:  ${show(actual)}\n  pattern: ${show(pattern instanceof RegExp ? String(pattern) : pattern)}`);
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
    const [{ createRoot, hydrateRoot }, { withHoisted }] = await Promise.all([
        import("react-dom/client"),
        import("./react.js")
    ]);
    let root;
    let hydrated = null;
    if (rendered !== null) {
        const { html, hoisted } = JSON.parse(rendered);
        const unsafe = container.setHTMLUnsafe;
        if (typeof unsafe === "function") unsafe.call(container, html);
        else container.innerHTML = html;
        root = hydrateRoot(container, withHoisted(decodeValue(hoisted), element));
        hydrated = module;
    } else {
        root = createRoot(container);
        root.render(element);
    }
    await settle();
    return {
        ...within(container),
        container,
        baseElement: document.body,
        root,
        hydrated,
        unmount () {
            root.unmount();
            container.remove();
        },
        async rerender (next) {
            root.render(next);
            await settle();
        },
        asFragment () {
            const template = document.createElement("template");
            template.innerHTML = container.innerHTML;
            return template.content;
        },
        debug (target, maxLength) {
            console.log(prettyDOM(target ?? container, maxLength));
        }
    };
}
export async function renderHook(hook, options = {}) {
    const { createElement } = await import("react");
    const result = {
        current: undefined
    };
    function Probe({ props }) {
        result.current = hook(props);
        return null;
    }
    const element = (props)=>{
        const probe = createElement(Probe, {
            props
        });
        return options.wrapper ? createElement(options.wrapper, null, probe) : probe;
    };
    const rendered = await render(element(options.initialProps), {
        ctx: options.ctx,
        hydrate: false
    });
    return {
        result,
        rerender: (props)=>rendered.rerender(element(props ?? options.initialProps)),
        unmount: ()=>rendered.unmount()
    };
}
export async function act(body) {
    const out = await body();
    await settle();
    return out;
}
export function cleanup() {
    discard(document.body);
    document.body.innerHTML = "";
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
    discard(document);
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
//# sourceMappingURL=testing.js.map
