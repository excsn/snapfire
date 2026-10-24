import { getAllContexts, getContext, hydrate, mount as mountSvelte, setContext, unmount, type Component, type Snippet } from "svelte";
import { createSubscriber } from "svelte/reactivity";

import { discard, hydrationMismatch, islandState, loadedModule, loadModule, markRendered, patchIsland, piecesOf, rendersNested, scan, timingFired, unmarked, type IslandEntry, type IslandPiece, type MountTiming, type Mounter, type Patcher, type Piece, type Props, type Unmounter } from "./boot.js";
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

/** One node of an island's children as `Pieces.svelte` renders it in the island's tree. */
export type TreePiece = TreeIsland | TreeElement | TreeOpaque | TreeHtml;

/** A Svelte island rendered in the tree: hydrated where the server rendered it, else rendered once its timing fires. */
export interface TreeIsland {
  kind: "island";
  id: string;
  moduleId: string;
  region: string | null;
  when: string | null;
  mode: string | null;
  ssr: boolean;
  props: Props;
  children: TreePiece[] | null;
  entry: IslandEntry | undefined;
  timing: MountTiming;
}

/** An element holding a Svelte island of the tree, rebuilt around it. */
export interface TreeElement {
  kind: "element";
  tag: string;
  attrs: { [name: string]: string };
  children: TreePiece[];
}

/** An element whose content Svelte never renders: kept as the server wrote it and morphed when a patch changes it. */
export interface TreeOpaque {
  kind: "opaque";
  tag: string;
  attrs: { [name: string]: string };
  html: string;
}

/** A run of text and comments. */
export interface TreeHtml {
  kind: "html";
  html: string;
}

/** A mounted island's props and children, so a patch updates both in place. */
interface Held {
  props: { [key: string]: unknown };
  pieces: TreePiece[] | null;
  changed: () => void;
  track: () => void;
}

const states = new WeakMap<Element, Held>();

function ownProps(props: Props): { [key: string]: unknown } {
  const own: { [key: string]: unknown } = {};
  for (const key of Object.keys(props)) {
    if (!RUNTIME_PROPS.includes(key)) own[key] = props[key];
  }
  return own;
}

/** The default export of a module or the module when it is the component itself. */
function componentOf(module: unknown): Component<{ [key: string]: unknown }> | null {
  const holder = module as { default?: Component<{ [key: string]: unknown }> } | undefined;
  return (holder && holder.default) || ((module as Component<{ [key: string]: unknown }> | undefined) ?? null);
}

/** The element an island is mounted in, as a getter, for `useLinkStatus` in an island the server placed inside a link. */
const ISLAND = Symbol("sf-island");

/** The children `Pieces.svelte` renders where it is given none: those of the island around it. */
const PIECES = Symbol("sf-pieces");

/** The contexts around each element `Pieces.svelte` keeps as the server wrote it, for a Svelte island inside one that hydrates later as a root of its own. */
const scopes = new WeakMap<Element, Map<unknown, unknown>>();

/** The `children` snippet `Pieces.svelte` exports, which the build's island registry hands over. */
let childrenTree: Snippet | null = null;

/** Takes the snippet that renders an island's children in its tree, the one `Pieces.svelte` exports. Called by the island registry the build writes. */
export function registerTree(tree: Snippet): void {
  childrenTree = tree;
}

function isMark(piece: Piece | undefined, text?: string): boolean {
  return piece?.kind === "comment" && (text === undefined || piece.text === text);
}

/** Children as the tree renders them: by the markers the server wrote around them, else by what each holds. */
function treeOf(pieces: Piece[]): TreePiece[] {
  return isMark(pieces[0], "[") ? marked(pieces) : plain(pieces);
}

function marked(pieces: Piece[]): TreePiece[] {
  const out: TreePiece[] = [];
  let at = 1;
  while (at < pieces.length && !isMark(pieces[at], "]")) {
    const branch = pieces[at].kind === "comment" ? (pieces[at] as { text: string }).text : "";
    const inner: Piece[] = [];
    let depth = 0;
    for (at++; at < pieces.length; at++) {
      const piece = pieces[at];
      if (piece.kind === "comment" && piece.text.startsWith("[")) depth++;
      if (piece.kind === "comment" && piece.text === "]" && depth-- === 0) break;
      inner.push(piece);
    }
    at++;
    out.push(item(branch, inner));
  }
  return out;
}

function item(branch: string, inner: Piece[]): TreePiece {
  const element = inner.find((piece) => piece.kind === "element");
  if (element?.kind === "element") {
    const island = element.children.find((piece): piece is IslandPiece => piece.kind === "island");
    if (branch === "[0" && island) return treeIsland(island, element.attrs);
    if (branch === "[1") return { kind: "element", tag: element.tag, attrs: element.attrs, children: treeOf(element.children) };
    if (branch === "[2") return { kind: "opaque", tag: element.tag, attrs: element.attrs, html: unmarked(element.node.innerHTML) };
  }
  return { kind: "html", html: serialised(inner.slice(1, -1)) };
}

