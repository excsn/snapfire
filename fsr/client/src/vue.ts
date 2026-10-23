import { createApp, createCommentVNode, createSSRApp, createTextVNode, defineAsyncComponent, defineComponent, getCurrentInstance, h, hydrateOnIdle, hydrateOnVisible, inject, onBeforeUnmount, onMounted, onScopeDispose, onUpdated, provide, reactive, ref, shallowRef, watch, type App, type Component, type InjectionKey, type Ref, type VNode } from "vue";

import { childrenRegion, islandState, loadedModule, loadModule, markRendered, patchIsland, piecesOf, release, rendersNested, scan, unmarked, type IslandEntry, type IslandPiece, type MountTiming, type Mounter, type Patcher, type Piece, type Props, type Unmounter } from "./boot.js";
import { CHILDREN_ATTR, PLACED_ISLAND, PLACED_OPEN, REGION_KEY } from "./render.js";
import { morph } from "./server.js";
import { encodeValue } from "./values.js";
import { get, set, subscribe, type StoreKey } from "./store.js";
import { currentLocale, subscribeLocale } from "./locale.js";
import { linkAttributes, type LinkOptions } from "./link.js";
import { pendingLink, subscribePendingLink } from "./navigator.js";
import { pictureParts, type PictureOptions } from "./picture.js";
import { embedParts, type EmbedOptions } from "./embed.js";

/** The reactive props an island was mounted with, so a patch re-renders it in place instead of tearing it down. */
const held = new WeakMap<Element, Record<string, unknown>>();

/** What the server rides on an island's props for the runtime rather than the component: the hoisted table, the region key, a server-mode island's state and the store values the island was rendered from. */
const RUNTIME_PROPS = ["$h", "$k", "$s", "$sv"];

/** What an app's markup was rendered from, provided to it so `useStore` hydrates against it: `values` is `$sv`, the keys the server held, and every other key was rendered from its `initial`. `hydrating` holds only while `app.mount` runs, the pass that hydrates everything loaded, so a component created after it renders from the store. An async component's subtree hydrates after `mount` returns, once its module lands, and is told apart by its vnode carrying the element it hydrates over. */
interface Rendered {
  values: { [key: string]: unknown };
  hydrating: boolean;
}

const RENDERED_STORE: InjectionKey<Rendered> = Symbol("sf-rendered-store");

/** The element an island is in, for `useLinkStatus` in an island the server placed inside a link. */
const ISLAND: InjectionKey<() => Element | null> = Symbol("sf-island");

/** The anchor a `Link` rendered, for `useLinkStatus` in a component inside it. */
const LINK: InjectionKey<Ref<Element | null>> = Symbol("sf-link");

function ownProps(props: Props): Record<string, unknown> {
  const own: Record<string, unknown> = {};
  for (const key of Object.keys(props)) {
    if (!RUNTIME_PROPS.includes(key)) own[key] = props[key];
  }
  return own;
}

/** What each island's children are now: the pieces boot read when it mounted the island, then what each patch gave them. */
const childrenHeld = new WeakMap<Element, Ref<{ pieces: Piece[]; html: string } | null>>();

/** Whether a piece is or holds an island Vue renders in this app. */
function holdsOwn(piece: Piece): boolean {
  if (piece.kind === "island") return piece.entry?.mount === vueMounter;
  return piece.kind === "element" && piece.children.some(holdsOwn);
}

/** A region of an island's markup holding what the server placed in it. Pieces that hold a Vue island render in this app, that island included; a region that holds none is markup Vue sets once and a patch morphs. */
function regionNode(attrs: Record<string, string>, pieces: Piece[], html: string): VNode {
  if (!pieces.some(holdsOwn)) return h(Opaque, { tag: "sf-s", attrs, html });
  return h(Scanned, { tag: "sf-s", attrs }, () => vnodesOf(pieces));
}

