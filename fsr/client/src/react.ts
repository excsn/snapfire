import * as ReactModule from "react";
import { cloneElement, createContext, createElement, Fragment, isValidElement, lazy, Suspense, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, version, type AnchorHTMLAttributes, type ComponentType, type ImgHTMLAttributes, type ReactElement, type ReactNode } from "react";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import { adoptNodes, childrenRegion, discard, hydrationMismatch, release, unmarked, islandState, loadedModule, loadModule, markerProps, markRendered, MountTiming, Mounter, owned, Patcher, patchIsland, piecesOf, readPieces, registeredIslands, rendersNested, scan, serverRendered, timingFired, type IslandEntry, type IslandPiece, type Pauser, type Piece, type Props, type Unmounter } from "./boot.js";
import { encodeValue } from "./values.js";
import { CHILDREN_ATTR, MARKUP_ATTR, type RegionSource } from "./render.js";
import { morph } from "./server.js";
import { pendingLink, subscribePendingLink, type PrefetchTiming } from "./navigator.js";
import { linkAttributes, type LinkOptions } from "./link.js";
import { embedParts, styleObject, type EmbedOptions } from "./embed.js";
import { pictureParts, type ImageAsset, type PictureOptions } from "./picture.js";

export type { ImageAsset } from "./picture.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { get, isPending, set, subscribe, whenSettled, type StoreKey } from "./store.js";

/** The store values this root's markup was rendered from, `$sv` on its props: what `useStore` hydrates against, since the live store may have moved on by the time a root hydrates. Null for a root the server did not render. */
const RenderedStoreContext = createContext<{ [key: string]: unknown } | null>(null);

const RENDERED_STORE_PROP = "$sv";

/** The `<sf-s>` a layout renders its child segment into or an island's children render in, `data-sf-children`: the first one in `el`'s own markup that is not an island's own region and is not a named slot. */
function slotOf(el: Element): Element | null {
  return owned(el, "sf-s:not([data-sf-island]):not([data-sf-name])")[0] ?? null;
}

/** The named slot regions a layout's own markup holds. */
function namedSlotsOf(el: Element): Element[] {
  return owned(el, "sf-s[data-sf-name]");
}

/** An adopted region: `<sf-s>` holding the server's nodes, created once and rendered unchanged, so React takes the region at hydration, takes its nodes when it mounts and never reconciles it. Navigation rewrites what is inside. */
function adopted(slot: Element | null, name?: string): ReactElement {
  return createElement(Adopted, { slot, name });
}

function Adopted({ slot, name }: { slot: Element | null; name?: string }): ReactElement {
  const node = useRef<Element | null>(null);
  const [html] = useState(() => slot?.innerHTML ?? "");
  useLayoutEffect(() => {
    if (node.current) adoptNodes(node.current, slot);
  }, []);
  const props: { [key: string]: unknown } = { ref: node, dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true };
  if (name !== undefined) props["data-sf-name"] = name;
  if (slot?.hasAttribute(CHILDREN_ATTR)) props[CHILDREN_ATTR] = "";
  return createElement("sf-s", props);
}

/** What a children region or a markup prop region shows: the pieces it renders and the markup they were read from, which a patch replaces. */
interface RegionBox {
  pieces: Piece[];
  html: string;
  /** The region the server wrote, whose nodes a mount takes; null for one a patch brought. */
  from: ParentNode | null;
  listeners: Set<() => void>;
}

function boxOf(pieces: Piece[], html: string, from: ParentNode | null = null): RegionBox {
  return { pieces, html, from, listeners: new Set() };
}

function refill(box: RegionBox, html: string): void {
  if (box.html === html) return;
  box.html = html;
  box.pieces = piecesOf(html);
  for (const listener of Array.from(box.listeners)) listener();
}

