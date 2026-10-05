import { adopt } from "./store.js";
import { scan } from "./boot.js";

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
 * Returns the function that takes the listeners off again.
 */
const SETTLED = ["htmx:after:settle", "htmx:afterSettle"];

export function bindHtmx(htmx: HtmxProcessor): () => void {
  const settled = () => {
    adopt();
    scan(document);
  };
  const rewire = () => htmx.process(document.body);
  for (const name of SETTLED) document.body.addEventListener(name, settled);
  document.addEventListener("sf:navigate", rewire);
  document.addEventListener("sf:fill", rewire);
  rewire();
  return () => {
    for (const name of SETTLED) document.body.removeEventListener(name, settled);
    document.removeEventListener("sf:navigate", rewire);
    document.removeEventListener("sf:fill", rewire);
  };
}
