import * as ReactModule from "react";
import { cloneElement, createContext, createElement, Fragment, isValidElement, Suspense, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, version, type AnchorHTMLAttributes, type ComponentType, type ImgHTMLAttributes, type ReactElement, type ReactNode } from "react";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import { discard, islandState, MountTiming, Mounter, Patcher, patchIsland, scan, type Pauser, type Props, type Unmounter } from "./boot.js";
import { encodeValue } from "./values.js";
import { CHILDREN_ATTR, MARKUP_ATTR, type RegionSource } from "./render.js";
import { morph } from "./server.js";
import { type PrefetchTiming } from "./navigator.js";
import { linkAttributes, type LinkOptions } from "./link.js";
import { embedParts, styleObject, type EmbedOptions } from "./embed.js";
import { pictureParts, type ImageAsset, type PictureOptions } from "./picture.js";

export type { ImageAsset } from "./picture.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { get, isPending, set, subscribe, whenSettled, type StoreKey } from "./store.js";

/** The store values this root's markup was rendered from, `$sv` on its props: what `useStore` hydrates against, since the live store may have moved on by the time a root hydrates. Null for a root the server did not render. */
const RenderedStoreContext = createContext<{ [key: string]: unknown } | null>(null);

const RENDERED_STORE_PROP = "$sv";

/** The `<sf-s>` a layout renders its child segment into or an island's children render in, `data-sf-children`: the first one under `el` that is not inside a nested island, is not an island's own region and is not a named slot. */
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
  if (slot?.hasAttribute(CHILDREN_ATTR)) props[CHILDREN_ATTR] = "";
  return createElement("sf-s", props);
}

/** The markup an island's children region holds now: what the server wrote, then what each patch gave it. */
const childrenMarkup = new WeakMap<Element, string>();

/** An island's children: an `<sf-s data-sf-children>` whose markup React sets once per instance and never reconciles. A patch morphs the region in place, so an island nested in it keeps its DOM and its state. A component that stops rendering its children and renders them again gets the latest markup. The islands in it are then mounted afresh. */
function IslandChildren({ root }: { root: Element }): ReactElement {
  const [html] = useState(() => childrenMarkup.get(root) ?? "");
  const region = useRef<Element | null>(null);
  useEffect(() => {
    if (region.current) scan(region.current);
  }, []);
  return createElement("sf-s", { ref: region, [CHILDREN_ATTR]: "", dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true });
}

/** The child element a layout or an island with children receives, created once per root. */
const children = new WeakMap<Element, ReactElement>();

/** The inert `<template data-sf-children>` the server writes after an island's markup when its first render did not place its children: under `el` and not inside a nested island. */
function heldChildrenOf(el: Element): Element | null {
  for (const held of Array.from(el.querySelectorAll(`template[${CHILDREN_ATTR}]`))) {
    if (held.parentElement?.closest("sf-i") === el) return held;
  }
  return null;
}

function childrenFor(el: Element): ReactElement | undefined {
  const held = children.get(el);
  if (held) return held;
  const slot = slotOf(el);
  if (!slot) {
    const template = heldChildrenOf(el);
    if (!template) return undefined;
    childrenMarkup.set(el, template.innerHTML);
    template.remove();
    const element = createElement(IslandChildren, { root: el });
    children.set(el, element);
    return element;
  }
  let element: ReactElement;
  if (slot.hasAttribute(CHILDREN_ATTR)) {
    childrenMarkup.set(el, slot.innerHTML);
    element = createElement(IslandChildren, { root: el });
  } else {
    element = adopted(slot);
  }
  children.set(el, element);
  return element;
}

/** Brings an island's children region to the markup a patch gave it: morphed in place while the component renders it, remembered for when it next does. */
function patchChildren(el: Element, fresh: string | null): void {
  if (fresh === null || !childrenMarkup.has(el) || childrenMarkup.get(el) === fresh) return;
  childrenMarkup.set(el, fresh);
  const region = slotOf(el);
  if (!region?.hasAttribute(CHILDREN_ATTR)) return;
  morph(region, fresh);
  scan(region);
}

/** The key of a marker the server puts in an island's props for a markup value. */
const MARKUP_PROP = "$sfm";

/** The markup each markup prop region of a root holds now, by region number: what the server wrote, then what each patch gave it. */
const markupHtml = new WeakMap<Element, Map<number, string>>();

