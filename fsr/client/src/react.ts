import { cloneElement, createContext, createElement, Fragment, isValidElement, useCallback, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore, type AnchorHTMLAttributes, type ComponentType, type ReactElement, type ReactNode } from "react";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import { islandState, MountTiming, Mounter, Patcher, patchIsland, scan, type Props } from "./boot.js";
import type { RegionSource } from "./render.js";
import type { PrefetchTiming } from "./navigator.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { get, set, subscribe, type StoreKey } from "./store.js";

/** The `<sf-s>` a layout renders its child segment into, when `el` is a layout: the first one under it that is not inside a nested island, is not an island's own region and is not a named slot. */
function slotOf(el: Element): Element | null {
  for (const slot of Array.from(el.querySelectorAll("sf-s:not([data-sf-island]):not([data-sf-name])"))) {
    if (slot.parentElement?.closest("sf-i") === el) return slot;
  }
  return null;
}

/** The named slot regions a layout's markup holds directly: `<sf-s data-sf-name>` under `el` and not under a nested island. */
function namedSlotsOf(el: Element): Element[] {
  return Array.from(el.querySelectorAll("sf-s[data-sf-name]")).filter((slot) => slot.parentElement?.closest("sf-i") === el);
}

/** An adopted region: `<sf-s>` with the markup it already holds, created once and rendered unchanged, so React takes the region at hydration and never reconciles it. Navigation rewrites what is inside. */
function adopted(slot: Element | null, name?: string): ReactElement {
  const props: { [key: string]: unknown } = { dangerouslySetInnerHTML: { __html: slot?.innerHTML ?? "" }, suppressHydrationWarning: true };
  if (name !== undefined) props["data-sf-name"] = name;
  return createElement("sf-s", props);
}

/** The child element a layout receives, created once per root. */
const children = new WeakMap<Element, ReactElement>();

function childrenFor(el: Element): ReactElement | undefined {
  const held = children.get(el);
  if (held) return held;
  const slot = slotOf(el);
  if (!slot) return undefined;
  const element = adopted(slot);
  children.set(el, element);
  return element;
}

/** A layout's named slots as props, one adopted region per `<sf-s data-sf-name>`, created once per root. */
const slotProps = new WeakMap<Element, { [name: string]: ReactElement }>();

function slotPropsFor(el: Element): { [name: string]: ReactElement } {
  const held = slotProps.get(el);
  if (held) return held;
  const props: { [name: string]: ReactElement } = {};
  for (const slot of namedSlotsOf(el)) {
    const name = slot.getAttribute("data-sf-name") ?? "";
    props[name] = adopted(slot, name);
  }
  slotProps.set(el, props);
  return props;
}

/** The island regions of one mounted root: the ones the server rendered, by the key it wrote on each, and what the last payload said about them. Created once per root, so a re-render never reclaims a region another placement already owns. */
interface Regions {
  root: Element;
  byKey: Map<string, Element>;
  /** The regions in document order, for a placement the build gave no key. */
  slots: Element[];
  next: number;
  /** What the payload behind the current patch describes, by region key. */
  sources: Map<string, RegionSource> | null;
  /** Bumped by each patch, so a placement consumes one payload once. */
  gen: number;
}

const RegionsContext = createContext<Regions | null>(null);

/** The per-root region state, built the first time the root renders and kept for its life. */
const regions = new WeakMap<Element, Regions>();

function regionsOf(el: Element): Regions {
  const held = regions.get(el);
  if (held) return held;
  const slots = Array.from(el.querySelectorAll("sf-s[data-sf-island]")).filter((slot) => slot.parentElement?.closest("sf-i") === el);
  const byKey = new Map<string, Element>();
  for (const slot of slots) {
    const key = slot.getAttribute("data-sf-region");
    if (key) byKey.set(key, slot);
  }
  const built: Regions = { root: el, byKey, slots, next: 0, sources: null, gen: 0 };
  regions.set(el, built);
  return built;
}

/** The prop the build splices onto an island placement, carrying the region key the server wrote. */
const KEY_PROP = "__sfKey";

/** The props key an island's hoisted values arrive under, and the region key's own; neither reaches the component. */
const REGION_KEY = "$k";

