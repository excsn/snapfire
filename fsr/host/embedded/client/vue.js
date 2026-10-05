import { createApp, createSSRApp, defineComponent, h, inject, onMounted, onScopeDispose, onUpdated, reactive, ref, shallowRef, watch } from "vue";
import { islandState, patchIsland, scan } from "./boot.js";
import { CHILDREN_ATTR } from "./render.js";
import { morph } from "./server.js";
import { encodeValue } from "./values.js";
import { get, set, subscribe } from "./store.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { linkAttributes } from "./link.js";
import { pictureParts } from "./picture.js";
const held = new WeakMap();
const RUNTIME_PROPS = [
    "$h",
    "$k",
    "$s",
    "$sv"
];
const RENDERED_STORE = Symbol("sf-rendered-store");
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
export const vueMounter = (module, props, el, hydrate)=>{
    const { root, props: state, children } = rootFor(componentOf(module), props, el);
    const app = hydrate ? createSSRApp(root) : createApp(root);
    const rendered = props.$sv;
    app.provide(RENDERED_STORE, hydrate && rendered ? rendered : null);
    held.set(el, state);
    childrenHeld.set(el, children);
    app.mount(el);
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
    const live = ()=>get(key) ?? initial;
    const state = reactive({
        value: rendered !== null && key in rendered ? rendered[key] : live()
    });
    let ours = false;
    const off = subscribe(key, (next)=>{
        if (ours) return;
        state.value = next;
    });
    onScopeDispose(off);
    if (rendered !== null) {
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
let placed = 0;
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
        return ()=>h("a", linkAttributes(attrs), slots.default?.());
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
