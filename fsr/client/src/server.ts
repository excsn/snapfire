import type { Props } from "./boot.js";
import { decodeValue, encodeValue, type SfValue } from "./values.js";

/** An island in server mode: the browser holds its props and state, every event round-trips to the server, and the markup that comes back is patched into place. No component code runs here. */
interface ServerIsland {
  module: string;
  props: Props;
  state: SfValue;
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

/** Mounts `el` as a server island with `props`, whose `$s` is the state the server rendered from. Listens for every event its markup binds. */
export function mountServer(el: Element, module: string, props: Props): void {
  const { [STATE_PROP]: state, ...own } = props;
  const island: ServerIsland = { module, props: own as Props, state: (state ?? {}) as SfValue, pending: false, listening: new Set() };
  islands.set(el, island);
  listen(el, island);
}

/** Gives a mounted server island new props, the way navigation gives a browser island new props: the server renders it again from them and the state it holds, and the markup is patched in. */
export async function patchServer(el: Element, props: Props): Promise<boolean> {
  const island = islands.get(el);
  if (!island) return false;
  const { [STATE_PROP]: state, ...own } = props;
  island.props = own as Props;
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

function handlerFor(el: Element, target: EventTarget | null, type: string): number | null {
  if (!(target instanceof Element)) return null;
  const bound = target.closest("[data-sf-on]");
  if (!bound || !el.contains(bound)) return null;
  for (const pair of (bound.getAttribute("data-sf-on") ?? "").split(" ")) {
    const [event, index] = pair.split(":");
    if (event === type && index !== undefined) return Number(index);
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

async function step(el: Element, island: ServerIsland, handler: number | null, event: SfValue | null): Promise<void> {
  island.pending = true;
  el.setAttribute("data-sf-pending", "");
  try {
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (typeof window !== "undefined") headers["x-sf-from"] = `${window.location.pathname}${window.location.search}`;
    const body = JSON.stringify(encodeValue({ props: island.props, state: island.state, handler, event } as SfValue));
    const res = await fetch(`/_sf/island/${encodeURIComponent(island.module)}`, { method: "POST", headers, body });
    const text = await res.text();
    if (!res.ok) {
      console.warn(`sf: island ${island.module} step failed with ${res.status}: ${text}`);
      return;
    }
    const answer = decodeValue(JSON.parse(text)) as { state: SfValue; html: string };
    island.state = answer.state;
    morph(el, answer.html);
    listen(el, island);
  } finally {
    island.pending = false;
    el.removeAttribute("data-sf-pending");
  }
}

/** Patches `el`'s children to match `html`, touching only what differs: text by content, elements by tag and position or by `data-sf-key`, attributes by name. A focused form control keeps its value. A nested island is left as it stands. */
export function morph(el: Element, html: string): void {
  const template = document.createElement("template");
  template.innerHTML = html;
  morphChildren(el, template.content);
}

function keyOf(node: Node): string | null {
  return node instanceof Element ? node.getAttribute("data-sf-key") : null;
}

function alike(a: Node, b: Node): boolean {
  if (a.nodeType !== b.nodeType) return false;
  if (a instanceof Element && b instanceof Element && a.tagName !== b.tagName) return false;
  return keyOf(a) === keyOf(b);
}

function morphChildren(from: Node, to: Node): void {
  const old = Array.from(from.childNodes);
  let i = 0;
  for (const next of Array.from(to.childNodes)) {
    const current = old[i];
    if (current && alike(current, next)) {
      morphNode(current, next);
      i += 1;
      continue;
    }
    const key = keyOf(next);
    const moved = key === null ? undefined : old.slice(i).find((candidate) => keyOf(candidate) === key);
    if (moved) {
      from.insertBefore(moved, current ?? null);
      old.splice(old.indexOf(moved), 1);
      old.splice(i, 0, moved);
      morphNode(moved, next);
    } else {
      const imported = document.importNode(next, true);
      from.insertBefore(imported, current ?? null);
      old.splice(i, 0, imported);
    }
    i += 1;
  }
  for (const stale of old.slice(i)) stale.remove();
}

function morphNode(current: Node, next: Node): void {
  if (current.nodeType === Node.TEXT_NODE || current.nodeType === Node.COMMENT_NODE) {
    if (current.nodeValue !== next.nodeValue) current.nodeValue = next.nodeValue;
    return;
  }
  if (!(current instanceof Element) || !(next instanceof Element)) return;
  morphAttributes(current, next);
  if (current.tagName === "SF-I") return;
  morphChildren(current, next);
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