/** The element each markup prop of a root is, created once per region so a re-render never replaces what React set. */
const markupElements = new WeakMap<Element, Map<number, ReactElement>>();

/** A markup prop: an `<sf-s data-sf-m>` whose markup React sets once per instance and never reconciles, scanned for the islands inside it. A patch morphs it in place. */
function IslandMarkup({ root, n }: { root: Element; n: number }): ReactElement {
  const [html] = useState(() => markupHtml.get(root)?.get(n) ?? "");
  const region = useRef<Element | null>(null);
  useEffect(() => {
    if (region.current) scan(region.current);
  }, []);
  return createElement("sf-s", { ref: region, [MARKUP_ATTR]: String(n), dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true });
}

/** The `selector` under `el` that is not inside an island nested in it. */
function ownOf(el: Element, selector: string): Element | null {
  return Array.from(el.querySelectorAll(selector)).find((found) => found.parentElement?.closest("sf-i") === el) ?? null;
}

function markupFor(el: Element, n: number): ReactElement {
  let elements = markupElements.get(el);
  if (!elements) markupElements.set(el, (elements = new Map()));
  const made = elements.get(n);
  if (made) return made;
  let html = markupHtml.get(el);
  if (!html) markupHtml.set(el, (html = new Map()));
  if (!html.has(n)) {
    const placed = ownOf(el, `sf-s[${MARKUP_ATTR}="${n}"]`);
    const held = ownOf(el, `template[${MARKUP_ATTR}="${n}"]`);
    html.set(n, placed?.innerHTML ?? held?.innerHTML ?? "");
    held?.remove();
  }
  const element = createElement(IslandMarkup, { key: `m${n}`, root: el, n });
  elements.set(n, element);
  return element;
}

/** Brings each markup prop region of a root to the markup a patch gave it: morphed in place where the island renders it, remembered for when it next does. */
function patchMarkup(el: Element, fresh: { [n: string]: string } | null): void {
  if (fresh === null) return;
  let html = markupHtml.get(el);
  if (!html) markupHtml.set(el, (html = new Map()));
  for (const [key, markup] of Object.entries(fresh)) {
    const n = Number(key);
    if (html.get(n) === markup) continue;
    html.set(n, markup);
    const region = ownOf(el, `sf-s[${MARKUP_ATTR}="${n}"]`);
    if (region) {
      morph(region, markup);
      scan(region);
    }
  }
}

function isMarker(value: unknown): value is { [MARKUP_PROP]: number } {
  return typeof value === "object" && value !== null && !Array.isArray(value) && Object.keys(value).length === 1 && typeof (value as { [MARKUP_PROP]?: unknown })[MARKUP_PROP] === "number";
}

function holdsMarker(value: unknown): boolean {
  if (isMarker(value)) return true;
  if (Array.isArray(value)) return value.some(holdsMarker);
  if (typeof value === "object" && value !== null && !isElement(value)) return Object.values(value).some(holdsMarker);
  return false;
}

