/**
 * What `<Link>` writes, apart from any framework: the anchor's attributes,
 * with the marks the navigator reads and `aria-current` for the page showing.
 */
import { currentAddressPath, currentDocumentPath, type PrefetchTiming } from "./navigator.js";

/** What a `<Link>` takes in every adapter; anything else goes on the `<a>`. */
export interface LinkOptions {
  href?: string;
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
  /** Which path the mark is judged against: `"url"`, the default, the address bar; `"document"`, the page beneath an open intercept. The two differ only while an intercept is open. */
  current?: "url" | "document";
  [attribute: string]: unknown;
}

export function linkAttributes({ full, into, prefetch, native, keep, match, current, ...rest }: LinkOptions): Record<string, unknown> {
  const attrs: Record<string, unknown> = { ...rest };
  if (full) attrs["data-sf-full"] = "true";
  if (into) attrs["data-sf-into"] = into;
  if (prefetch) attrs["data-sf-prefetch"] = prefetch;
  if (native) attrs["data-sf-native"] = "true";
  if (keep !== undefined) attrs["data-sf-keep"] = keep ? "true" : "false";
  const rule = match ?? "exact";
  if (rule !== "none" && typeof rest.href === "string" && rest["aria-current"] === undefined) {
    attrs["data-sf-link"] = rule;
    if (current === "document") attrs["data-sf-current"] = "document";
    const at = current === "document" ? currentDocumentPath() : currentAddressPath();
    const cut = at.indexOf("?");
    const path = cut === -1 ? at : at.slice(0, cut);
    if (rest.href === path) attrs["aria-current"] = rule === "prefix" ? "true" : "page";
    else if (rule === "prefix" && path.startsWith(`${rest.href}/`)) attrs["aria-current"] = "true";
  }
  return attrs;
}