/** A region of an island's markup that holds what the server placed in it: its children or a markup prop. Pieces that hold a React island render in this tree, that island included; a region that holds none is markup React sets once and a patch morphs. */
function PiecesRegion({ box, attrs }: { box: RegionBox; attrs: { [name: string]: string } }): ReactElement {
  const subscribe = useCallback((listener: () => void) => {
    box.listeners.add(listener);
    return () => box.listeners.delete(listener);
  }, [box]);
  const pieces = useSyncExternalStore(subscribe, () => box.pieces, () => box.pieces);
  const region = useRef<Element | null>(null);
  const tree = pieces.some(holdsOwn);
  useEffect(() => {
    if (tree && region.current) scan(region.current);
  });
  if (!tree) return createElement(Opaque, { key: "opaque", tag: "sf-s", attrs, html: box.html, from: box.from });
  return createElement("sf-s", { key: "tree", ...attrs, ref: region, suppressHydrationWarning: true }, ...nodesOf(pieces));
}

/** An element whose markup React sets once and never reconciles, scanned for the islands inside it. Mounted, it takes the nodes of `from`, the element the server wrote. New markup is morphed in, so an island inside keeps its DOM and its state. Taken away, it releases those islands. */
function Opaque({ tag, attrs, html, from }: { tag: string; attrs: { [prop: string]: unknown }; html: string; from: ParentNode | null }): ReactElement {
  const [first] = useState(() => unmarked(html));
  const shown = useRef(html);
  const node = useRef<Element | null>(null);
  useReleasing(node);
  useLayoutEffect(() => {
    if (node.current) adoptNodes(node.current, from);
  }, []);
  useEffect(() => {
    const el = node.current;
    if (!el) return;
    if (html !== shown.current) {
      const before = shown.current;
      shown.current = html;
      morph(el, html, before);
    }
    scan(el);
  }, [html]);
  return createElement(tag, { ...attrs, ref: node, dangerouslySetInnerHTML: { __html: first }, suppressHydrationWarning: true });
}

/** Whether a piece is or holds an island React renders in this tree. */
function holdsOwn(piece: Piece): boolean {
  if (piece.kind === "island") return piece.entry?.mount === reactMounter;
  return piece.kind === "element" && piece.children.some(holdsOwn);
}

/** HTML attribute names whose React prop is spelled otherwise. */
const PROP_NAMES: { [attr: string]: string } = {
  class: "className",
  for: "htmlFor",
  tabindex: "tabIndex",
  readonly: "readOnly",
  maxlength: "maxLength",
  minlength: "minLength",
  colspan: "colSpan",
  rowspan: "rowSpan",
  autocomplete: "autoComplete",
  autofocus: "autoFocus",
  crossorigin: "crossOrigin",
  contenteditable: "contentEditable",
  enctype: "encType",
  formaction: "formAction",
  formnovalidate: "formNoValidate",
  novalidate: "noValidate",
  srcset: "srcSet",
  srcdoc: "srcDoc",
  "http-equiv": "httpEquiv",
  "accept-charset": "acceptCharset",
  itemprop: "itemProp",
  itemscope: "itemScope",
  itemtype: "itemType",
  usemap: "useMap",
  datetime: "dateTime",
  playsinline: "playsInline",
  referrerpolicy: "referrerPolicy",
  spellcheck: "spellCheck",
  inputmode: "inputMode",
  enterkeyhint: "enterKeyHint",
  fetchpriority: "fetchPriority",
  allowfullscreen: "allowFullScreen",
  autoplay: "autoPlay",
  nomodule: "noModule",
  popovertarget: "popoverTarget",
  "xlink:href": "xlinkHref",
  "xml:lang": "xmlLang",
};

/** HTML attributes React takes as booleans. */
const BOOLEAN_PROPS = new Set(["hidden", "disabled", "selected", "multiple", "readonly", "required", "open", "autofocus", "async", "defer", "novalidate", "formnovalidate", "allowfullscreen", "controls", "loop", "muted", "autoplay", "playsinline", "default", "reversed", "inert", "itemscope", "nomodule"]);

const VOID_TAGS = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);

