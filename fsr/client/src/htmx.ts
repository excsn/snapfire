import { adopt } from "./store.js";
import { scan } from "./boot.js";
import { navigates } from "./navigator.js";

/** The one method this adapter calls on htmx: the library's own re-scan of a subtree. */
export interface HtmxProcessor {
  process(element: Element): void;
}

/**
 * Makes htmx and the client aware of each other's markup, both directions.
 * After htmx settles a swap the client reads any store seed the fragment
 * carried and mounts any island it placed; after the navigator applies a
 * payload htmx processes what it wrote, without which a form or an anchor
 * reached by a soft navigation is markup htmx never saw and the browser
 * follows it natively. The document is processed once as it binds; htmx
 * leaves an element it already processed alone.
 *
 * htmx 4 names the settle event `htmx:after:settle` and htmx 2
 * `htmx:afterSettle`; each fires only its own, so both are heard.
 *
 * While the navigator is enabled a step back or forward is its own: htmx 4
 * otherwise fetches the page again on `popstate` for an entry it stamped and
 * swaps the body under the navigator's patch, so the binding cancels
 * `htmx:before:history:restore`.
 *
 * Returns the function that takes the listeners off again.
 */
const SETTLED = ["htmx:after:settle", "htmx:afterSettle"];
const RESTORE = "htmx:before:history:restore";

export function bindHtmx(htmx: HtmxProcessor): () => void {
  const settled = () => {
    adopt();
    scan(document);
  };
  const rewire = () => htmx.process(document.body);
  const restore = (event: Event) => {
    if (navigates()) event.preventDefault();
  };
  for (const name of SETTLED) document.body.addEventListener(name, settled);
  document.addEventListener(RESTORE, restore);
  document.addEventListener("sf:navigate", rewire);
  document.addEventListener("sf:fill", rewire);
  rewire();
  return () => {
    for (const name of SETTLED) document.body.removeEventListener(name, settled);
    document.removeEventListener("sf:navigate", rewire);
    document.removeEventListener("sf:fill", rewire);
    document.removeEventListener(RESTORE, restore);
  };
}