function keyOf(children: ReactNode): string | null {
  if (!isValidElement(children)) return null;
  const key = (children.props as { [KEY_PROP]?: unknown })[KEY_PROP];
  return typeof key === "string" ? key : null;
}

/** The child's props as the island takes them: what the parent just computed, without the key the build spliced in. */
function propsOf(children: ReactNode): Props {
  if (!isValidElement(children)) return {};
  const { [KEY_PROP]: _key, ...rest } = children.props as { [key: string]: unknown };
  return rest as Props;
}

/** The `sf-i` a region holds, when one is mounted there. */
function rootIn(region: Element): Element | null {
  const first = region.firstElementChild;
  return first?.tagName === "SF-I" && first.hasAttribute("data-sf-mounted") ? first : null;
}

export interface IslandProps {
  /** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
  when?: MountTiming;
  /** `server`: the island's events round-trip to the server, which re-renders it; no React root is mounted. */
  mode?: "server";
  children?: ReactNode;
}

/** Places its one child component as an island of its own: on the server the child renders inside an `<sf-s data-sf-island>` region as a nested island; in the browser this element adopts that region as it stands and never reconciles it, and the boot runtime mounts the child in its own root at the timing asked for. Lowered by the build, so the child is never rendered here.
 *
 * The region is claimed once, by the key the build splices in, and after that only the island's own root writes inside it. A re-render hands the mounted root the props the parent just computed; a placement the parent has only now added takes its markup from the payload that added it, or renders its child inline when no payload describes one. */
export function Island({ when, mode, children }: IslandProps): ReactElement {
  const regions = useContext(RegionsContext);
  const key = keyOf(children);
  const node = useRef<Element | null>(null);
  const consumed = useRef(-1);
  const hoisted = useRef<unknown>(undefined);
  const [claimed] = useState<{ html: string; inline: boolean }>(() => {
    if (!regions) return { html: "", inline: false };
    // A region belongs to the placement that first took it, for as long as
    // that placement lives. A placement added later takes its markup from the
    // payload that added it, never a region another one is already showing.
    const held = key === null ? regions.slots[regions.next++] : regions.byKey.get(key);
    if (held) {
      if (key !== null) regions.byKey.delete(key);
      return { html: held.innerHTML, inline: false };
    }
    return { html: "", inline: key === null || !regions.sources?.has(key) };
  });

  useEffect(() => {
    const region = node.current;
    if (!region || !regions || claimed.inline) return;
    const fresh = regions.gen !== consumed.current;
    consumed.current = regions.gen;
    const source = fresh && key !== null ? (regions.sources?.get(key) ?? null) : null;
    const mounted = rootIn(region);
    if (!mounted) {
      if (source) region.innerHTML = source.html;
      if (region.firstElementChild) scan(region);
      return;
    }
    if (source) {
      hoisted.current = source.props[HOISTED_PROP];
      void patchIsland(mounted, source.props as Props, source.nested);
      return;
    }
    if (hoisted.current === undefined) hoisted.current = islandState(mounted)?.props[HOISTED_PROP];
    const next = propsOf(children);
    if (hoisted.current !== undefined) next[HOISTED_PROP] = hoisted.current as never;
    void patchIsland(mounted, next, null);
  });

  if (claimed.inline) return createElement(Fragment, null, isValidElement(children) ? cloneElement(children, { [KEY_PROP]: undefined } as never) : children);
  const props: { [key: string]: unknown } = { ref: node, "data-sf-island": "", dangerouslySetInnerHTML: { __html: claimed.html }, suppressHydrationWarning: true };
  if (key !== null) props["data-sf-region"] = key;
  if (when) props["data-sf-when"] = when;
  if (mode) props["data-sf-mode"] = mode;
  return createElement("sf-s", props);
}

/** `component` as a component that places it as an island with `options.when` and `options.mode` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`. */
export function island<P extends object>(component: ComponentType<P>, options: { when?: MountTiming; mode?: "server" } = {}): (props: P) => ReactElement {
  return function IslandOf(props: P): ReactElement {
    return createElement(Island, { when: options.when, mode: options.mode }, createElement(component as ComponentType<object>, props));
  };
}

