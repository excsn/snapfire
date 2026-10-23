import { createRawSnippet, getContext, hydrate, mount as mountSvelte, setContext, unmount, type Component } from "svelte";
import { createSubscriber } from "svelte/reactivity";

import { discard, islandState, patchIsland, scan, type MountTiming, type Mounter, type Patcher, type Props, type Unmounter } from "./boot.js";
import { CHILDREN_ATTR } from "./render.js";
import { morph } from "./server.js";
import { get, set, subscribe, type StoreKey } from "./store.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { linkAttributes, type LinkOptions } from "./link.js";
import { pendingLink, subscribePendingLink } from "./navigator.js";
import { pictureParts, type PictureOptions } from "./picture.js";
import { embedParts, type EmbedOptions } from "./embed.js";
import { encodeValue } from "./values.js";

/** What the server rides on an island's props for the runtime rather than the component. */
const RUNTIME_PROPS = ["$h", "$k", "$s", "$sv"];

/** A mounted island's props and the region its children render in, so a patch updates both in place. */
interface Held {
  props: { [key: string]: unknown };
  changed: () => void;
  region: Element | null;
  children: string | null;
  snippet: ReturnType<typeof createRawSnippet> | null;
  /** True while Svelte hydrates the server's markup, when the region already holds what the server wrote. */
  hydrating: boolean;
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

/**
 * An island's children as its `children` snippet: an `<sf-s data-sf-children>`
 * written from the server's markup and scanned for islands, which a patch
 * morphs in place. While hydrating, the region the server rendered is kept as
 * it stands, so the islands inside it are not torn down.
 */
function childrenSnippet(state: Held) {
  if (state.snippet) return state.snippet;
  state.snippet = createRawSnippet(() => ({
    render: () => `<sf-s ${CHILDREN_ATTR}></sf-s>`,
    setup: (region: Element) => {
      state.region = region;
      if (!state.hydrating) {
        const template = document.createElement("template");
        template.innerHTML = state.children ?? "";
        region.replaceChildren(template.content);
      }
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
  const state: Held = { props: ownProps(props), changed: () => {}, region: null, children: region ? region.innerHTML : null, snippet: null, hydrating: isHydrating };
  held.set(el, state);
  const options = { target: el, props: liveProps(state), context: new Map<unknown, unknown>([[ISLAND, el]]) };
  if (isHydrating) {
    const instance = hydrate(componentOf(module), options);
    state.hydrating = false;
    return instance;
  }
  el.replaceChildren();
  return mountSvelte(componentOf(module), options);
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

/** The element an island is mounted in, for `useLinkStatus` in an island the server placed inside a link. */
const ISLAND = Symbol("sf-island");

/** The anchor FSR's `Link` component rendered, for `useLinkStatus` in a component inside it. */
const LINK = Symbol("sf-link");

/** What FSR's `Link` component calls as it initialises, so `useLinkStatus` in a component inside it reads the anchor it rendered. */
export function holdLink(anchor: () => Element | null): void {
  setContext(LINK, anchor);
}

/** Whether the navigation a click on the enclosing link started is still in flight, as a Svelte store of `{ pending }`: FSR's `Link` this component renders inside, else the link the server placed this island in. It turns false once the page the link names is shown. Call it while the component initialises. */
export function useLinkStatus(): { subscribe: (run: (value: { pending: boolean }) => void) => () => void } {
  const link = getContext<(() => Element | null) | undefined>(LINK);
  const island = getContext<Element | undefined>(ISLAND);
  const pending = () => {
    const at = pendingLink();
    return at !== null && at === (link ? link() : (island?.closest("a") ?? null));
  };
  return {
    subscribe: (run) => {
      run({ pending: pending() });
      return subscribePendingLink(() => run({ pending: pending() }));
    },
  };
}

/** The attributes of an anchor the navigator reads, to spread on an `<a>`: `<a {...link({ href: "/next" })}>`. */
export function link(options: LinkOptions): Record<string, unknown> {
  return linkAttributes(options);
}

/** The `<img>` and, when the image policy derives variants, the `<source>`s the server writes for the same options: an `<img {...parts.img}>` inside a `<picture>` holding one `<source>` per entry of `parts.sources`, or the `<img>` alone when `sources` is null. */
export function picture(options: PictureOptions): { img: Record<string, unknown>; sources: Record<string, unknown>[] | null } {
  return pictureParts(options);
}

/** The attributes of the placeholder the server writes for a video hosted elsewhere: a `<div {...parts.outer}>` holding a `<button {...parts.button}>` with the still, the title and the play mark. */
export function embed(options: EmbedOptions): ReturnType<typeof embedParts> {
  return embedParts(options);
}

/** Ids for the markers this module writes, which no server rendered. */
let placed = 0;

/** What the Svelte `Mount` the build supplies attaches to its `<sf-s data-sf-island>`: the marker for `module` with its props inside it, mounted by the registry's entry the way the React and Vue `Mount` do theirs and patched in place when the props change. */
export function mount(module: string, props: Props = {}, _when?: MountTiming): (region: Element) => () => void {
  return (region) => {
    const mounted = region.querySelector("sf-i");
    if (mounted) {
      void patchIsland(mounted, props);
    } else {
      const id = `sf-svelte-m${++placed}`;
      const marker = document.createElement("sf-i");
      marker.id = id;
      marker.setAttribute("data-sf-module", module);
      const script = document.createElement("script");
      script.type = "application/json";
      script.setAttribute("data-sf-props", id);
      script.textContent = JSON.stringify(encodeValue(props as never));
      region.append(marker, script);
      scan(region);
    }
    return () => {
      queueMicrotask(() => {
        if (!region.isConnected) discard(region);
      });
    };
  };
}

/**
 * An island another framework mounts, placed by module id as an attachment:
 * `<div {@attach island("src/ui/Chart.tsx#default", { label })}></div>`. The
 * registry entry decides the mounter, so this writes the marker the boot
 * runtime reads and never renders the island itself. When the props change
 * Svelte runs the attachment again, which patches the mounted island in place.
 * An element taken out of the document ends the islands inside it.
 */
export function island(module: string, props: Props = {}, when?: MountTiming): (element: Element) => () => void {
  return (element) => {
    let region = Array.from(element.children).find((child) => child.tagName === "SF-S" && child.hasAttribute("data-sf-island"));
    const mounted = region?.querySelector("sf-i");
    if (region && mounted) {
      void patchIsland(mounted, props);
    } else {
      region = document.createElement("sf-s");
      region.setAttribute("data-sf-island", "");
      if (when) region.setAttribute("data-sf-when", when);
      const id = `sf-svelte-m${++placed}`;
      const marker = document.createElement("sf-i");
      marker.id = id;
      marker.setAttribute("data-sf-module", module);
      const script = document.createElement("script");
      script.type = "application/json";
      script.setAttribute("data-sf-props", id);
      script.textContent = JSON.stringify(encodeValue(props as never));
      region.append(marker, script);
      element.append(region);
      scan(region);
    }
    return () => {
      queueMicrotask(() => {
        if (!element.isConnected) discard(element);
      });
    };
  };
}