/** `value` with each marker the server put in place of a markup value made the element that adopts its region. */
function withMarkup(value: unknown, el: Element): unknown {
  if (isMarker(value)) return markupFor(el, value[MARKUP_PROP]);
  if (!holdsMarker(value)) return value;
  if (Array.isArray(value)) return value.map((item) => withMarkup(item, el));
  return Object.fromEntries(Object.entries(value as object).map(([k, v]) => [k, withMarkup(v, el)]));
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

/** The island regions of one mounted root: the ones the server rendered, keyed by what it wrote on each, plus what the last payload said about them. Created once per root, so a re-render never reclaims a region another placement already owns. */
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

/** The props key an island's hoisted values arrive under and the region key's own; neither reaches the component. */
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

/** The `sf-i` a region holds, when a scan has taken one there. A marker whose timing has not fired yet counts: the boot runtime owns it either way. */
function rootIn(region: Element): Element | null {
  const first = region.firstElementChild;
  return first?.tagName === "SF-I" && first.hasAttribute("data-sf-scheduled") ? first : null;
}

export interface IslandProps {
  /** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
  when?: MountTiming;
  /** `server`: the island's events round-trip to the server, which re-renders it; no React root is mounted. */
  mode?: "server";
  /** The module that defines the custom element inside, imported at the island's timing rather than at load. The child is then an element, its markup written by the server, with nothing mounted over it. Not combinable with `mode`. */
  define?: string;
  children?: ReactNode;
}

/** Places its one child component as an island of its own: on the server the child renders inside an `<sf-s data-sf-island>` region as a nested island; in the browser this element adopts that region as it stands and never reconciles it, while the boot runtime mounts the child in its own root at the timing asked for. Lowered by the build, so the child is never rendered here.
 *
 * The region is claimed once, by the key the build splices in and after that only the island's own root writes inside it. A re-render hands the mounted root the props the parent just computed; a placement the parent has only now added takes its markup from the payload that added it or renders its child inline when no payload describes one. */
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
      void patchIsland(mounted, source.props as Props, source.nested, source.children, source.encoded);
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

/** Ids for the markers this module writes, which no server rendered. */
let placed = 0;

export interface MountProps {
  /** The module id the registry knows the island under, `src/ui/Chart.vue#default` for one. */
  module: string;
  /** What the island is mounted with, re-applied as a patch when they change. */
  props?: Props;
  when?: MountTiming;
}

/**
 * Places an island by module id rather than by component, for a React tree
 * holding an island another framework mounts: the registry entry decides the
 * mounter, so this writes the marker the boot runtime reads and never renders
 * the child itself. `<Island>` is the one to use for a React child, since it
 * adopts the region the server rendered; nothing rendered this one, so it is
 * mounted fresh and patched from here whenever `props` change.
 */
export function Mount({ module, props = {}, when }: MountProps): ReactElement {
  const region = useRef<Element | null>(null);
  const [id] = useState(() => `sf-m${++placed}`);
  useEffect(() => {
    const host = region.current;
    if (!host) return;
    const mounted = host.querySelector("sf-i");
    if (mounted) {
      void patchIsland(mounted, props);
      return;
    }
    const marker = document.createElement("sf-i");
    marker.id = id;
    marker.setAttribute("data-sf-module", module);
    const script = document.createElement("script");
    script.type = "application/json";
    script.setAttribute("data-sf-props", id);
    script.textContent = JSON.stringify(encodeValue(props as never));
    host.append(marker, script);
    scan(host);
  });
  const attrs: { [key: string]: unknown } = { ref: region, "data-sf-island": "", suppressHydrationWarning: true };
  if (when) attrs["data-sf-when"] = when;
  return createElement("sf-s", attrs);
}

/** The property a `foreign` component carries its module id under. */
export const FOREIGN: unique symbol = Symbol.for("sf.foreign") as never;

/** A component another framework mounts, as a React component placing it by module id through `Mount`. The build writes `const Chart = foreign("src/ui/Chart.vue#default")` in place of a React-JSX file's import of one, so `<Chart />` places that island. Children cannot cross into another framework's component. */
export function foreign<P extends Props>(module: string): ((props: P) => ReactElement) & { [FOREIGN]: string } {
  const Foreign = (props: P): ReactElement => {
    if ((props as { children?: unknown }).children !== undefined) throw new Error(`${module} is mounted by another framework, so it takes no React children`);
    return createElement(Mount, { module, props });
  };
  return Object.assign(Foreign, { [FOREIGN]: module });
}

/** `component` as a component that places it as an island with `options.when` and `options.mode` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`. */
export function island<P extends object>(component: ComponentType<P>, options: { when?: MountTiming; mode?: "server" } = {}): (props: P) => ReactElement {
  return function IslandOf(props: P): ReactElement {
    return createElement(Island, { when: options.when, mode: options.mode }, createElement(component as ComponentType<object>, props));
  };
}

export interface SlotProps {
  /** The slot's name: a `slots/<name>` directory beside the layout or the slot a `page.<name>.tsx` under it renders into. */
  name: string;
  /** What the slot shows while nothing fills it. Rendered by the server, lowered by the build; never rendered here. */
  children?: ReactNode;
}

/** A named slot of a layout: the region a parallel route renders into or an intercepted route opens in. On the server it is `<sf-s data-sf-name>` around the segment or around the fallback children while nothing fills it; in the browser this element adopts the region as it stands and navigation fills and empties it without React reconciling it. */
export function Slot({ name }: SlotProps): ReactElement {
  const regions = useContext(RegionsContext);
  const [html] = useState(() => {
    if (!regions) return "";
    const slot = namedSlotsOf(regions.root).find((s) => s.getAttribute("data-sf-name") === name);
    return slot?.innerHTML ?? "";
  });
  return createElement("sf-s", { "data-sf-name": name, dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true });
}

/** A store key as state: the value the store holds (or `initial` while nothing does) and a setter that writes the store. Every island reading the key re-renders, whichever root it is in. Hydration reads the value the server rendered this island from, which its props carry, so a key another island wrote or a later segment seeded in the meantime moves the island after it hydrates rather than failing it; the build lowers this call, so the key must be a literal or a `key()`. */
export function useStore<T>(k: StoreKey<T>, initial: T): [T, (next: T) => void] {
  const [fallback] = useState(initial);
  const rendered = useContext(RenderedStoreContext);
  const committed = useRef(false);
  useEffect(() => {
    committed.current = true;
  }, []);
  const read = () => {
    const held = get(k);
    return held === undefined ? fallback : held;
  };
  const renderedRead = () => (rendered !== null && k in rendered ? (rendered[k] as T) : fallback);
  const value = useSyncExternalStore((changed: () => void) => subscribe(k, changed), read, renderedRead);
  const setter = useCallback((next: T) => set(k, next), [k]);
  // A root the server did not render, on its first render, waits for a key a
  // deferred segment has yet to seed rather than showing the value it is about
  // to replace. A mounted root keeps what it shows and moves when the seed lands.
  if (rendered === null && !committed.current) suspendOn(k);
  return [value, setter];
}

/** React 19's `use`, which suspends on a promise; React 18 has none and suspends on a thrown one. */
const reactUse = (ReactModule as unknown as { use?: <V>(promise: Promise<V>) => V }).use;

/** The promise a first render waited on for each key, kept once it settles: React replays a render that suspended in `use` and requires the replay to call `use` as often, so the render after the seed lands takes the same promise and goes on. A settled one carries the `status` React reads off a thenable, so `use` returns at once rather than suspending on it. */
const seeds = new Map<string, Promise<void> & { status?: string; value?: undefined }>();

function suspendOn(k: string): void {
  let seed = seeds.get(k);
  if (isPending(k) && (seed === undefined || seed.status === "fulfilled")) {
    const waiting: Promise<void> & { status?: string; value?: undefined } = whenSettled([k]);
    void waiting.then(() => Object.assign(waiting, { status: "fulfilled", value: undefined }));
    seeds.set(k, waiting);
    seed = waiting;
  }
  if (reactUse) {
    if (seed !== undefined) reactUse(seed);
    return;
  }
  if (isPending(k) && seed !== undefined) throw seed;
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
  /** Whether a segment whose key changed but whose module did not is morphed in place, keeping the islands its new markup places again, rather than replaced. Left out, the navigator keeps them when only the query changes. */
  keep?: boolean;
  /** When the link is marked `aria-current`: `"exact"`, the default, on the page its `href` names; `"prefix"` on that page and anything under it; `"none"` never. An `href` carrying a query or a fragment never matches. */
  match?: "exact" | "prefix" | "none";
  /** Which path the mark is judged against: `"url"`, the default, the address bar; `"document"`, the page beneath an open intercept, so a nav describing the section a drawer or a modal opened over stays where it was. The two differ only while an intercept is open. */
  current?: "url" | "document";
}

/** An `<a>` the navigator reads: `full`, `into`, `prefetch`, `native` and `keep` ride as `data-sf-*` attributes, `match` as the `data-sf-link` the navigator re-reads after each navigation and `current` as `data-sf-current` when it is the document's. */
export function Link(props: LinkProps): ReactElement {
  return createElement("a", linkAttributes(props as LinkOptions));
}

export interface PictureProps extends Omit<ImgHTMLAttributes<HTMLImageElement>, "src"> {
  /** An imported image, or a string: a URL as written, or the value a named source's template takes. */
  src: ImageAsset | string;
  /** The `[images.sources]` entry a string `src` goes through. */
  source?: string;
  /** The largest contentful image: loaded eagerly at high priority and preloaded by the server. */
  priority?: boolean;
  /** The widths to offer instead of the policy's; never upscaled past the image's own. */
  widths?: number[];
  /** The quality the build encodes this image at, which changes the bytes and never the markup. */
  quality?: number | { avif?: number; webp?: number };
}

/** React 19 knows `fetchPriority`; under 18 the lowercase attribute passes through as written. */
const FETCH_PRIORITY = Number(version.split(".")[0]) >= 19 ? "fetchPriority" : "fetchpriority";

/** The markup the server wrote for the same props: a `<picture>` with a `<source>` per format and the hashed original as its `<img>`; an `<img>` alone for an image served as it is; for a string `src`, an `<img>` whose `srcset` a named source's template writes. */
export function Picture(props: PictureProps): ReactElement {
  const { img, sources } = pictureParts(props as PictureOptions, FETCH_PRIORITY);
  if (sources === null) return createElement("img", img);
  return createElement("picture", null, ...sources.map((source) => createElement("source", source)), createElement("img", img));
}

/** The placeholder the server writes for a video hosted elsewhere, for a component the browser renders. */
export function Embed(props: EmbedOptions): ReactElement {
  const parts = embedParts(props);
  const react = (attrs: Record<string, string>) => {
    const { style, class: className, ...rest } = attrs;
    return { ...rest, ...(className ? { className } : {}), ...(style ? { style: styleObject(style) } : {}) };
  };
  const face = [...(parts.still ? [createElement("img", { key: "still", ...react(parts.still) })] : []), createElement("span", { key: "title", ...react(parts.title) }, props.title), createElement("span", { key: "play", ...react(parts.play) }, "\u25B6")];
  return createElement("div", react(parts.outer), createElement("button", react(parts.button), ...face));
}

/** The values the server computed for an island's hoisted expressions, keyed `module|id@i.j`; see `useHoisted`. */
export type Hoisted = { readonly [key: string]: unknown };

const HoistContext = createContext<Hoisted | null>(null);

/** The props key an island's hoisted values arrive under, lifted out before the component sees its props. */
const HOISTED_PROP = "$h";

/** The reader the build binds at the top of a component it rewrote: `r` in place of a render-path call whose inputs are props only, so hydration reads what the server rendered instead of computing it again; `l` around each JSX `.map` callback, so a read inside it knows its iteration. */
export interface HoistReader {
  /** The server's value for hoist `id` at the current loop indices or `compute()` when it recorded none. */
  r<T>(id: number, compute: () => T): T;
  /** `f` with its index argument pushed onto the loop path while it runs. */
  l<A extends unknown[], R>(f: (...args: A) => R): (...args: A) => R;
  /** The element for a static subtree: `hit` with the server's inner markup for chunk `id` when the table holds it, else `miss`, the original JSX. */
  c(id: number, hit: (html: { __html: string }) => ReactElement, miss: () => ReactElement): ReactElement;
  /** The region key for the island placement `id` at the current loop indices, the same string the server wrote on the region. Placements are numbered apart from the hoists and marked `i`. */
  k(id: number): string;
  /** `element`, a keyed placement `id` of a component, under a provider whose path is the current one plus `c<id>`, so what that component keys sits below this placement and two placements of it key apart. */
  p(id: number, element: ReactElement): ReactElement;
  /** Enters iteration `index` of a `for…of` or counted `for` the server lowered, pushing it onto the loop path until `x`. */
  e(index: number): void;
  /** Leaves the iteration `e` entered. */
  x(): void;
  /** `value` as the loop body pushes it: an element under a provider holding the current path, as `l` gives a `.map` callback's element; anything else as it is. */
  w<T>(value: T): T;
}

/** The path a component was rendered under by its callers: loop indices and `c<id>` for each keyed placement, so a component placed from a `.map` keys its own hoists below the iteration that placed it. */
const PathContext = createContext<readonly (number | string)[]>([]);

/** A value only the browser knows, the viewer's clock for one, which the server rendered as nothing. The reader answers null until the component has mounted, so the first render matches the server's markup, then computes the value and renders once more. The build wraps each such expression in the browser copy. */
export function useBrowser(): <T>(compute: () => T) => T | null {
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    setMounted(true);
  }, []);
  return useCallback(<T,>(compute: () => T): T | null => (mounted ? compute() : null), [mounted]);
}