/** An element Vue renders the children of, scanned for the islands among them no app renders. */
const Scanned = defineComponent({
  name: "SfScanned",
  props: { tag: { type: String, required: true }, attrs: { type: Object, required: true } },
  setup(props, { slots }) {
    const node = shallowRef<Element | null>(null);
    const rescan = () => {
      if (node.value) scan(node.value);
    };
    onMounted(rescan);
    onUpdated(rescan);
    return () => h(props.tag, { ...(props.attrs as Record<string, string>), ref: node }, slots.default?.());
  },
});

/** An element whose markup Vue sets once and never patches, scanned for the islands inside it. New markup is morphed in, so an island inside keeps its DOM and its state. Taken away, it releases those islands. */
const Opaque = defineComponent({
  name: "SfOpaque",
  props: { tag: { type: String, required: true }, attrs: { type: Object, required: true }, html: { type: String, required: true } },
  setup(props) {
    const node = shallowRef<Element | null>(null);
    const first = unmarked(props.html);
    let shown = props.html;
    onMounted(() => {
      if (node.value) scan(node.value);
    });
    watch(
      () => props.html,
      (html) => {
        const el = node.value;
        if (!el || html === shown) return;
        shown = html;
        morph(el, html);
        scan(el);
      },
      { flush: "post" },
    );
    onBeforeUnmount(() => {
      if (node.value) release(node.value);
    });
    return () => h(props.tag, { ...(props.attrs as Record<string, string>), ref: node, innerHTML: first });
  },
});

const VOID_TAGS = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);

/** Pieces as vnodes: an element on the way to a Vue island as an element, one holding none as `Opaque`, a Vue island in this app and another framework's island as its marker for the boot runtime to mount. */
function vnodesOf(pieces: Piece[]): VNode[] {
  const out: VNode[] = [];
  pieces.forEach((piece, key) => {
    if (piece.kind === "text") out.push(createTextVNode(piece.text));
    else if (piece.kind === "comment") out.push(createCommentVNode(piece.text));
    else if (piece.kind === "island") {
      out.push(h(piece.entry?.mount === vueMounter ? NestedIsland : Foreign, { key: `${key}:${piece.moduleId}`, piece }));
      const script = propsScript(piece);
      if (script) out.push(script);
    }
    else if (VOID_TAGS.has(piece.tag)) out.push(h(piece.tag, { ...piece.attrs, key }));
    else if (piece.children.some(holdsOwn)) out.push(h(piece.tag, { ...piece.attrs, key }, vnodesOf(piece.children)));
    else out.push(h(Opaque, { key, tag: piece.tag, attrs: piece.attrs, html: piece.node.innerHTML }));
  });
  return out;
}

/** A marker's attributes as the server wrote them, without the marks the boot runtime adds. */
function markerAttrs(marker: Element): Record<string, string> {
  const attrs: Record<string, string> = {};
  for (const attr of Array.from(marker.attributes)) {
    if (attr.name !== "data-sf-scheduled" && attr.name !== "data-sf-mounted") attrs[attr.name] = attr.value;
  }
  return attrs;
}

/** An island's props script, rendered back as the server wrote it, beside the island's own component: a component rendering both would be a fragment, which hydrates only between markers the server does not write. */
function propsScript(piece: IslandPiece): VNode | null {
  if (!piece.script) return null;
  return h("script", { type: "application/json", "data-sf-props": piece.marker.id, innerHTML: piece.script.textContent ?? "" });
}

/** Another framework's island among the pieces: its marker with the markup the server wrote, mounted by the boot runtime. New props reach a mounted one as a patch; taken away, it is released. */
const Foreign = defineComponent({
  name: "SfForeign",
  props: { piece: { type: Object as () => IslandPiece, required: true } },
  setup(props) {
    const node = shallowRef<Element | null>(null);
    const html = unmarked(props.piece.marker.innerHTML);
    watch(
      () => props.piece,
      (piece) => {
        const el = node.value;
        if (el?.hasAttribute("data-sf-scheduled")) void patchIsland(el, piece.props, null, childrenRegion(piece.marker)?.innerHTML ?? null, piece.encoded);
      },
    );
    onBeforeUnmount(() => {
      if (node.value) release(node.value);
    });
    return () => h("sf-i", { ...markerAttrs(props.piece.marker), ref: node, innerHTML: html });
  },
});