function treeIsland(island: IslandPiece, around: { [name: string]: string }): TreeIsland {
  const timing = (around["data-sf-when"] ?? island.entry?.when ?? "load") as MountTiming;
  return {
    kind: "island",
    id: island.marker.id,
    moduleId: island.moduleId,
    region: around["data-sf-region"] ?? null,
    when: around["data-sf-when"] ?? null,
    mode: around["data-sf-mode"] ?? null,
    ssr: island.ssr,
    props: island.props,
    children: island.children ? treeOf(island.children) : null,
    entry: island.entry,
    timing,
  };
}

/** Whether the tree renders this island rather than leaving it to a root of its own. */
function inTree(island: IslandPiece, around: { [name: string]: string }): boolean {
  const timing = around["data-sf-when"] ?? island.entry?.when ?? "load";
  return island.entry?.mount === svelteMounter && around["data-sf-mode"] === undefined && !(island.ssr && timing !== "load");
}

function holdsIsland(pieces: Piece[]): boolean {
  return pieces.some((piece) => piece.kind === "element" && (wrapped(piece) !== null || holdsIsland(piece.children)));
}

function wrapped(piece: Piece): IslandPiece | null {
  if (piece.kind !== "element" || piece.tag !== "sf-s" || !("data-sf-island" in piece.attrs)) return null;
  const island = piece.children.find((child): child is IslandPiece => child.kind === "island");
  return island && inTree(island, piece.attrs) ? island : null;
}

/** Children no marker describes, laid out by what each piece holds. */
function plain(pieces: Piece[]): TreePiece[] {
  const out: TreePiece[] = [];
  let run: Piece[] = [];
  const flush = () => {
    if (run.length) out.push({ kind: "html", html: serialised(run) });
    run = [];
  };
  for (const piece of pieces) {
    if (piece.kind !== "element") {
      run.push(piece);
      continue;
    }
    flush();
    const island = wrapped(piece);
    if (island) out.push(treeIsland(island, piece.attrs));
    else if (holdsIsland(piece.children)) out.push({ kind: "element", tag: piece.tag, attrs: piece.attrs, children: plain(piece.children) });
    else out.push({ kind: "opaque", tag: piece.tag, attrs: piece.attrs, html: unmarked(piece.node.innerHTML) });
  }
  flush();
  return out;
}

function serialised(pieces: Piece[]): string {
  return pieces
    .map((piece) => {
      switch (piece.kind) {
        case "text":
          return piece.text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
        case "comment":
          return `<!--${piece.text}-->`;
        case "element":
          return unmarked(piece.node.outerHTML);
        case "island":
          return unmarked(piece.marker.outerHTML + (piece.script?.outerHTML ?? ""));
      }
    })
    .join("");
}

/** What `Pieces.svelte` lists: the pieces it was given, else the children of the island around it. */
export function held(given: () => TreePiece[] | undefined): () => TreePiece[] {
  const around = getContext<{ read: () => TreePiece[] } | undefined>(PIECES);
  return () => given() ?? around?.read() ?? [];
}

/** The attachment `Pieces.svelte` puts on an element it keeps as the server wrote it: the content stays while hydrating, is written on a fresh render and morphed when a patch changes it. The islands inside it are scanned and ended once the element leaves the document. */
export function opaque(): (piece: TreeOpaque) => (el: Element) => () => void {
  const contexts = getAllContexts();
  return (piece) => (el) => {
    scopes.set(el, contexts);
    if (unmarked(el.innerHTML) !== piece.html) morph(el, piece.html);
    scan(el);
    return () => {
      queueMicrotask(() => {
        if (!el.isConnected) {
          scopes.delete(el);
          discard(el);
        }
      });
    };
  };
}

/** What `Island.svelte` renders a Svelte island of the tree with: its props and its component, loaded already when the server rendered it, else once its timing fires. Sets the contexts the island's own children and `useLinkStatus` read. */
export function nested(piece: () => TreeIsland): { readonly component: Component<{ [key: string]: unknown }> | null; readonly props: { [key: string]: unknown }; attach: (el: Element) => () => void } {
  const first = piece();
  let marker: Element | null = null;
  setContext(ISLAND, () => marker);
  setContext(PIECES, { read: () => piece().children ?? [] });
  let current = first.ssr && first.entry ? componentOf(loadedModule(first.entry)) : null;
  let changed = () => {};
  const subscribed = createSubscriber((update) => {
    changed = update;
    return () => {
      changed = () => {};
    };
  });
  return {
    get component() {
      subscribed();
      return current;
    },
    get props() {
      return ownProps(piece().props);
    },
    attach: (el) => {
      marker = el;
      markRendered(el);
      let live = true;
      const entry = first.entry;
      if (!current && entry) {
        void timingFired(first.timing, () => marker)
          .then(() => loadedModule(entry) ?? loadModule(entry))
          .then((mod) => {
            if (!live) return;
            current = componentOf(mod);
            changed();
          });
      }
      return () => {
        live = false;
        if (marker === el) marker = null;
      };
    },
  };
}