/** The reader for the island being rendered, bound to `module`, whose keys are `module|id` or `module|id@i.j` under loops and keyed placements, the callers' first. */
export function useHoisted(module: string): HoistReader {
  const table = useContext(HoistContext);
  const base = useContext(PathContext);
  return useMemo(() => {
    const path: (number | string)[] = [...base];
    const key = (id: number | string): string => (path.length === 0 ? `${module}|${id}` : `${module}|${id}@${path.join(".")}`);
    return {
      r<T>(id: number, compute: () => T): T {
        if (table === null) return compute();
        const k = key(id);
        return k in table ? (table[k] as T) : compute();
      },
      c(id: number, hit: (html: { __html: string }) => ReactElement, miss?: () => ReactElement): ReactElement {
        const k = key(id);
        const html = table === null ? undefined : table[k];
        if (typeof html === "string") return hit({ __html: html });
        if (miss !== undefined) return miss();
        throw new Error(table === null ? `${k} is a subtree the server renders, but this root mounted with no hoisted table: render it through the server or register it as an island` : `${k} is a subtree the server renders and the table it mounted with holds no markup for it`);
      },
      k(id: number): string {
        return key(`i${id}`);
      },
      p(id: number, element: ReactElement): ReactElement {
        return createElement(PathContext.Provider, { key: element.key, value: [...path, `c${id}`] }, element);
      },
      e(index: number): void {
        path.push(index);
      },
      x(): void {
        path.pop();
      },
      w<T>(value: T): T {
        return isElement(value) ? (createElement(PathContext.Provider, { key: value.key, value: [...path] }, value) as T) : value;
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

/** `props` with the hoisted table, the region key and the rendered store values removed, plus the table and the values themselves. */
function splitHoisted(props: object): [object, Hoisted | null, { [key: string]: unknown } | null] {
  const { [HOISTED_PROP]: hoisted, [REGION_KEY]: _key, [RENDERED_STORE_PROP]: rendered, ...rest } = props as { [HOISTED_PROP]?: Hoisted; [REGION_KEY]?: unknown; [RENDERED_STORE_PROP]?: { [key: string]: unknown } };
  return [rest, hoisted ?? null, rendered ?? null];
}

/** `element` under this root's region state, with the regions the payload behind a patch describes taken as the current generation: `sources` when the caller holds them, else what the boot runtime recorded for `el`. */
function withRegions(el: Element, element: ReactElement, patched: boolean, sources?: Map<string, RegionSource> | null): ReactElement {
  const state = regionsOf(el);
  if (patched) {
    state.sources = sources === undefined ? ((islandState(el)?.regions as Map<string, RegionSource> | null) ?? null) : sources;
    state.gen += 1;
  }
  return createElement(RegionsContext.Provider, { value: state }, element);
}

function islandElement(component: unknown, props: object, el: Element, patched: boolean, sources?: Map<string, RegionSource> | null): ReactElement {
  const [own, hoisted, rendered] = splitHoisted(props);
  const element = createElement(component as never, { ...(withMarkup(own, el) as object), ...slotPropsFor(el) } as never, childrenFor(el));
  const under = createElement(RenderedStoreContext.Provider, { value: rendered }, withHoisted(hoisted, element));
  return createElement(Mounting, { el }, withRegions(el, under, patched, sources));
}

/** Scans the regions the root around it has built. A root the server did not render copies each region's markup into a fresh element, so an island inside one is a copy nothing has mounted; a scan from here is where it is reached and `scan` leaves alone whatever is mounted already. Every path through `islandElement` wraps in this, mount, hydrate and patch alike: a root whose child element changed type between renders is torn down and rebuilt, which would lose the DOM a patch exists to keep. */
function Mounting({ el, children }: { el: Element; children?: ReactNode }): ReactElement {
  useEffect(() => {
    scan(el);
  });
  return createElement(Fragment, null, children);
}

/** Roots React rendered fresh, under a `Suspense` with no fallback so `useStore` can wait for a pending key. A root it hydrates has none: the server's markup carries no boundary to hydrate one against. */
const suspending = new WeakSet<Element>();

function rootElement(element: ReactElement, el: Element): ReactElement {
  return suspending.has(el) ? createElement(Suspense, { fallback: null }, element) : element;
}

/** What a hydrated root renders, held outside it, and whether its hydration has committed. React 18 abandons hydration and renders the root from scratch when `render` or `unmount` is called on it before hydration commits, which a navigation patching or discarding an island that is still hydrating does. A patch writes `element` instead and `useSyncExternalStore` applies it: the server's element during hydration, the latest one after. An unmount waits for `committed`. */
interface Held {
  element: ReactElement;
  listeners: Set<() => void>;
  done: boolean;
  committed: Promise<void>;
  commit: () => void;
}

const held = new WeakMap<Element, Held>();
const heldByRoot = new WeakMap<Root, Held>();

function HydratedRoot({ box, initial }: { box: Held; initial: ReactElement }): ReactElement {
  const subscribe = useCallback((listener: () => void) => {
    box.listeners.add(listener);
    return () => box.listeners.delete(listener);
  }, [box]);
  useEffect(() => box.commit(), [box]);
  return useSyncExternalStore(subscribe, () => box.element, () => initial);
}

/** React's `<Activity>`, from 19.2. Every root renders under one, visible until activity holds its page, so hiding it keeps the tree it wraps; a boundary added later would rebuild that tree. */
const Activity = (ReactModule as unknown as { Activity?: ComponentType<{ mode: "visible" | "hidden"; children?: ReactNode }> }).Activity;

/** What each root renders inside its `<Activity>`. */
const inner = new WeakMap<Element, ReactElement>();
/** The roots hidden now. */
const hidden = new WeakSet<Element>();

/** What settles the promise `reactPauser` returned for a root it is showing again, by the root's element. */
const showing = new Map<Element, () => void>();

/** How long `reactPauser` waits for a root it shows again to commit before settling anyway. */
const SHOWN_LIMIT_MS = 1000;

/** Inside the root's `<Activity>`, whose layout effects run again each time it is shown. */
function Shown({ el }: { el: Element }): null {
  useLayoutEffect(() => {
    showing.get(el)?.();
  }, [el]);
  return null;
}

function underActivity(el: Element, element: ReactElement): ReactElement {
  inner.set(el, element);
  return Activity ? createElement(Activity, { mode: hidden.has(el) ? "hidden" : "visible" }, createElement(Fragment, null, element, createElement(Shown, { el }))) : element;
}

/** Brackets the server's markup the way React's own server writes an `<Activity>` boundary, `<!--&-->` and `<!--/&-->`, so a root hydrating under one finds it. */
function markActivity(el: Element): void {
  if (!Activity) return;
  el.insertBefore(el.ownerDocument.createComment("&"), el.firstChild);
  el.appendChild(el.ownerDocument.createComment("/&"));
}

export const reactMounter: Mounter = (component, props, el, hydrate) => {
  const element = islandElement(component, props, el, false);
  hidden.delete(el);
  if (hydrate) {
    let commit = () => {};
    const committed = new Promise<void>((resolve) => (commit = resolve));
    const box: Held = { element, listeners: new Set(), done: false, committed, commit: () => {} };
    box.commit = () => {
      box.done = true;
      commit();
    };
    held.set(el, box);
    markActivity(el);
    const root = hydrateRoot(el, underActivity(el, createElement(HydratedRoot, { box, initial: element })));
    heldByRoot.set(root, box);
    return root;
  }
  held.delete(el);
  suspending.add(el);
  const root = createRoot(el);
  root.render(underActivity(el, rootElement(element, el)));
  return root;
};

export const reactPatcher: Patcher = (handle, component, props, el) => {
  patchChildren(el, islandState(el)?.children ?? null);
  patchMarkup(el, islandState(el)?.markup ?? null);
  const element = rootElement(islandElement(component, props, el, true), el);
  const box = held.get(el);
  if (box === undefined) {
    (handle as Root).render(underActivity(el, element));
    return;
  }
  box.element = element;
  for (const listener of Array.from(box.listeners)) listener();
};

/** Hides or shows a root through its `<Activity>`. False where React has no `Activity`, which arrived in 19.2. False too for a root still hydrating, which a render would make React abandon. */
export const reactPauser: Pauser = (handle, el, paused) => {
  const element = inner.get(el);
  const box = held.get(el);
  if (!Activity || element === undefined || (box !== undefined && !box.done)) return false;
  if (paused) {
    hidden.add(el);
    (handle as Root).render(underActivity(el, element));
    return true;
  }
  hidden.delete(el);
  const shown = new Promise<void>((resolve) => {
    const done = () => {
      showing.delete(el);
      resolve();
    };
    showing.set(el, done);
    setTimeout(done, SHOWN_LIMIT_MS);
  });
  (handle as Root).render(underActivity(el, element));
  return shown;
};

export const reactUnmounter: Unmounter = (handle) => {
  const root = handle as Root;
  const box = heldByRoot.get(root);
  if (box === undefined || box.done) {
    root.unmount();
    return;
  }
  void box.committed.then(() => root.unmount());
};

/** The template dialect's context where an island reaches a template file: the build points that file's import of the dialect here, so its contexts are React's and `use` is `useContext`. */
export { createContext, useContext as use };
