import { createContext, createElement, useContext, useState, type ComponentType, type ReactElement, type ReactNode } from "react";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import { MountTiming, Mounter, Patcher } from "./boot.js";

/** The `<sf-s>` a layout renders its child segment into, when `el` is a layout: the first one under it that is not inside a nested island and is not an island's own region. */
function slotOf(el: Element): Element | null {
  for (const slot of Array.from(el.querySelectorAll("sf-s:not([data-sf-island])"))) {
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

/** The island regions a root's markup holds, in document order, which is the order the root's `Island` elements render in; each takes the next. */
interface Regions {
  slots: Element[];
  next: number;
}

const RegionsContext = createContext<Regions | null>(null);

function regionsOf(el: Element): Regions {
  const slots = Array.from(el.querySelectorAll("sf-s[data-sf-island]")).filter((slot) => slot.parentElement?.closest("sf-i") === el);
  return { slots, next: 0 };
}

export interface IslandProps {
  /** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
  when?: MountTiming;
  children?: ReactNode;
}

/** Places its one child component as an island of its own: on the server the child renders inside an `<sf-s data-sf-island>` region as a nested island; in the browser this element adopts that region as it stands and never reconciles it, and the boot runtime mounts the child in its own root at the timing asked for. Lowered by the build, so the child is never rendered here. */
export function Island({ when }: IslandProps): ReactElement {
  const regions = useContext(RegionsContext);
  const [html] = useState(() => {
    if (!regions) return "";
    const slot = regions.slots[regions.next++];
    return slot?.innerHTML ?? "";
  });
  const props: { [key: string]: unknown } = { "data-sf-island": "", dangerouslySetInnerHTML: { __html: html }, suppressHydrationWarning: true };
  if (when) props["data-sf-when"] = when;
  return createElement("sf-s", props);
}

/** `component` as a component that places it as an island with `options.when` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`. */
export function island<P extends object>(component: ComponentType<P>, options: { when?: MountTiming } = {}): (props: P) => ReactElement {
  return function IslandOf(props: P): ReactElement {
    return createElement(Island, { when: options.when }, createElement(component as ComponentType<object>, props));
  };
}

function withRegions(el: Element, element: ReactElement): ReactElement {
  return createElement(RegionsContext.Provider, { value: regionsOf(el) }, element);
}

export const reactMounter: Mounter = (component, props, el, hydrate) => {
  const element = withRegions(el, createElement(component as never, props as never, childrenFor(el)));
  if (hydrate) {
    return hydrateRoot(el, element);
  }
  const root = createRoot(el);
  root.render(element);
  return root;
};

export const reactPatcher: Patcher = (handle, component, props, el) => {
  (handle as Root).render(withRegions(el, createElement(component as never, props as never, childrenFor(el))));
};
