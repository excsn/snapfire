import { createApp, createSSRApp, defineComponent, h, onScopeDispose, reactive } from "vue";
import { get, set, subscribe } from "./store.js";
const held = new WeakMap();
const RUNTIME_PROPS = [
    "$h",
    "$k",
    "$s"
];
function ownProps(props) {
    const own = {};
    for (const key of Object.keys(props)){
        if (!RUNTIME_PROPS.includes(key)) own[key] = props[key];
    }
    return own;
}
function rootFor(component, props) {
    const state = reactive(ownProps(props));
    const root = defineComponent({
        name: "SfIsland",
        setup () {
            return ()=>h(component, state);
        }
    });
    return {
        root,
        props: state
    };
}
function componentOf(module) {
    const holder = module;
    return holder && holder.default || module;
}
export const vueMounter = (module, props, el, hydrate)=>{
    const { root, props: state } = rootFor(componentOf(module), props);
    const app = hydrate ? createSSRApp(root) : createApp(root);
    held.set(el, state);
    app.mount(el);
    return app;
};
export const vuePatcher = (handle, module, props, el)=>{
    const state = held.get(el);
    if (!state) return;
    const next = ownProps(props);
    for (const key of Object.keys(state)){
        if (!(key in next)) delete state[key];
    }
    Object.assign(state, next);
};
export function useStore(key, initial) {
    const state = reactive({
        value: get(key) ?? initial
    });
    let ours = false;
    const off = subscribe(key, (next)=>{
        if (ours) return;
        state.value = next;
    });
    onScopeDispose(off);
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
//# sourceMappingURL=vue.js.map
