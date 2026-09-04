import { createElement, type ReactElement } from "react";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import { Mounter, Patcher } from "./boot.js";

/** The `<sf-s>` a layout renders its child segment into, when `el` is a layout: the first one under it that is not inside a nested island. */
function slotOf(el: Element): Element | null {
  for (const slot of Array.from(el.querySelectorAll("sf-s"))) {
    if (slot.parentElement?.closest("sf-i") === el) return slot;
  }
  return null;
}

/** The child element a layout receives: `<sf-s>` with the markup it already holds, created once and passed unchanged on every render so React adopts the region at hydration and never reconciles it. */
const children = new WeakMap<Element, ReactElement>();

function childrenFor(el: Element): ReactElement | undefined {
  const held = children.get(el);
  if (held) return held;
  const slot = slotOf(el);
  if (!slot) return undefined;
  const element = createElement("sf-s", { dangerouslySetInnerHTML: { __html: slot.innerHTML }, suppressHydrationWarning: true });
  children.set(el, element);
  return element;
}

export const reactMounter: Mounter = (component, props, el, hydrate) => {
  const element = createElement(component as never, props as never, childrenFor(el));
  if (hydrate) {
    return hydrateRoot(el, element);
  }
  const root = createRoot(el);
  root.render(element);
  return root;
};

export const reactPatcher: Patcher = (handle, component, props, el) => {
  (handle as Root).render(createElement(component as never, props as never, childrenFor(el)));
};