/** An element's attributes as the props React writes them back with. An `on…` attribute is left out, since React takes a handler and never a string. */
function reactProps(tag: string, attrs: { [name: string]: string }, svg: boolean): { [prop: string]: unknown } {
  const props: { [prop: string]: unknown } = {};
  for (const [name, value] of Object.entries(attrs)) {
    if (name.startsWith("on")) continue;
    if (name === "style") props.style = styleOf(value);
    else if (name === "value" && (tag === "input" || tag === "select" || tag === "textarea")) props.defaultValue = value;
    else if (name === "checked") props.defaultChecked = true;
    else if (BOOLEAN_PROPS.has(name)) props[PROP_NAMES[name] ?? name] = true;
    else if (PROP_NAMES[name]) props[PROP_NAMES[name]] = value;
    else if (svg && name.includes("-") && !name.startsWith("data-") && !name.startsWith("aria-")) props[name.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase())] = value;
    else props[name] = value;
  }
  return props;
}

function styleOf(text: string): { [property: string]: string } {
  const out: { [property: string]: string } = {};
  for (const part of text.split(";")) {
    const at = part.indexOf(":");
    if (at < 0) continue;
    const name = part.slice(0, at).trim();
    out[name.startsWith("--") ? name : name.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase())] = part.slice(at + 1).trim();
  }
  return out;
}

/** Pieces as React nodes: an element on the way to a React island as an element, one holding none as `Opaque`, a React island in this tree and another framework's island as its marker for the boot runtime to mount. A comment is left out, as React's hydration skips one. */
function nodesOf(pieces: Piece[], svg = false): ReactNode[] {
  const out: ReactNode[] = [];
  pieces.forEach((piece, key) => {
    if (piece.kind === "text") out.push(piece.text);
    else if (piece.kind === "island") out.push(piece.entry?.mount === reactMounter ? createElement(NestedIsland, { key: `${key}:${piece.moduleId}`, piece }) : createElement(Foreign, { key: `${key}:${piece.moduleId}`, piece }));
    else if (piece.kind === "element") {
      const inSvg = svg || piece.tag === "svg";
      const props = { ...reactProps(piece.tag, piece.attrs, inSvg), key };
      if (VOID_TAGS.has(piece.tag)) out.push(createElement(piece.tag, props));
      else if (piece.tag === "textarea") out.push(createElement("textarea", { ...props, defaultValue: piece.node.textContent ?? "" }));
      else if (piece.children.some(holdsOwn)) out.push(createElement(piece.tag, props, ...nodesOf(piece.children, inSvg)));
      else out.push(createElement(Opaque, { key, tag: piece.tag, attrs: props, html: piece.node.innerHTML, from: piece.node }));
    }
  });
  return out;
}

/** A marker's attributes as the server wrote them, without the marks the boot runtime adds. */
function markerAttrs(marker: Element): { [name: string]: string } {
  const attrs: { [name: string]: string } = {};
  for (const attr of Array.from(marker.attributes)) {
    if (attr.name !== "data-sf-scheduled" && attr.name !== "data-sf-mounted") attrs[attr.name] = attr.value;
  }
  return attrs;
}

/** An island's props script, rendered back as the server wrote it. */
function propsScript(piece: IslandPiece): ReactElement | null {
  if (!piece.script) return null;
  return createElement("script", { type: "application/json", "data-sf-props": piece.marker.id, dangerouslySetInnerHTML: { __html: piece.script.textContent ?? "" }, suppressHydrationWarning: true });
}

/** Releases the islands inside the element `node` holds when React takes it away or hides it, so their frameworks end them or pause them. They are marked for a scan to mount again if the element comes back. */
function useReleasing(node: { current: Element | null }): void {
  useEffect(() => {
    const el = node.current;
    return () => {
      if (el) release(el);
    };
  }, []);
}