/** Settles when an island's timing fires: at once on load, once its element is in view or once the main thread is idle. */
function timingFired(when: MountTiming, find: () => Element | null): Promise<void> {
  if (when === "load") return Promise.resolve();
  if (when === "idle") return new Promise((resolve) => ((globalThis as { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback ?? setTimeout)(() => resolve()));
  return new Promise((resolve) => {
    const observe = (el: Element | null) => {
      if (!el || typeof IntersectionObserver === "undefined") return resolve();
      const observer = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer.disconnect();
          resolve();
        }
      });
      observer.observe(el);
    };
    const now = find();
    if (now) observe(now);
    else setTimeout(() => observe(find()));
  });
}

/** The island-scoped values a nested island reads, provided again around it: the element it is in and the store values it was rendered from, hydrating for as long as the app around it is. */
const IslandScope = defineComponent({
  name: "SfIslandScope",
  props: { at: { type: Function, required: true }, values: { type: Object, required: true } },
  setup(props, { slots }) {
    provide(ISLAND, props.at as () => Element | null);
    const outer = inject(RENDERED_STORE, null);
    provide(RENDERED_STORE, {
      values: props.values as { [key: string]: unknown },
      get hydrating() {
        return outer?.hydrating ?? false;
      },
    } as Rendered);
    return () => slots.default?.()[0];
  },
});

/** A Vue island among the pieces, rendered in this app: what the components around it provide reaches it and an error inside it reaches their handlers. Its own children render the same way. One timed later than load is an async component Vue hydrates when that timing fires. One the server left to the browser keeps the server's markup while the app hydrates and mounts after. */
const NestedIsland = defineComponent({
  name: "SfNestedIsland",
  props: { piece: { type: Object as () => IslandPiece, required: true } },
  setup(props) {
    const node = shallowRef<Element | null>(null);
    const id = props.piece.marker.id;
    const find = () => node.value ?? document.getElementById(id);
    const entry = props.piece.entry as IslandEntry;
    const when = props.piece.when;
    const ready = loadedModule(entry);
    const component: Component =
      when === "load" && ready !== undefined
        ? componentOf(ready)
        : defineAsyncComponent({
            loader: () => timingFired(when, find).then(() => loadModule(entry)).then(componentOf),
            hydrate: when === "visible" ? hydrateOnVisible() : when === "idle" ? hydrateOnIdle() : undefined,
            suspensible: false,
          });
    const rendered = inject(RENDERED_STORE, null);
    const hydrating = rendered !== null && (rendered.hydrating || getCurrentInstance()?.vnode.el != null);
    const live = ref(props.piece.ssr || !hydrating);
    const html = props.piece.marker.innerHTML;
    onMounted(() => {
      if (node.value) markRendered(node.value);
      live.value = true;
    });
    onUpdated(() => {
      if (node.value) markRendered(node.value);
    });
    return () => {
      const piece = props.piece;
      const attrs = { ...markerAttrs(piece.marker), ref: node };
      if (!live.value) return h("sf-i", { ...attrs, innerHTML: html });
      const values = (piece.props as { $sv?: { [key: string]: unknown } }).$sv ?? {};
      const slots = piece.children === null ? undefined : { default: () => regionNode({ [CHILDREN_ATTR]: "" }, piece.children ?? [], childrenRegion(piece.marker)?.innerHTML ?? "") };
      const body = h(IslandScope, { at: find, values }, () => h(component, ownProps(piece.props), slots));
      return h("sf-i", attrs, [body]);
    };
  },
});

