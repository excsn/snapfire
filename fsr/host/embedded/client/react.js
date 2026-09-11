import { cloneElement, createContext, createElement, Fragment, isValidElement, useCallback, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { createRoot, hydrateRoot } from "react-dom/client";
import { islandState, patchIsland, scan } from "./boot.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { get, set, subscribe } from "./store.js";
function slotOf(el) {
    for (const slot of Array.from(el.querySelectorAll("sf-s:not([data-sf-island]):not([data-sf-name])"))){
        if (slot.parentElement?.closest("sf-i") === el) return slot;
    }
    return null;
}
function namedSlotsOf(el) {
    return Array.from(el.querySelectorAll("sf-s[data-sf-name]")).filter((slot)=>slot.parentElement?.closest("sf-i") === el);
}
function adopted(slot, name) {
    const props = {
        dangerouslySetInnerHTML: {
            __html: slot?.innerHTML ?? ""
        },
        suppressHydrationWarning: true
    };
    if (name !== undefined) props["data-sf-name"] = name;
    return createElement("sf-s", props);
}
const children = new WeakMap();
function childrenFor(el) {
    const held = children.get(el);
    if (held) return held;
    const slot = slotOf(el);
    if (!slot) return undefined;
    const element = adopted(slot);
    children.set(el, element);
    return element;
}
const slotProps = new WeakMap();
function slotPropsFor(el) {
    const held = slotProps.get(el);
    if (held) return held;
    const props = {};
    for (const slot of namedSlotsOf(el)){
        const name = slot.getAttribute("data-sf-name") ?? "";
        props[name] = adopted(slot, name);
    }
    slotProps.set(el, props);
    return props;
}
const RegionsContext = createContext(null);
const regions = new WeakMap();
function regionsOf(el) {
    const held = regions.get(el);
    if (held) return held;
    const slots = Array.from(el.querySelectorAll("sf-s[data-sf-island]")).filter((slot)=>slot.parentElement?.closest("sf-i") === el);
    const byKey = new Map();
    for (const slot of slots){
        const key = slot.getAttribute("data-sf-region");
        if (key) byKey.set(key, slot);
    }
    const built = {
        root: el,
        byKey,
        slots,
        next: 0,
        sources: null,
        gen: 0
    };
    regions.set(el, built);
    return built;
}
const KEY_PROP = "__sfKey";
const REGION_KEY = "$k";
function keyOf(children) {
    if (!isValidElement(children)) return null;
    const key = children.props[KEY_PROP];
    return typeof key === "string" ? key : null;
}
function propsOf(children) {
    if (!isValidElement(children)) return {};
    const { [KEY_PROP]: _key, ...rest } = children.props;
    return rest;
}
function rootIn(region) {
    const first = region.firstElementChild;
    return first?.tagName === "SF-I" && first.hasAttribute("data-sf-mounted") ? first : null;
}
export function Island({ when, mode, children }) {
    const regions = useContext(RegionsContext);
    const key = keyOf(children);
    const node = useRef(null);
    const consumed = useRef(-1);
    const hoisted = useRef(undefined);
    const [claimed] = useState(()=>{
        if (!regions) return {
            html: "",
            inline: false
        };
        const held = key === null ? regions.slots[regions.next++] : regions.byKey.get(key);
        if (held) {
            if (key !== null) regions.byKey.delete(key);
            return {
                html: held.innerHTML,
                inline: false
            };
        }
        return {
            html: "",
            inline: key === null || !regions.sources?.has(key)
        };
    });
    useEffect(()=>{
        const region = node.current;
        if (!region || !regions || claimed.inline) return;
        const fresh = regions.gen !== consumed.current;
        consumed.current = regions.gen;
        const source = fresh && key !== null ? regions.sources?.get(key) ?? null : null;
        const mounted = rootIn(region);
        if (!mounted) {
            if (source) region.innerHTML = source.html;
            if (region.firstElementChild) scan(region);
            return;
        }
        if (source) {
            hoisted.current = source.props[HOISTED_PROP];
            void patchIsland(mounted, source.props, source.nested);
            return;
        }
        if (hoisted.current === undefined) hoisted.current = islandState(mounted)?.props[HOISTED_PROP];
        const next = propsOf(children);
        if (hoisted.current !== undefined) next[HOISTED_PROP] = hoisted.current;
        void patchIsland(mounted, next, null);
    });
    if (claimed.inline) return createElement(Fragment, null, isValidElement(children) ? cloneElement(children, {
        [KEY_PROP]: undefined
    }) : children);
    const props = {
        ref: node,
        "data-sf-island": "",
        dangerouslySetInnerHTML: {
            __html: claimed.html
        },
        suppressHydrationWarning: true
    };
    if (key !== null) props["data-sf-region"] = key;
    if (when) props["data-sf-when"] = when;
    if (mode) props["data-sf-mode"] = mode;
    return createElement("sf-s", props);
}
export function island(component, options = {}) {
    return function IslandOf(props) {
        return createElement(Island, {
            when: options.when,
            mode: options.mode
        }, createElement(component, props));
    };
}
export function Slot({ name }) {
    const regions = useContext(RegionsContext);
    const [html] = useState(()=>{
        if (!regions) return "";
        const slot = namedSlotsOf(regions.root).find((s)=>s.getAttribute("data-sf-name") === name);
        return slot?.innerHTML ?? "";
    });
    return createElement("sf-s", {
        "data-sf-name": name,
        dangerouslySetInnerHTML: {
            __html: html
        },
        suppressHydrationWarning: true
    });
}
export function useStore(k, initial) {
    const [fallback] = useState(initial);
    const read = ()=>{
        const held = get(k);
        return held === undefined ? fallback : held;
    };
    const value = useSyncExternalStore((changed)=>subscribe(k, changed), read, read);
    return [
        value,
        useCallback((next)=>set(k, next), [
            k
        ])
    ];
}
export function useLocale() {
    return useSyncExternalStore(subscribeLocale, currentLocale, currentLocale);
}
export function Link({ full, into, prefetch, native, ...rest }) {
    const attrs = {
        ...rest
    };
    if (full) attrs["data-sf-full"] = "true";
    if (into) attrs["data-sf-into"] = into;
    if (prefetch) attrs["data-sf-prefetch"] = prefetch;
    if (native) attrs["data-sf-native"] = "true";
    return createElement("a", attrs);
}
const HoistContext = createContext(null);
const HOISTED_PROP = "$h";
const PathContext = createContext([]);
export function useHoisted(module) {
    const table = useContext(HoistContext);
    const base = useContext(PathContext);
    return useMemo(()=>{
        const path = [
            ...base
        ];
        const key = (id)=>path.length === 0 ? `${module}|${id}` : `${module}|${id}@${path.join(".")}`;
        return {
            r (id, compute) {
                if (table === null) return compute();
                const k = key(id);
                return k in table ? table[k] : compute();
            },
            c (id, hit, miss) {
                if (table === null) return miss();
                const k = key(id);
                const html = table[k];
                return typeof html === "string" ? hit({
                    __html: html
                }) : miss();
            },
            k (id) {
                return key(`i${id}`);
            },
            l (f) {
                return (...args)=>{
                    path.push(typeof args[1] === "number" ? args[1] : -1);
                    try {
                        const out = f(...args);
                        if (isElement(out)) {
                            return createElement(PathContext.Provider, {
                                key: out.key,
                                value: [
                                    ...path
                                ]
                            }, out);
                        }
                        return out;
                    } finally{
                        path.pop();
                    }
                };
            }
        };
    }, [
        table,
        module,
        base
    ]);
}
function isElement(value) {
    return typeof value === "object" && value !== null && "$$typeof" in value && "key" in value;
}
export function withHoisted(table, element) {
    return createElement(HoistContext.Provider, {
        value: table
    }, element);
}
function splitHoisted(props) {
    const { [HOISTED_PROP]: hoisted, [REGION_KEY]: _key, ...rest } = props;
    return [
        rest,
        hoisted ?? null
    ];
}
function withRegions(el, element, patched) {
    const state = regionsOf(el);
    if (patched) {
        state.sources = islandState(el)?.regions ?? null;
        state.gen += 1;
    }
    return createElement(RegionsContext.Provider, {
        value: state
    }, element);
}
function islandElement(component, props, el, patched) {
    const [own, hoisted] = splitHoisted(props);
    const element = createElement(component, {
        ...own,
        ...slotPropsFor(el)
    }, childrenFor(el));
    return createElement(Mounting, {
        el
    }, withRegions(el, withHoisted(hoisted, element), patched));
}
function Mounting({ el, children }) {
    useEffect(()=>{
        scan(el);
    });
    return createElement(Fragment, null, children);
}
export const reactMounter = (component, props, el, hydrate)=>{
    const element = islandElement(component, props, el, false);
    if (hydrate) {
        return hydrateRoot(el, element);
    }
    const root = createRoot(el);
    root.render(element);
    return root;
};
export const reactPatcher = (handle, component, props, el)=>{
    handle.render(islandElement(component, props, el, true));
};
//# sourceMappingURL=react.js.map