/** Another framework's island among the pieces: its marker with the markup the server wrote, mounted by the boot runtime. New props reach a mounted one as a patch. */
function Foreign({ piece }: { piece: IslandPiece }): ReactElement {
  const node = useRef<Element | null>(null);
  const [html] = useState(() => unmarked(piece.marker.innerHTML));
  useReleasing(node);
  useLayoutEffect(() => {
    if (node.current) adoptNodes(node.current, piece.marker);
  }, []);
  const first = useRef(piece);
  useEffect(() => {
    const el = node.current;
    if (!el || piece === first.current) return;
    first.current = piece;
    if (el.hasAttribute("data-sf-scheduled")) void patchIsland(el, piece.props, null, childrenRegion(piece.marker)?.innerHTML ?? null, piece.encoded);
  }, [piece]);
  return createElement(Fragment, null, createElement("sf-i", { ...markerAttrs(piece.marker), ref: node, dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true }), propsScript(piece));
}

/** A promise a render suspends on, carrying the `status` React reads off a thenable once it settles. */
type Waiting = Promise<void> & { status?: string; value?: undefined };

/** What an island timed later than load waits on, null for one timed on load. Held by the island's own component, which commits around the boundary its body suspends in, so a render that replays or a patch that reads the island again takes the same promise. */
function useTiming(when: MountTiming, find: () => Element | null): Waiting | null {
  const [waiting] = useState<Waiting | null>(() => {
    if (when === "load") return null;
    const made: Waiting = timingFired(when, find);
    void made.then(() => Object.assign(made, { status: "fulfilled", value: undefined }));
    return made;
  });
  return waiting;
}

/** Suspends until `waiting` settles. */
function Timed({ waiting, children }: { waiting: Waiting | null; children?: ReactNode }): ReactNode {
  if (waiting === null) return children;
  if (reactUse) reactUse(waiting);
  else if (waiting.status !== "fulfilled") throw waiting;
  return children;
}

/** The component of a registered React island, loaded as React renders it, by entry. */
const lazies = new WeakMap<IslandEntry, ComponentType<object>>();

function componentFor(entry: IslandEntry): ComponentType<object> {
  const ready = loadedModule(entry);
  if (ready !== undefined) return ready as ComponentType<object>;
  let made = lazies.get(entry);
  if (!made) {
    made = lazy(() => loadModule(entry).then((component) => ({ default: component as ComponentType<object> })));
    lazies.set(entry, made);
  }
  return made;
}

/** An island the server left to the browser: its markup kept as the server wrote it while React hydrates around it, then, once `ready` settles, what `body` renders, with no boundary of its own. */
function ClientOnly({ marker, at, ready, body }: { marker: Element; at: { current: Element | null }; ready: () => Promise<unknown>; body: () => ReactNode }): ReactElement {
  const [html] = useState(marker.innerHTML);
  const [mounted, setMounted] = useState(false);
  useLayoutEffect(() => {
    if (at.current) adoptNodes(at.current, marker);
  }, []);
  useEffect(() => {
    let live = true;
    void ready().then(() => {
      if (live) setMounted(true);
    });
    return () => {
      live = false;
    };
  }, []);
  if (!mounted) return createElement("sf-i", { ...markerAttrs(marker), ref: at, dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true });
  return createElement("sf-i", { ...markerAttrs(marker), ref: at }, body());
}

