/**
 * The shadow root of a custom element whose template the server wrote. The
 * HTML parser attaches a declarative shadow root itself. Markup that arrived
 * through `innerHTML`, an htmx swap among it, keeps the
 * `<template shadowrootmode>` as a child instead, so the root is attached
 * with the mode and options the template carries and the template taken out.
 * A closed root the parser attached is
 * reached through `internals`. `null` when the element has neither.
 */
export function shadowOf(element: HTMLElement, internals?: ElementInternals): ShadowRoot | null {
  const attached = element.shadowRoot ?? internals?.shadowRoot ?? null;
  if (attached) return attached;
  let template: HTMLTemplateElement | null = null;
  for (const child of Array.from(element.children)) {
    if (child instanceof HTMLTemplateElement && child.hasAttribute("shadowrootmode")) {
      template = child;
      break;
    }
  }
  if (!template) return null;
  const root = element.attachShadow({
    mode: template.getAttribute("shadowrootmode") === "closed" ? "closed" : "open",
    delegatesFocus: template.hasAttribute("shadowrootdelegatesfocus"),
    clonable: template.hasAttribute("shadowrootclonable"),
    serializable: template.hasAttribute("shadowrootserializable"),
  });
  root.append(template.content.cloneNode(true));
  template.remove();
  return root;
}
