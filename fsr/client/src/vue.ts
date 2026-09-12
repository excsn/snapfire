import { createApp, createSSRApp, defineComponent, h, onScopeDispose, reactive, type App, type Component } from "vue";

import type { Mounter, Patcher, Props } from "./boot.js";
import { get, set, subscribe, type StoreKey } from "./store.js";

/** The reactive props an island was mounted with, so a patch re-renders it in place instead of tearing it down. */
const held = new WeakMap<Element, Record<string, unknown>>();

/** What the server rides on an island's props for the runtime rather than the component: the hoisted table, the region key and a server-mode island's state. */
const RUNTIME_PROPS = ["$h", "$k", "$s"];

function ownProps(props: Props): Record<string, unknown> {
  const own: Record<string, unknown> = {};
  for (const key of Object.keys(props)) {
    if (!RUNTIME_PROPS.includes(key)) own[key] = props[key];
  }
  return own;
}

/**
 * Vue mounts a component through an app, and an app takes its root props once.
 * A one-element wrapper holding them reactively is what makes a patch possible:
 * it renders nothing of its own, so the markup Vue hydrates is the component's
 * and nothing else.
 */
function rootFor(component: Component, props: Props): { root: Component; props: Record<string, unknown> } {
  const state = reactive(ownProps(props));
  const root = defineComponent({
    name: "SfIsland",
    setup() {
      return () => h(component, state);
    },
  });
  return { root, props: state };
}

/** The default export of a module, or the module when it is the component itself. */
function componentOf(module: unknown): Component {
  const holder = module as { default?: Component };
  return (holder && holder.default) || (module as Component);
}

export const vueMounter: Mounter = (module, props, el, hydrate) => {
  const { root, props: state } = rootFor(componentOf(module), props);
  const app: App = hydrate ? createSSRApp(root) : createApp(root);
  held.set(el, state);
  app.mount(el);
  return app;
};

export const vuePatcher: Patcher = (handle, module, props, el) => {
  const state = held.get(el);
  if (!state) return;
  const next = ownProps(props);
  for (const key of Object.keys(state)) {
    if (!(key in next)) delete state[key];
  }
  Object.assign(state, next);
};

/** The neutral store as a Vue ref: `const region = useStore(regionKey, "all")`, readable and writable, following every other island that shares the key. Call it in `setup`, so the subscription ends with the component. */
export function useStore<T>(key: StoreKey<T>, initial: T): { value: T } {
  const state = reactive({ value: get(key) ?? initial }) as { value: T };
  let ours = false;
  const off = subscribe(key, (next) => {
    if (ours) return;
    state.value = next as T;
  });
  onScopeDispose(off);
  return new Proxy(state, {
    get: (target, name) => (target as Record<string | symbol, unknown>)[name],
    set: (target, name, value) => {
      (target as Record<string | symbol, unknown>)[name] = value;
      if (name === "value") {
        ours = true;
        set(key, value as T);
        ours = false;
      }
      return true;
    },
  });
}
