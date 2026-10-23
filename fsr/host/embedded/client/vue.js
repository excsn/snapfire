import { createApp, createSSRApp, defineComponent, getCurrentInstance, h, inject, onMounted, onScopeDispose, onUpdated, provide, reactive, ref, shallowRef, watch } from "vue";
import { islandState, patchIsland, scan } from "./boot.js";
import { CHILDREN_ATTR, PLACED_ISLAND, PLACED_OPEN, REGION_KEY } from "./render.js";
import { morph } from "./server.js";
import { encodeValue } from "./values.js";
import { get, set, subscribe } from "./store.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { linkAttributes } from "./link.js";
import { pendingLink, subscribePendingLink } from "./navigator.js";
import { pictureParts } from "./picture.js";
import { embedParts } from "./embed.js";
const held = new WeakMap();
const RUNTIME_PROPS = [
    "$h",
    "$k",
    "$s",
    "$sv"
];
const RENDERED_STORE = Symbol("sf-rendered-store");
const ISLAND = Symbol("sf-island");
const LINK = Symbol("sf-link");
function ownProps(props) {
    const own = {};
    for (const key of Object.keys(props)){
        if (!RUNTIME_PROPS.includes(key)) own[key] = props[key];
    }
    return own;
}
const childrenHeld = new WeakMap();
function childrenRegion(el) {
    for (const region of Array.from(el.querySelectorAll(`sf-s[${CHILDREN_ATTR}], template[${CHILDREN_ATTR}]`))){
        if (region.parentElement?.closest("sf-i") === el) return region;
    }
    return null;
}
const Children = defineComponent({
    name: "SfChildren",
    props: {
        html: {
            type: String,
            required: true
        }
    },
    setup (props) {
        const region = shallowRef(null);
        let written = null;
        const write = ()=>{
            const el = region.value;
            if (!el || written === props.html) return;
            if (written === null && el.childNodes.length > 0) {
                written = props.html;
                return;
            }
            if (written === null) {
                const template = document.createElement("template");
                template.innerHTML = props.html;
                el.replaceChildren(template.content);
            } else {
                morph(el, props.html);
            }
            written = props.html;
            scan(el);
        };
        onMounted(write);
        watch(()=>props.html, write, {
            flush: "post"
        });
        return ()=>h("sf-s", {
                ref: region,
                [CHILDREN_ATTR]: ""
            });
    }
});
function rootFor(component, props, el) {
    const state = reactive(ownProps(props));
    const region = childrenRegion(el);
    const children = ref(region ? region.innerHTML : null);
    if (region?.tagName === "TEMPLATE") region.remove();
    const root = defineComponent({
        name: "SfIsland",
        setup () {
            return ()=>h(component, state, children.value === null ? undefined : {
                    default: ()=>h(Children, {
                            html: children.value ?? ""
                        })
                });
        }
    });
    return {
        root,
        props: state,
        children
    };
}
function componentOf(module) {
    const holder = module;
    return holder && holder.default || module;
}
function startTeleports(el, props) {
    const name = typeof props[REGION_KEY] === "string" ? props[REGION_KEY] : el.id;
    const set = new Set();
    for (const open of Array.from(document.querySelectorAll(`template[${PLACED_OPEN}]`))){
        const target = open.parentNode;
        if (!target || set.has(target) || open.getAttribute(PLACED_ISLAND) !== name) continue;
        target._lpa = open.nextSibling;
        set.add(target);
    }
    if (!set.has(document.body)) document.body._lpa = null;
}
export const vueMounter = (module, props, el, hydrate)=>{
    const { root, props: state, children } = rootFor(componentOf(module), props, el);
    const app = hydrate ? createSSRApp(root) : createApp(root);
    const rendered = {
        values: props.$sv ?? {},
        hydrating: hydrate
    };
    app.provide(RENDERED_STORE, rendered);
    app.provide(ISLAND, el);
    held.set(el, state);
    childrenHeld.set(el, children);
    if (hydrate) startTeleports(el, props);
    app.mount(el);
    rendered.hydrating = false;
    return app;
};
export const vueUnmounter = (handle, el)=>{
    handle.unmount();
    held.delete(el);
    childrenHeld.delete(el);
};
export const vuePatcher = (handle, module, props, el)=>{
    const state = held.get(el);
    if (!state) return;
    const next = ownProps(props);
    for (const key of Object.keys(state)){
        if (!(key in next)) delete state[key];
    }
    Object.assign(state, next);
    const fresh = islandState(el)?.children ?? null;
    const children = childrenHeld.get(el);
    if (fresh !== null && children) children.value = fresh;
};
export function useStore(key, initial) {
    const rendered = inject(RENDERED_STORE, null);
    const hydrating = rendered !== null && (rendered.hydrating || getCurrentInstance()?.vnode.el != null);
    const values = hydrating ? rendered.values : null;
    const live = ()=>get(key) ?? initial;
    const state = reactive({
        value: values === null ? live() : key in values ? values[key] : initial
    });
    let ours = false;
    const off = subscribe(key, (next)=>{
        if (ours) return;
        state.value = next;
    });
    onScopeDispose(off);
    if (values !== null) {
        onMounted(()=>{
            const now = live();
            if (!Object.is(state.value, now)) state.value = now;
        });
    }
    return new Proxy(state, {
        get: (target, name)=>target[name],
        set: (target, name, value)=>{
            target[name] = value;
            if (name === "value") {
                ours = true;
                set(key, value);
                ours = false;
            }
            return true;
        }
    });
}
export function useLocale() {
    const locale = ref(currentLocale());
    onScopeDispose(subscribeLocale((next)=>{
        locale.value = next;
    }));
    return locale;
}
export function useLinkStatus() {
    const link = inject(LINK, null);
    const island = inject(ISLAND, null);
    const status = reactive({
        pending: false
    });
    const update = ()=>{
        const at = pendingLink();
        status.pending = at !== null && at === (link ? link.value : island?.closest("a") ?? null);
    };
    update();
    onScopeDispose(subscribePendingLink(update));
    return status;
}
let placed = 0;
export const Place = defineComponent({
    name: "SfPlace",
    props: {
        name: {
            type: String,
            required: true
        },
        as: {
            type: String,
            default: "div"
        }
    },
    setup (props) {
        return ()=>h(props.as, {
                id: props.name
            });
    }
});
export const Mount = defineComponent({
    name: "SfMount",
    props: {
        module: {
            type: String,
            required: true
        },
        props: {
            type: Object,
            default: ()=>({})
        },
        when: {
            type: String,
            default: undefined
        }
    },
    setup (props) {
        const region = ref(null);
        const id = `sf-m${++placed}`;
        const apply = ()=>{
            const host = region.value;
            if (!host) return;
            const mounted = host.querySelector("sf-i");
            if (mounted) {
                void patchIsland(mounted, props.props);
                return;
            }
            const marker = document.createElement("sf-i");
            marker.id = id;
            marker.setAttribute("data-sf-module", props.module);
            const script = document.createElement("script");
            script.type = "application/json";
            script.setAttribute("data-sf-props", id);
            script.textContent = JSON.stringify(encodeValue(props.props));
            host.append(marker, script);
            scan(host);
        };
        onMounted(apply);
        onUpdated(apply);
        return ()=>h("sf-s", {
                ref: region,
                "data-sf-island": "",
                "data-sf-when": props.when
            });
    }
});
export const Link = defineComponent({
    name: "SfLink",
    inheritAttrs: false,
    setup (_, { attrs, slots }) {
        const anchor = ref(null);
        provide(LINK, anchor);
        return ()=>h("a", {
                ...linkAttributes(attrs),
                ref: anchor
            }, slots.default?.());
    }
});
export const Embed = defineComponent({
    name: "SfEmbed",
    inheritAttrs: false,
    setup (_, { attrs }) {
        return ()=>{
            const props = attrs;
            const parts = embedParts(props);
            const face = [
                ...parts.still ? [
                    h("img", parts.still)
                ] : [],
                h("span", parts.title, props.title),
                h("span", parts.play, "\u25B6")
            ];
            return h("div", parts.outer, [
                h("button", parts.button, face)
            ]);
        };
    }
});
export const Picture = defineComponent({
    name: "SfPicture",
    inheritAttrs: false,
    setup (_, { attrs }) {
        return ()=>{
            const { img, sources } = pictureParts(attrs);
            if (sources === null) return h("img", img);
            return h("picture", null, [
                ...sources.map((source)=>h("source", source)),
                h("img", img)
            ]);
        };
    }
});
//# sourceMappingURL=vue.js.map