/** The contexts of the tree an island sits in when it is a root of its own inside an element that tree keeps as the server wrote it. */
function contextsAbove(el: Element): Map<unknown, unknown> {
  for (let at = el.parentElement; at; at = at.parentElement) {
    const found = scopes.get(at);
    if (found) return found;
  }
  return new Map();
}

/**
 * The props a component is mounted with: Svelte reads a prop from this object
 * each time it uses it, so a read that subscribes makes a patch re-render
 * what reads the prop and keeps the component's own `$state`. `children` is
 * the tree's snippet when the server placed children.
 */
function liveProps(state: Held): { [key: string]: unknown } {
  const read = (key: string | symbol) => {
    state.track();
    if (key === "children" && state.pieces !== null) return childrenTree ?? undefined;
    return typeof key === "string" ? state.props[key] : undefined;
  };
  const keys = () => [...Object.keys(state.props), ...(state.pieces !== null && childrenTree ? ["children"] : [])];
  return new Proxy(
    {},
    {
      get: (_, key) => read(key),
      has: (_, key) => {
        state.track();
        return typeof key === "string" && keys().includes(key);
      },
      ownKeys: () => {
        state.track();
        return keys();
      },
      getOwnPropertyDescriptor: (_, key) => ({ configurable: true, enumerable: true, value: read(key) }),
    },
  );
}

export const svelteMounter: Mounter = (module, props, el, isHydrating, children) => {
  const state: Held = { props: ownProps(props), pieces: children ? treeOf(children) : null, changed: () => {}, track: () => {} };
  state.track = createSubscriber((update) => {
    state.changed = update;
    return () => {
      state.changed = () => {};
    };
  });
  states.set(el, state);
  const context = new Map<unknown, unknown>([...contextsAbove(el), [ISLAND, () => el], [PIECES, { read: () => (state.track(), state.pieces ?? []) }]]);
  const options = { target: el, props: liveProps(state), context };
  const component = componentOf(module) as Component<{ [key: string]: unknown }>;
  if (isHydrating) {
    try {
      return hydrate(component, { ...options, recover: false });
    } catch (error) {
      if (!(error instanceof Error) || !error.message.includes("hydration_failed")) throw error;
      hydrationMismatch(el, error);
    }
  }
  el.replaceChildren();
  return mountSvelte(component, options);
};

/** A Svelte island of the tree is one the server wrote in the branch of `Pieces.svelte` that renders it, which a later timing never is: that one hydrates as a root of its own. */
rendersNested(svelteMounter, (el) => {
  const before = el.parentElement?.previousSibling;
  return before?.nodeType === 8 && (before as Comment).data === "[0";
});

export const sveltePatcher: Patcher = (_handle, _module, props, el) => {
  const state = states.get(el);
  if (!state) return;
  state.props = ownProps(props);
  const fresh = islandState(el)?.children ?? null;
  if (fresh !== null) state.pieces = treeOf(piecesOf(fresh));
  state.changed();
};

export const svelteUnmounter: Unmounter = (handle, el) => {
  void unmount(handle as Record<string, unknown>);
  states.delete(el);
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

/** The anchor FSR's `Link` component rendered, for `useLinkStatus` in a component inside it. */
const LINK = Symbol("sf-link");

/** What FSR's `Link` component calls as it initialises, so `useLinkStatus` in a component inside it reads the anchor it rendered. */
export function holdLink(anchor: () => Element | null): void {
  setContext(LINK, anchor);
}

/** Whether the navigation a click on the enclosing link started is still in flight, as a Svelte store of `{ pending }`: FSR's `Link` this component renders inside, else the link the server placed this island in. It turns false once the page the link names is shown. Call it while the component initialises. */
export function useLinkStatus(): { subscribe: (run: (value: { pending: boolean }) => void) => () => void } {
  const link = getContext<(() => Element | null) | undefined>(LINK);
  const island = getContext<(() => Element | null) | undefined>(ISLAND);
  const pending = () => {
    const at = pendingLink();
    return at !== null && at === (link ? link() : (island?.()?.closest("a") ?? null));
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