/**
 * Vue mounts a component through an app and an app takes its root props once.
 * A one-element wrapper holding them reactively is what makes a patch possible:
 * it renders nothing of its own, so the markup Vue hydrates is the component's
 * and nothing else. The held children region is taken out before the app
 * mounts, since mounting empties `el`.
 */
function rootFor(component: Component, props: Props, el: Element, pieces: Piece[] | null): { root: Component; props: Record<string, unknown>; children: Ref<{ pieces: Piece[]; html: string } | null> } {
  const state = reactive(ownProps(props));
  const region = childrenRegion(el);
  const children = shallowRef<{ pieces: Piece[]; html: string } | null>(pieces === null ? null : { pieces, html: region?.innerHTML ?? "" });
  if (region?.tagName === "TEMPLATE") region.remove();
  const root = defineComponent({
    name: "SfIsland",
    setup() {
      return () => {
        const held = children.value;
        return h(component, state, held === null ? undefined : { default: () => regionNode({ [CHILDREN_ATTR]: "" }, held.pieces, held.html) });
      };
    },
  });
  return { root, props: state, children };
}

/** The default export of a module or the module when it is the component itself. */
function componentOf(module: unknown): Component {
  const holder = module as { default?: Component };
  return (holder && holder.default) || (module as Component);
}

/** Vue hydrates a teleport's content from its target's `_lpa`, the node after what the teleport before it hydrated, else from the target's first child. That node has to be the content's start anchor. The content an island placed is one run per place, opened by a template naming the island by its region key or else its id, so each place's cursor starts at the island's run there before the island hydrates. The body's is cleared when the island placed nothing there. */
function startTeleports(el: Element, props: Props): void {
  const name = typeof props[REGION_KEY] === "string" ? props[REGION_KEY] : el.id;
  type Target = Node & { _lpa?: ChildNode | null };
  const set = new Set<Node>();
  for (const open of Array.from(document.querySelectorAll(`template[${PLACED_OPEN}]`))) {
    const target = open.parentNode;
    if (!target || set.has(target) || open.getAttribute(PLACED_ISLAND) !== name) continue;
    (target as Target)._lpa = open.nextSibling;
    set.add(target);
  }
  if (!set.has(document.body)) (document.body as Target)._lpa = null;
}

export const vueMounter: Mounter = (module, props, el, hydrate, pieces) => {
  const { root, props: state, children } = rootFor(componentOf(module), props, el, pieces);
  const app: App = hydrate ? createSSRApp(root) : createApp(root);
  const rendered: Rendered = { values: (props as { $sv?: { [key: string]: unknown } }).$sv ?? {}, hydrating: hydrate };
  app.provide(RENDERED_STORE, rendered);
  app.provide(ISLAND, () => el);
  held.set(el, state);
  childrenHeld.set(el, children);
  if (hydrate) startTeleports(el, props);
  app.mount(el);
  rendered.hydrating = false;
  return app;
};

export const vueUnmounter: Unmounter = (handle, el) => {
  (handle as App).unmount();
  held.delete(el);
  childrenHeld.delete(el);
};

export const vuePatcher: Patcher = (handle, module, props, el) => {
  const state = held.get(el);
  if (!state) return;
  const next = ownProps(props);
  for (const key of Object.keys(state)) {
    if (!(key in next)) delete state[key];
  }
  Object.assign(state, next);
  const fresh = islandState(el)?.children ?? null;
  const children = childrenHeld.get(el);
  if (fresh !== null && children?.value && children.value.html !== fresh) children.value = { pieces: piecesOf(fresh), html: fresh };
};

rendersNested(vueMounter);

