import { patchIsland, type Props } from "./boot.js";
import { refresh } from "./navigator.js";
import { decodeValue, encodeValue, type SfValue } from "./values.js";

/** An island in server mode: the browser holds its props and state, every event round-trips to the server and the markup that comes back is patched into place. No component code runs here. `state` is kept encoded, exactly as the server wrote it, since decoding a double and encoding it again would hand back an integer: JavaScript has one number type and the tag is the only thing that says which this was. */
interface ServerIsland {
  module: string;
  props: { [key: string]: unknown };
  state: unknown;
  pending: boolean;
  listening: Set<string>;
}

/** The props key the initial state arrives under. */
const STATE_PROP = "$s";

const islands = new WeakMap<Element, ServerIsland>();

/** True when `el` is mounted as a server island. */
export function isServerIsland(el: Element): boolean {
  return islands.has(el);
}

/**
 * Mounts `el` as a server island with `encoded`, the props script as the
 * server wrote it, whose `$s` is the state it rendered from. Both are kept
 * encoded and handed back untouched: this browser is a courier for a
 * component that runs on the server and decoding a double here would hand
 * back an integer, since JavaScript has one number type and the tag is the
 * only thing that says which this was. Listens for every event its markup
 * binds.
 */
export function mountServer(el: Element, module: string, encoded: unknown): void {
  const carried = (encoded ?? {}) as { [key: string]: unknown };
  const { [STATE_PROP]: state, ...props } = carried;
  const island: ServerIsland = { module, props, state: state ?? {}, pending: false, listening: new Set() };
  islands.set(el, island);
  // A marker the renderer left empty: the component was placed by something
  // that cannot render it, a template of another tier for one, so the first
  // render is a step of its own. A marker the server filled is left alone.
  if (!el.firstElementChild) {
    void step(el, island, null, null);
    return;
  }
  listen(el, island);
}

/** Gives a mounted server island new props, the way navigation gives a browser island new props: the server renders it again from them and the state it holds, then the markup is patched in. `encoded` is the props as the server wrote them, handed back as they are; props the server never wrote, a page's own, are encoded here. */
export async function patchServer(el: Element, props: Props, encoded?: unknown): Promise<boolean> {
  const island = islands.get(el);
  if (!island) return false;
  const carried = (encoded ?? encodeValue(props as SfValue)) as { [key: string]: unknown };
  const { [STATE_PROP]: state, ...own } = carried;
  island.props = own;
  if (state !== undefined) island.state = state;
  await step(el, island, null, null);
  return true;
}

function eventsBound(el: Element): Set<string> {
  const types = new Set<string>();
  for (const bound of Array.from(el.querySelectorAll("[data-sf-on]"))) {
    for (const pair of (bound.getAttribute("data-sf-on") ?? "").split(" ")) {
      const type = pair.split(":")[0];
      if (type) types.add(type);
    }
  }
  return types;
}

function listen(el: Element, island: ServerIsland): void {
  for (const type of eventsBound(el)) {
    if (island.listening.has(type)) continue;
    island.listening.add(type);
    el.addEventListener(type, (event) => void fire(el, island, type, event));
  }
}

/** The handler bound for `type` on the element the event came from: an index for a lowered component, a name for an island a template rendered. The token is carried as written and the host decides which it is. */
function handlerFor(el: Element, target: EventTarget | null, type: string): string | null {
  if (!(target instanceof Element)) return null;
  const bound = target.closest("[data-sf-on]");
  if (!bound || !el.contains(bound)) return null;
  for (const pair of (bound.getAttribute("data-sf-on") ?? "").split(" ")) {
    const [event, handler] = pair.split(":");
    if (event === type && handler !== undefined) return handler;
  }
  return null;
}

async function fire(el: Element, island: ServerIsland, type: string, event: Event): Promise<void> {
  const handler = handlerFor(el, event.target, type);
  if (handler === null) return;
  if (type === "submit") event.preventDefault();
  if (island.pending) return;
  const target = event.target as { value?: string; checked?: boolean; name?: string } | null;
  const detail = {
    target: { value: target?.value ?? null, checked: target?.checked ?? null, name: target?.name ?? null },
    key: (event as KeyboardEvent).key ?? null,
  };
  await step(el, island, handler, detail as SfValue);
}

async function step(el: Element, island: ServerIsland, handler: string | null, event: SfValue | null): Promise<void> {
  island.pending = true;
  el.setAttribute("data-sf-pending", "");
  try {
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (typeof window !== "undefined") headers["x-sf-from"] = `${window.location.pathname}${window.location.search}`;
    // Everything but the state is encoded here; the state is already the
    // server's own encoding, carried back untouched so a double stays one.
    const named = handler !== null && !/^\d+$/.test(handler);
    const body = JSON.stringify({ props: island.props, state: island.state, handler: handler === null || named ? handler : Number(handler), event: encodeValue(event as SfValue) });
    const res = await fetch(`/_sf/island/${encodeURIComponent(island.module)}`, { method: "POST", headers, body });
    const text = await res.text();
    if (!res.ok) {
      console.warn(`sf: island ${island.module} step failed with ${res.status}: ${text}`);
      return;
    }
    const answer = JSON.parse(text) as { state: unknown; html: string; revalidate?: boolean };
    island.state = answer.state;
    morph(el, answer.html);
    listen(el, island);
    // The handler called an action and the host ran it before answering, so
    // the page's data may have moved: refresh the way a browser-mode call does.
    if (answer.revalidate) await refresh();
  } finally {
    island.pending = false;
    el.removeAttribute("data-sf-pending");
  }
}