/** A React island among the pieces, rendered in this tree: the providers around it reach it and an error or a suspension inside it reaches the boundaries above. Its own children render the same way. A server-rendered one timed later than load sits behind a boundary of its own, which the server's markup is bracketed for before hydration. */
function NestedIsland({ piece }: { piece: IslandPiece }): ReactElement {
  const node = useRef<Element | null>(null);
  const at = useCallback(() => node.current, []);
  const box = useMemo(() => {
    if (piece.children === null) return null;
    const region = childrenRegion(piece.marker);
    return boxOf(piece.children, region?.innerHTML ?? "", region);
  }, [piece]);
  const entry = piece.entry as IslandEntry;
  const [component] = useState(() => componentFor(entry));
  const [id] = useState(piece.marker.id);
  const find = () => node.current ?? document.getElementById(id);
  const waiting = useTiming(piece.ssr ? piece.when : "load", find);
  useEffect(() => {
    if (node.current) markRendered(node.current);
  });
  const childrenElement = box === null ? undefined : createElement(PiecesRegion, { box, attrs: { [CHILDREN_ATTR]: "" } });
  const render = () => islandElement(component, piece.props, piece.marker, false, null, { at, children: childrenElement });
  const body = () => createElement(Timed, { waiting }, render());
  const script = propsScript(piece);
  if (!piece.ssr) return createElement(Fragment, null, createElement(ClientOnly, { marker: piece.marker, at: node, ready: () => timingFired(piece.when, find).then(() => loadModule(entry)), body: render }), script);
  const inner = nestedBehindBoundary(piece) ? createElement(Suspense, { fallback: null }, body()) : body();
  return createElement(Fragment, null, createElement("sf-i", { ...markerAttrs(piece.marker), ref: node, suppressHydrationWarning: true }, inner), script);
}

/** Whether a server-rendered React island among the pieces renders behind a boundary of its own: one timed later than load or one whose module has yet to load. */
function nestedBehindBoundary(piece: IslandPiece): boolean {
  return piece.ssr && !(piece.when === "load" && loadedModule(piece.entry as IslandEntry) !== undefined);
}

/** When a React island an island's own markup placed mounts: the placement's timing, else the registry's, else load. */
function placementTiming(marker: Element, when: MountTiming | null | undefined): MountTiming {
  return when ?? registeredIslands().get(marker.getAttribute("data-sf-module") ?? "")?.when ?? "load";
}

/** Whether `region`, a region an island's own markup holds, is one its tree renders in place behind a boundary of its own: a server-rendered React island timed later than load. */
function placedBehindBoundary(region: Element): boolean {
  const marker = region.firstElementChild;
  if (marker?.tagName !== "SF-I" || !serverRendered(marker) || region.hasAttribute("data-sf-mode")) return false;
  if (registeredIslands().get(marker.getAttribute("data-sf-module") ?? "")?.mount !== reactMounter) return false;
  return placementTiming(marker, region.getAttribute("data-sf-when") as MountTiming | null) !== "load";
}

/** Brackets the markup of each React island `el`'s tree renders behind a boundary of its own in the comments React reads as a completed Suspense boundary, so hydration finds the boundary and leaves the markup as it is until the boundary resolves. Those are the islands among its pieces and in the island regions its `Island` placements claim, read from the same list. */
function bracketNested(el: Element, children: Piece[] | null): void {
  const bracket = (marker: Element) => {
    if (!marker.firstChild || marker.firstChild.nodeType === 8) return;
    marker.insertBefore(marker.ownerDocument.createComment("$"), marker.firstChild);
    marker.appendChild(marker.ownerDocument.createComment("/$"));
  };
  const walk = (pieces: Piece[]) => {
    for (const piece of pieces) {
      if (piece.kind === "element") walk(piece.children);
      if (piece.kind !== "island" || piece.entry?.mount !== reactMounter) continue;
      if (nestedBehindBoundary(piece)) bracket(piece.marker);
      if (piece.children) walk(piece.children);
    }
  };
  if (children) walk(children);
  for (const region of regionsOf(el).slots) {
    if (placedBehindBoundary(region)) bracket(region.firstElementChild as Element);
  }
}

/** Each root's children, as boot read them when it mounted the root. */
const childrenBoxes = new WeakMap<Element, RegionBox>();

/** The child element a layout or an island with children receives, created once per root. */
const children = new WeakMap<Element, ReactElement>();

function childrenFor(el: Element): ReactElement | undefined {
  const made = children.get(el);
  if (made) return made;
  const box = childrenBoxes.get(el);
  const slot = box ? null : slotOf(el);
  if (!box && (!slot || slot.hasAttribute(CHILDREN_ATTR))) return undefined;
  const element = box ? createElement(PiecesRegion, { box, attrs: { [CHILDREN_ATTR]: "" } }) : adopted(slot);
  children.set(el, element);
  return element;
}

