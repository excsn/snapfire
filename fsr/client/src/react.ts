import { createElement } from "react";
import { createRoot, hydrateRoot } from "react-dom/client";

import { Mounter } from "./boot.js";

export const reactMounter: Mounter = (component, props, el, hydrate) => {
  const element = createElement(component as never, props as never);
  if (hydrate) {
    return hydrateRoot(el, element);
  }
  const root = createRoot(el);
  root.render(element);
  return root;
};