/** The neutral store as a Vue ref: `const region = useStore(regionKey, "all")`, readable and writable, following every other island that shares the key. A hydrating island starts from the value the server rendered it from, which its props carry, or from `initial` for a key the server held none of, and moves to the store's once mounted, so a key that moved in the meantime never fails the hydration. Call it in `setup`, so the subscription ends with the component. */
export function useStore<T>(key: StoreKey<T>, initial: T): { value: T } {
  const rendered = inject(RENDERED_STORE, null);
  const hydrating = rendered !== null && (rendered.hydrating || getCurrentInstance()?.vnode.el != null);
  const values = hydrating ? rendered.values : null;
  const live = () => get(key) ?? initial;
  const state = reactive({ value: values === null ? live() : key in values ? (values[key] as T) : initial }) as { value: T };
  let ours = false;
  const off = subscribe(key, (next) => {
    if (ours) return;
    state.value = next as T;
  });
  onScopeDispose(off);
  if (values !== null) {
    onMounted(() => {
      const now = live();
      if (!Object.is(state.value, now)) state.value = now;
    });
  }
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

/** The document's locale as a Vue ref, following every navigation that changes it. Call it in `setup`, so the subscription ends with the component. */
export function useLocale(): Readonly<Ref<string>> {
  const locale = ref(currentLocale());
  onScopeDispose(subscribeLocale((next) => {
    locale.value = next;
  }));
  return locale;
}

/** Whether the navigation a click on the enclosing link started is still in flight, as a reactive `{ pending }`: the `Link` this component renders inside, else the link the server placed this island in. It turns false once the page the link names is shown. Call it in `setup`, so the subscription ends with the component. */
export function useLinkStatus(): { readonly pending: boolean } {
  const link = inject(LINK, null);
  const island = inject(ISLAND, null);
  const status = reactive({ pending: false });
  const update = () => {
    const at = pendingLink();
    status.pending = at !== null && at === (link ? link.value : (island?.()?.closest("a") ?? null));
  };
  update();
  onScopeDispose(subscribePendingLink(update));
  return status;
}

/** Ids for the markers this module writes, which no server rendered. */
let placed = 0;

/** What [`Mount`] takes: the module id the registry knows the island under, the props it is mounted with and re-patched from, plus the timing that schedules it. */
export interface MountProps {
  module: string;
  props?: Props;
  when?: MountTiming;
}

/** The element content placed at `#name` goes in: `<div id="name">` unless `as` names another tag, with the other attributes falling through. The server writes what is placed at it inside it. Vue leaves an element with no children of its own as it finds it, in an island as in the shell. */
export const Place: Component = defineComponent({
  name: "SfPlace",
  props: {
    name: { type: String, required: true },
    as: { type: String, default: "div" },
  },
  setup(props) {
    return () => h(props.as, { id: props.name });
  },
});

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

/** An anchor with the marks the navigator reads, for a Vue island. Attributes pass through as `<a>`'s. */
export const Link: Component = defineComponent({
  name: "SfLink",
  inheritAttrs: false,
  setup(_, { attrs, slots }) {
    const anchor = ref<Element | null>(null);
    provide(LINK, anchor);
    return () => h("a", { ...linkAttributes(attrs as LinkOptions), ref: anchor }, slots.default?.());
  },
});

/** The placeholder the server writes for a video hosted elsewhere, for a component the browser renders. */
export const Embed: Component = defineComponent({
  name: "SfEmbed",
  inheritAttrs: false,
  setup(_, { attrs }) {
    return () => {
      const props = attrs as unknown as EmbedOptions;
      const parts = embedParts(props);
      const face = [...(parts.still ? [h("img", parts.still)] : []), h("span", parts.title, props.title), h("span", parts.play, "\u25B6")];
      return h("div", parts.outer, [h("button", parts.button, face)]);
    };
  },
});

/** The `<picture>` the server writes for the same attributes, for a Vue island. */
export const Picture: Component = defineComponent({
  name: "SfPicture",
  inheritAttrs: false,
  setup(_, { attrs }) {
    return () => {
      const { img, sources } = pictureParts(attrs as PictureOptions);
      if (sources === null) return h("img", img);
      return h("picture", null, [...sources.map((source) => h("source", source)), h("img", img)]);
    };
  },
});