export interface SlotProps {
  /** The slot's name: a `slots/<name>` directory beside the layout, or the slot a `page.<name>.tsx` under it renders into. */
  name: string;
  /** What the slot shows while nothing fills it. Rendered by the server, lowered by the build; never rendered here. */
  children?: ReactNode;
}

/** A named slot of a layout: the region a parallel route renders into, or an intercepted route opens in. On the server it is `<sf-s data-sf-name>` around the segment, or around the fallback children while nothing fills it; in the browser this element adopts the region as it stands, and navigation fills and empties it without React reconciling it. */
export function Slot({ name }: SlotProps): ReactElement {
  const regions = useContext(RegionsContext);
  const [html] = useState(() => {
    if (!regions) return "";
    const slot = namedSlotsOf(regions.root).find((s) => s.getAttribute("data-sf-name") === name);
    return slot?.innerHTML ?? "";
  });
  return createElement("sf-s", { "data-sf-name": name, dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true });
}

/** A store key as state: the value the store holds, or `initial` while nothing does, and a setter that writes the store. Every island reading the key re-renders, whichever root it is in. The server renders from the seed its loaders settled on, so the first paint and the hydration agree; the build lowers this call, so the key must be a literal or a `key()`. */
export function useStore<T>(k: StoreKey<T>, initial: T): [T, (next: T) => void] {
  const [fallback] = useState(initial);
  const read = () => {
    const held = get(k);
    return held === undefined ? fallback : held;
  };
  const value = useSyncExternalStore((changed: () => void) => subscribe(k, changed), read, read);
  return [value, useCallback((next: T) => set(k, next), [k])];
}

/** The document's locale as the application spells it, `fr_FR` or `fr`. The server renders it from the request, so the first paint and the hydration agree; a navigation that changes it re-renders every island reading it. The build lowers this call. */
export function useLocale(): string {
  return useSyncExternalStore(subscribeLocale, currentLocale, currentLocale);
}

export interface LinkProps extends AnchorHTMLAttributes<HTMLAnchorElement> {
  /** Always the document's rendering of the target, never an intercept into a slot. */
  full?: boolean;
  /** Renders the target into this slot of the nearest live layout that declares it, whether or not the server would intercept from here. */
  into?: string;
  /** Whether the navigator fetches the target ahead of a click. */
  prefetch?: PrefetchTiming;
  /** Leaves the click to the browser: a full document load. */
  native?: boolean;
}

/** An `<a>` the navigator reads: `full`, `into`, `prefetch` and `native` ride as `data-sf-*` attributes. */
export function Link({ full, into, prefetch, native, ...rest }: LinkProps): ReactElement {
  const attrs: { [key: string]: unknown } = { ...rest };
  if (full) attrs["data-sf-full"] = "true";
  if (into) attrs["data-sf-into"] = into;
  if (prefetch) attrs["data-sf-prefetch"] = prefetch;
  if (native) attrs["data-sf-native"] = "true";
  return createElement("a", attrs);
}

/** The values the server computed for an island's hoisted expressions, keyed `module|id@i.j`; see `useHoisted`. */
export type Hoisted = { readonly [key: string]: unknown };

const HoistContext = createContext<Hoisted | null>(null);

/** The props key an island's hoisted values arrive under, lifted out before the component sees its props. */
const HOISTED_PROP = "$h";

/** The reader the build binds at the top of a component it rewrote: `r` in place of a render-path call whose inputs are props only, so hydration reads what the server rendered instead of computing it again; `l` around each JSX `.map` callback, so a read inside it knows its iteration. */
export interface HoistReader {
  /** The server's value for hoist `id` at the current loop indices, or `compute()` when it recorded none. */
  r<T>(id: number, compute: () => T): T;
  /** `f` with its index argument pushed onto the loop path while it runs. */
  l<A extends unknown[], R>(f: (...args: A) => R): (...args: A) => R;
  /** The element for a static subtree: `hit` with the server's inner markup for chunk `id` when the table holds it, else `miss`, the original JSX. */
  c(id: number, hit: (html: { __html: string }) => ReactElement, miss: () => ReactElement): ReactElement;
  /** The region key for the island placement `id` at the current loop indices, the same string the server wrote on the region. Placements are numbered apart from the hoists, and marked `i`. */
  k(id: number): string;
}

