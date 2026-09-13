import { createApp, createSSRApp, defineComponent, h, onMounted, onScopeDispose, onUpdated, reactive, ref, type App, type Component } from "vue";

import { patchIsland, scan, type MountTiming, type Mounter, type Patcher, type Props } from "./boot.js";
import { encodeValue } from "./values.js";
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
 * Vue mounts a component through an app and an app takes its root props once.
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

/** The default export of a module or the module when it is the component itself. */
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

/** Ids for the markers this module writes, which no server rendered. */
let placed = 0;

/** What [`Mount`] takes: the module id the registry knows the island under, the props it is mounted with and re-patched from, plus the timing that schedules it. */
export interface MountProps {
  module: string;
  props?: Props;
  when?: MountTiming;
}

/**
 * Places an island by module id inside a Vue tree, for a component holding
 * one another framework mounts: the registry entry decides the mounter, so
 * this writes the marker the boot runtime reads and never renders the child
 * itself. Nothing rendered it on the server, so it is mounted fresh and
 * patched from here whenever its props change.
 */
export const Mount: Component = defineComponent({
  name: "SfMount",
  props: {
    module: { type: String, required: true },
    props: { type: Object, default: () => ({}) },
    when: { type: String as () => MountTiming, default: undefined },
  },
  setup(props) {
    const region = ref<Element | null>(null);
    const id = `sf-m${++placed}`;
    const apply = () => {
      const host = region.value;
      if (!host) return;
      const mounted = host.querySelector("sf-i");
      if (mounted) {
        void patchIsland(mounted, props.props as Props);
        return;
      }
      const marker = document.createElement("sf-i");
      marker.id = id;
      marker.setAttribute("data-sf-module", props.module);
      const script = document.createElement("script");
      script.type = "application/json";
      script.setAttribute("data-sf-props", id);
      script.textContent = JSON.stringify(encodeValue(props.props as never));
      host.append(marker, script);
      scan(host);
    };
    onMounted(apply);
    onUpdated(apply);
    return () => h("sf-s", { ref: region, "data-sf-island": "", "data-sf-when": props.when });
  },
});
