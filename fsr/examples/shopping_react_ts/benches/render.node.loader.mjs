// Resolves the bare specifiers the prepared bench modules import, from the map
// `cargo bench --bench render` wrote, so Node loads exactly what QuickJS did.
import { pathToFileURL } from "node:url";

let map = {};

export function initialize(data) {
  map = data ?? {};
}

export function resolve(specifier, context, next) {
  const hit = map[specifier];
  if (hit) {
    return { url: pathToFileURL(hit).href, shortCircuit: true };
  }
  return next(specifier, context);
}
