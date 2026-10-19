import { createRawSnippet, hydrate, mount, unmount, type Component } from "svelte";
import { createSubscriber } from "svelte/reactivity";

import { islandState, scan, type Mounter, type Patcher, type Props, type Unmounter } from "./boot.js";
import { CHILDREN_ATTR } from "./render.js";
import { morph } from "./server.js";
import { get, set, subscribe, type StoreKey } from "./store.js";
import { currentLocale, subscribeLocale } from "./locale.js";

/** What the server rides on an island's props for the runtime rather than the component. */
const RUNTIME_PROPS = ["$h", "$k", "$s", "$sv"];

/** A mounted island's props and the region its children render in, so a patch updates both in place. */
interface Held {
  props: { [key: string]: unknown };
  changed: () => void;
  region: Element | null;
  children: string | null;
  snippet: ReturnType<typeof createRawSnippet> | null;
}

const held = new WeakMap<Element, Held>();

function ownProps(props: Props): { [key: string]: unknown } {
  const own: { [key: string]: unknown } = {};
  for (const key of Object.keys(props)) {
    if (!RUNTIME_PROPS.includes(key)) own[key] = props[key];
  }
  return own;
}

/** The region `el`'s children render in, not one inside a nested island. */
function childrenRegion(el: Element): Element | null {
  for (const region of Array.from(el.querySelectorAll(`sf-s[${CHILDREN_ATTR}], template[${CHILDREN_ATTR}]`))) {
    if (region.parentElement?.closest("sf-i") === el) return region;
  }
  return null;
}

/** The default export of a module or the module when it is the component itself. */
function componentOf(module: unknown): Component<{ [key: string]: unknown }> {
  const holder = module as { default?: Component<{ [key: string]: unknown }> };
  return (holder && holder.default) || (module as Component<{ [key: string]: unknown }>);
}

/**
 * The props a component is mounted with: Svelte reads a prop from this object
 * each time it uses it, so a read that subscribes makes a patch re-render
 * what reads the prop and keeps the component's own `$state`.
 */
function liveProps(state: Held): { [key: string]: unknown } {
  const subscribed = createSubscriber((update) => {
    state.changed = update;
    return () => {
      state.changed = () => {};
    };
  });
  const read = (key: string | symbol) => {
    subscribed();
    return typeof key === "string" ? state.props[key] : undefined;
  };
  return new Proxy(
    {},
    {
      get: (_, key) => (key === "children" && state.children !== null ? childrenSnippet(state) : read(key)),
      has: (_, key) => {
        subscribed();
        return typeof key === "string" && (key in state.props || (key === "children" && state.children !== null));
      },
      ownKeys: () => {
        subscribed();
        return [...Object.keys(state.props), ...(state.children !== null ? ["children"] : [])];
      },
      getOwnPropertyDescriptor: (_, key) => ({ configurable: true, enumerable: true, value: key === "children" && state.children !== null ? childrenSnippet(state) : read(key) }),
    },
  );
}

/** An island's children as its `children` snippet: an `<sf-s data-sf-children>` written from the server's markup and scanned for islands, which a patch morphs in place. */
function childrenSnippet(state: Held) {
  if (state.snippet) return state.snippet;
  state.snippet = createRawSnippet(() => ({
    render: () => `<sf-s ${CHILDREN_ATTR}></sf-s>`,
    setup: (region: Element) => {
      state.region = region;
      const template = document.createElement("template");
      template.innerHTML = state.children ?? "";
      region.replaceChildren(template.content);
      scan(region);
      return () => {
        if (state.region === region) state.region = null;
      };
    },
  }));
  return state.snippet;
}

export const svelteMounter: Mounter = (module, props, el, isHydrating) => {
  const region = childrenRegion(el);
  const state: Held = { props: ownProps(props), changed: () => {}, region: null, children: region ? region.innerHTML : null, snippet: null };
  held.set(el, state);
  const options = { target: el, props: liveProps(state) };
  if (isHydrating) return hydrate(componentOf(module), options);
  el.replaceChildren();
  return mount(componentOf(module), options);
};

export const sveltePatcher: Patcher = (_handle, _module, props, el) => {
  const state = held.get(el);
  if (!state) return;
  state.props = ownProps(props);
  const fresh = islandState(el)?.children ?? null;
  if (fresh !== null && fresh !== state.children) {
    state.children = fresh;
    if (state.region) {
      morph(state.region, fresh);
      scan(state.region);
    }
  }
  state.changed();
};

export const svelteUnmounter: Unmounter = (handle, el) => {
  void unmount(handle as Record<string, unknown>);
  held.delete(el);
};

/** A Svelte store over the neutral store: `const count = useStore(countKey, 0)`, then `$count` reads it and `$count = 3` writes it, in runes and legacy mode alike, following every other island that shares the key. */
export function useStore<T>(key: StoreKey<T>, initial: T): { subscribe: (run: (value: T) => void) => () => void; set: (value: T) => void; update: (fn: (value: T) => T) => void } {
  const current = () => (get(key) ?? initial) as T;
  return {
    subscribe: (run) => {
      run(current());
      return subscribe(key, () => run(current()));
    },
    set: (value) => set(key, value),
    update: (fn) => set(key, fn(current())),
  };
}

/** The document's locale as a Svelte store, following every navigation that changes it: `$locale`. */
export function useLocale(): { subscribe: (run: (value: string) => void) => () => void } {
  return {
    subscribe: (run) => {
      run(currentLocale());
      return subscribeLocale(run);
    },
  };
}