/** Patches `el`'s children to match `html`, touching only what differs: text by content, elements by tag and position or by key, attributes by name. An element's key is its `data-sf-key`; an island's region is keyed by the region key the build wrote, so a region that moved takes its mounted island with it. A focused form control keeps its value. A nested island's marker and children are left as they stand; when the props script after it changed, the island mounted there takes the new props. */
export function morph(el: Element, html: string): void {
  const template = document.createElement("template");
  template.innerHTML = html;
  morphNodes(el, Array.from(el.childNodes), Array.from(template.content.childNodes), null, { nested: morphNested });
}

/** What a morph asks of its caller. `nested` settles an island marker the new markup places again, along with the props script after it, which the walk leaves alone. `adopt` answers a keyed new node none of the siblings carries with a node from elsewhere to move in. Null has the new one imported. `drop` is told of each node the walk is about to remove, before it goes. */
export interface MorphHooks {
  nested: (current: Element, next: Element) => void;
  adopt?: (key: string) => Node | null;
  drop?: (node: Node) => void;
}

function keyOf(node: Node): string | null {
  if (!(node instanceof Element)) return null;
  const key = node.getAttribute("data-sf-key");
  if (key !== null) return key;
  const region = node.hasAttribute("data-sf-island") ? node.getAttribute("data-sf-region") : null;
  return region === null ? null : `region:${region}`;
}

function alike(a: Node, b: Node): boolean {
  if (a.nodeType !== b.nodeType) return false;
  if (a instanceof Element && b instanceof Element && a.tagName !== b.tagName) return false;
  return keyOf(a) === keyOf(b);
}

/** Patches `old`, a run of `parent`'s children, to match `fresh` by the rules of `morph`. What is new once the run is used up goes in before `end`. A node of the run that something moved to another parent is no longer part of it. */
export function morphNodes(parent: Node, old: Node[], fresh: Node[], end: Node | null, hooks: MorphHooks): void {
  let i = 0;
  for (const next of fresh) {
    while (i < old.length && old[i].parentNode !== parent) old.splice(i, 1);
    const current = old[i];
    if (current && alike(current, next)) {
      morphNode(current, next, hooks);
      i += 1;
      continue;
    }
    const key = keyOf(next);
    const moved = key === null ? null : (old.slice(i).find((candidate) => candidate.parentNode === parent && keyOf(candidate) === key) ?? hooks.adopt?.(key) ?? null);
    if (moved) {
      parent.insertBefore(moved, current ?? end);
      const at = old.indexOf(moved);
      if (at !== -1) old.splice(at, 1);
      old.splice(i, 0, moved);
      morphNode(moved, next, hooks);
    } else {
      const imported = document.importNode(next, true);
      parent.insertBefore(imported, current ?? end);
      old.splice(i, 0, imported);
    }
    i += 1;
  }
  for (const stale of old.slice(i)) {
    if (stale.parentNode !== parent) continue;
    hooks.drop?.(stale);
    parent.removeChild(stale);
  }
}

/** Patches `current` to match `next`: attributes by name, then children by the rules of `morph`. */
export function morphElement(current: Element, next: Element, hooks: MorphHooks): void {
  morphAttributes(current, next);
  morphNodes(current, Array.from(current.childNodes), Array.from(next.childNodes), null, hooks);
}

function morphNode(current: Node, next: Node, hooks: MorphHooks): void {
  if (current.nodeType === Node.TEXT_NODE || current.nodeType === Node.COMMENT_NODE) {
    if (current.nodeValue !== next.nodeValue) current.nodeValue = next.nodeValue;
    return;
  }
  if (!(current instanceof Element) || !(next instanceof Element)) return;
  if (isPropsScript(current)) return;
  if (current.tagName === "SF-I") {
    hooks.nested(current, next);
    return;
  }
  morphElement(current, next, hooks);
}

function isPropsScript(el: Element): boolean {
  return el.tagName === "SCRIPT" && el.hasAttribute("data-sf-props");
}

/** The marker's id and the client's own marks on it are kept, since the answer numbers its islands from zero and knows nothing of what mounted. Its props script, the sibling after it, takes the answer's text when that changed and the island mounted there takes the props: a server island by a step of its own, any other through `patchIsland`. */
function morphNested(current: Element, next: Element): void {
  const held = current.nextSibling;
  const wanted = next.nextSibling;
  if (!(held instanceof Element) || !(wanted instanceof Element) || !isPropsScript(held) || !isPropsScript(wanted)) return;
  const text = wanted.textContent ?? "";
  if (held.textContent === text) return;
  held.textContent = text;
  const raw = JSON.parse(text) as { [key: string]: unknown };
  const island = islands.get(current);
  if (island) {
    const { [STATE_PROP]: state, ...props } = raw;
    island.props = props;
    if (state !== undefined) island.state = state;
    void step(current, island, null, null);
    return;
  }
  void patchIsland(current, decodeValue(raw as SfValue) as Props);
}

function morphAttributes(current: Element, next: Element): void {
  for (const attr of Array.from(current.attributes)) {
    if (!next.hasAttribute(attr.name)) current.removeAttribute(attr.name);
  }
  for (const attr of Array.from(next.attributes)) {
    if (current.getAttribute(attr.name) !== attr.value) current.setAttribute(attr.name, attr.value);
  }
  const focused = typeof document !== "undefined" && document.activeElement === current;
  if (!focused && "value" in current && "value" in next) {
    const control = current as HTMLInputElement;
    const wanted = next as HTMLInputElement;
    if (control.value !== wanted.value) control.value = wanted.value;
    if ("checked" in wanted && control.checked !== wanted.checked) control.checked = wanted.checked;
  }
}