/** The loop indices a component was rendered under by its callers, so a component placed from a `.map` keys its own hoists below the iteration that placed it. */
const PathContext = createContext<readonly number[]>([]);

/** The reader for the island being rendered, bound to `module`, whose keys are `module|id` or `module|id@i.j` under loops, the callers' loops first. */
export function useHoisted(module: string): HoistReader {
  const table = useContext(HoistContext);
  const base = useContext(PathContext);
  return useMemo(() => {
    const path: number[] = [...base];
    const key = (id: number | string): string => (path.length === 0 ? `${module}|${id}` : `${module}|${id}@${path.join(".")}`);
    return {
      r<T>(id: number, compute: () => T): T {
        if (table === null) return compute();
        const k = key(id);
        return k in table ? (table[k] as T) : compute();
      },
      c(id: number, hit: (html: { __html: string }) => ReactElement, miss: () => ReactElement): ReactElement {
        if (table === null) return miss();
        const k = key(id);
        const html = table[k];
        return typeof html === "string" ? hit({ __html: html }) : miss();
      },
      k(id: number): string {
        return key(`i${id}`);
      },
      l<A extends unknown[], R>(f: (...args: A) => R): (...args: A) => R {
        return (...args: A): R => {
          path.push(typeof args[1] === "number" ? args[1] : -1);
          try {
            const out = f(...args);
            if (isElement(out)) {
              return createElement(PathContext.Provider, { key: out.key, value: [...path] }, out) as R;
            }
            return out;
          } finally {
            path.pop();
          }
        };
      },
    };
  }, [table, module, base]);
}

function isElement(value: unknown): value is ReactElement {
  return typeof value === "object" && value !== null && "$$typeof" in value && "key" in value;
}

/** `element` under the hoisted table `table`, the way the mounter places an island under the table its props carried. */
export function withHoisted(table: Hoisted | null, element: ReactElement): ReactElement {
  return createElement(HoistContext.Provider, { value: table }, element);
}

/** `props` without the hoisted table or the region key, and the table itself. */
function splitHoisted(props: object): [object, Hoisted | null] {
  const { [HOISTED_PROP]: hoisted, [REGION_KEY]: _key, ...rest } = props as { [HOISTED_PROP]?: Hoisted; [REGION_KEY]?: unknown };
  return [rest, hoisted ?? null];
}

/** `element` under this root's region state, with the regions the payload behind a patch describes taken as the current generation. */
function withRegions(el: Element, element: ReactElement, patched: boolean): ReactElement {
  const state = regionsOf(el);
  if (patched) {
    state.sources = (islandState(el)?.regions as Map<string, RegionSource> | null) ?? null;
    state.gen += 1;
  }
  return createElement(RegionsContext.Provider, { value: state }, element);
}

function islandElement(component: unknown, props: object, el: Element, patched: boolean): ReactElement {
  const [own, hoisted] = splitHoisted(props);
  const element = createElement(component as never, { ...own, ...slotPropsFor(el) } as never, childrenFor(el));
  return createElement(Mounting, { el }, withRegions(el, withHoisted(hoisted, element), patched));
}

/** Scans the regions the root around it has built. A root the server did not render copies each region's markup into a fresh element, so an island inside one is a copy nothing has mounted; a scan from here is where it is reached, and `scan` leaves alone whatever is mounted already. Every path through `islandElement` wraps in this, mount, hydrate and patch alike: a root whose child element changed type between renders is torn down and rebuilt, which would lose the DOM a patch exists to keep. */
function Mounting({ el, children }: { el: Element; children: ReactNode }): ReactElement {
  useEffect(() => {
    scan(el);
  });
  return createElement(Fragment, null, children);
}

export const reactMounter: Mounter = (component, props, el, hydrate) => {
  const element = islandElement(component, props, el, false);
  if (hydrate) {
    return hydrateRoot(el, element);
  }
  const root = createRoot(el);
  root.render(element);
  return root;
};

export const reactPatcher: Patcher = (handle, component, props, el) => {
  (handle as Root).render(islandElement(component, props, el, true));
};