/** Brings a root's children to the markup a patch gave them. */
function patchChildren(el: Element, fresh: string | null): void {
  const box = childrenBoxes.get(el);
  if (fresh !== null && box) refill(box, fresh);
}

/** The key of a marker the server puts in an island's props for a markup value. */
const MARKUP_PROP = "$sfm";

/** Each markup prop region of a root, by region number: what the server wrote, then what each patch gave it. */
const markupBoxes = new WeakMap<Element, Map<number, RegionBox>>();

/** The element each markup prop of a root is, created once per region so a re-render never replaces what React set. */
const markupElements = new WeakMap<Element, Map<number, ReactElement>>();

/** The first `selector` in `el`'s own markup. */
function ownOf(el: Element, selector: string): Element | null {
  return owned(el, selector)[0] ?? null;
}

function markupFor(el: Element, n: number): ReactElement {
  let elements = markupElements.get(el);
  if (!elements) markupElements.set(el, (elements = new Map()));
  const made = elements.get(n);
  if (made) return made;
  let boxes = markupBoxes.get(el);
  if (!boxes) markupBoxes.set(el, (boxes = new Map()));
  let box = boxes.get(n);
  if (!box) {
    const placed = ownOf(el, `sf-s[${MARKUP_ATTR}="${n}"]`);
    const held = ownOf(el, `template[${MARKUP_ATTR}="${n}"]`) as HTMLTemplateElement | null;
    box = placed ? boxOf(readPieces(placed.childNodes), placed.innerHTML, placed) : held ? boxOf(readPieces(held.content.childNodes), held.innerHTML, held) : boxOf([], "");
    held?.remove();
    boxes.set(n, box);
  }
  const element = createElement(PiecesRegion, { key: `m${n}`, box, attrs: { [MARKUP_ATTR]: String(n) } });
  elements.set(n, element);
  return element;
}

