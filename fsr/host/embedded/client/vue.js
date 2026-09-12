import { createApp, createSSRApp, defineComponent, h, reactive } from "vue";
import { get, set, subscribe } from "./store.js";
const held = new WeakMap();
function rootFor(component, props) {
    const state = reactive({
        ...props
    });
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
    const next = props;
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
    subscribe(key, (next)=>{
        if (ours) return;
        state.value = next;
    });
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