/** Brings each markup prop region of a root to the markup a patch gave it. */
function patchMarkup(el: Element, fresh: { [n: string]: string } | null): void {
  if (fresh === null) return;
  let boxes = markupBoxes.get(el);
  if (!boxes) markupBoxes.set(el, (boxes = new Map()));
  for (const [key, markup] of Object.entries(fresh)) {
    const n = Number(key);
    const box = boxes.get(n);
    if (box) refill(box, markup);
    else boxes.set(n, boxOf(piecesOf(markup), markup));
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
  const slots = owned(el, "sf-s[data-sf-island]");
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
  const [claimed] = useState<{ html: string; inline: boolean; held?: Element }>(() => {
    if (!regions) return { html: "", inline: false };
    // A region belongs to the placement that first took it, for as long as
    // that placement lives. A placement added later takes its markup from the
    // payload that added it, never a region another one is already showing.
    const held = key === null ? regions.slots[regions.next++] : regions.byKey.get(key);
    if (held) {
      if (key !== null) regions.byKey.delete(key);
      return { html: held.innerHTML, inline: false, held };
    }
    return { html: "", inline: key === null || !regions.sources?.has(key) };
  });

  const marker = claimed.held?.firstElementChild;
  const inTree = mode === undefined && marker?.tagName === "SF-I" && registeredIslands().get(marker.getAttribute("data-sf-module") ?? "")?.mount === reactMounter;

  useLayoutEffect(() => {
    if (node.current && !inTree) adoptNodes(node.current, claimed.held ?? null);
  }, []);

  useEffect(() => {
    const region = node.current;
    if (!region || !regions || claimed.inline || inTree) return;
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

  const child = isValidElement(children) ? cloneElement(children, { [KEY_PROP]: undefined } as never) : children;
  if (claimed.inline) return createElement(Fragment, null, child);
  const props: { [key: string]: unknown } = { ref: node, "data-sf-island": "" };
  if (key !== null) props["data-sf-region"] = key;
  if (when) props["data-sf-when"] = when;
  if (mode) props["data-sf-mode"] = mode;
  if (inTree && marker) {
    const source = key !== null ? regions?.sources?.get(key) : undefined;
    return createElement("sf-s", { ...props, suppressHydrationWarning: true }, createElement(PlacedIsland, { marker, when, source, child }));
  }
  return createElement("sf-s", { ...props, dangerouslySetInnerHTML: { __html: claimed.html }, suppressHydrationWarning: true });
}

/** A React island an island's own markup placed, rendered in that island's tree: its props are the ones the parent just computed. What it has of its own (its hoisted table and its store values) comes from the props the server wrote or from the payload behind the latest patch. */
function PlacedIsland({ marker, when, source, child }: { marker: Element; when: MountTiming | undefined; source: RegionSource | undefined; child: ReactNode }): ReactElement {
  const node = useRef<Element | null>(null);
  const at = useCallback(() => node.current, []);
  const [written] = useState(() => markerProps(marker));
  const [script] = useState(() => written.script?.textContent ?? null);
  useEffect(() => {
    if (node.current) markRendered(node.current);
  });
  const timing = placementTiming(marker, when);
  const find = () => node.current ?? marker;
  const waiting = useTiming(serverRendered(marker) ? timing : "load", find);
  const props = (source?.props ?? written.props) as object;
  const render = () => islandScope(createElement(Fragment, null, child), props, marker, false, null, at);
  const body = () => createElement(Timed, { waiting }, render());
  const scriptElement = script === null ? null : createElement("script", { type: "application/json", "data-sf-props": marker.id, dangerouslySetInnerHTML: { __html: script }, suppressHydrationWarning: true });
  if (!serverRendered(marker)) return createElement(Fragment, null, createElement(ClientOnly, { marker, at: node, ready: () => timingFired(timing, find), body: render }), scriptElement);
  const inner = timing === "load" ? body() : createElement(Suspense, { fallback: null }, body());
  return createElement(Fragment, null, createElement("sf-i", { ...markerAttrs(marker), ref: node, suppressHydrationWarning: true }, inner), scriptElement);
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
  const [slot] = useState(() => (regions ? (namedSlotsOf(regions.root).find((s) => s.getAttribute("data-sf-name") === name) ?? null) : null));
  const [html] = useState(() => slot?.innerHTML ?? "");
  const node = useRef<Element | null>(null);
  useLayoutEffect(() => {
    if (node.current) adoptNodes(node.current, slot);
  }, []);
  return createElement("sf-s", { ref: node, "data-sf-name": name, dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true });
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

/** The anchor a `Link` rendered, for `useLinkStatus` in a component inside it. */
const LinkContext = createContext<{ readonly current: Element | null } | null>(null);

/** The element an island is in, for `useLinkStatus` in an island the server placed inside a link. */
const IslandContext = createContext<(() => Element | null) | null>(null);

/** An `<a>` the navigator reads: `full`, `into`, `prefetch`, `native` and `keep` ride as `data-sf-*` attributes, `match` as the `data-sf-link` the navigator re-reads after each navigation and `current` as `data-sf-current` when it is the document's. */
export function Link(props: LinkProps): ReactElement {
  const anchor = useRef<HTMLAnchorElement>(null);
  return createElement(LinkContext.Provider, { value: anchor }, createElement("a", { ...linkAttributes(props as LinkOptions), ref: anchor }));
}

/** Whether the navigation a click on the enclosing link started is still in flight: the `Link` this component renders inside, else the link the server placed this island in. It turns false once the page the link names is shown. Outside a link `pending` is always false. */
export function useLinkStatus(): { pending: boolean } {
  const link = useContext(LinkContext);
  const island = useContext(IslandContext);
  const pending = useSyncExternalStore(
    subscribePendingLink,
    () => {
      const at = pendingLink();
      return at !== null && at === (link ? link.current : (island?.()?.closest("a") ?? null));
    },
    () => false,
  );
  return { pending };
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

export interface PlaceProps {
  /** The place's name, which is the element's id and what `<Teleport to="#name">` names. Letters, digits, `-` and `_`. */
  name: string;
  /** The tag, `div` unless given. */
  as?: string;
  [attribute: string]: unknown;
}

/** The element content placed at `#name` goes in: `<div id="name">` unless `as` names another tag, with the other attributes as given. The server writes what is placed at it inside it. Its children are not React's: empty inner HTML React does not compare makes hydration and every render leave them where they are, in an island as in the shell. */
export function Place({ name, as = "div", ...attrs }: PlaceProps): ReactElement {
  return createElement(as, { ...attrs, id: name, dangerouslySetInnerHTML: EMPTY_HTML, suppressHydrationWarning: true });
}

const EMPTY_HTML = { __html: "" };

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
  /** The id for the `useId` call `id`, `_S_` plus the key plus `_`, which is what the server wrote: the build replaces the call with this. Unique within a root, as React's own is. */
  id(id: number): string;
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
      id(id: number): string {
        return `_S_${key(id)}_`;
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

/** Where an island renders: the element it is in now, which for one rendered in another's tree is known once React has placed it. Also its children element. */
interface Scope {
  at: () => Element | null;
  children: ReactElement | undefined;
}

/** `component` with `props` as the island at `el` renders it, under the providers each island has of its own. `el` is the marker as the server wrote it, which the island's regions are read from. */
function islandElement(component: unknown, props: object, el: Element, patched: boolean, sources?: Map<string, RegionSource> | null, scope: Scope = { at: () => el, children: childrenFor(el) }): ReactElement {
  const [own] = splitHoisted(props);
  const element = createElement(component as never, { ...(withMarkup(own, el) as object), ...slotPropsFor(el) } as never, scope.children);
  return islandScope(element, props, el, patched, sources, scope.at);
}

/** `element` under what an island provides of its own: the element it is in, the store values it was rendered from, its hoisted table and its regions. */
function islandScope(element: ReactElement, props: object, el: Element, patched: boolean, sources: Map<string, RegionSource> | null | undefined, at: () => Element | null): ReactElement {
  const [, hoisted, rendered] = splitHoisted(props);
  const under = createElement(RenderedStoreContext.Provider, { value: rendered }, withHoisted(hoisted, element));
  return createElement(IslandContext.Provider, { value: at }, createElement(Mounting, { at }, withRegions(el, under, patched, sources)));
}

/** Scans the island around it once React has placed it, for the islands in its regions no other island renders. A root the server did not render copies each region's markup into a fresh element, so an island inside one is a copy nothing has mounted; a scan from here is where it is reached and `scan` leaves alone whatever is mounted already. Every path through `islandElement` wraps in this, mount, hydrate and patch alike: a root whose child element changed type between renders is torn down and rebuilt, which would lose the DOM a patch exists to keep. */
function Mounting({ at, children }: { at: () => Element | null; children?: ReactNode }): ReactElement {
  useEffect(() => {
    const el = at();
    if (el) scan(el);
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

export const reactMounter: Mounter = (component, props, el, hydrate, pieces) => {
  const region = childrenRegion(el);
  if (pieces !== null) childrenBoxes.set(el, boxOf(pieces, region?.innerHTML ?? "", region));
  if (region instanceof HTMLTemplateElement) region.remove();
  if (hydrate) bracketNested(el, pieces);
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
    let mismatched = false;
    const onRecoverableError = (error: unknown) => {
      if (!mismatched) hydrationMismatch(el, error);
      mismatched = true;
    };
    const root = hydrateRoot(el, underActivity(el, createElement(HydratedRoot, { box, initial: element })), { onRecoverableError });
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

rendersNested(reactMounter);

/** The template dialect's context where an island reaches a template file: the build points that file's import of the dialect here, so its contexts are React's and `use` is `useContext`. */
export { createContext, useContext as use };
