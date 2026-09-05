import { boot, derive, enableNavigation, get } from "@snapfire/fsr-client";
import { registerIslands } from "@generated/islands.js";

import { headline, openAlerts, watching } from "./store.js";

registerIslands();

/** The same string the root layout's `store` seeds, so registering this costs no re-render at hydration and every later change to either key updates the header. */
derive(headline, [openAlerts, watching], (read) => {
  const open = read(openAlerts) ?? 0;
  const held = read(watching) ?? 0;
  return `${open === 0 ? "quiet" : `${open} to look at`}${held === 0 ? "" : `, watching ${held}`}`;
});

boot();
enableNavigation();

const g = globalThis as { __ops?: { headline: () => string | undefined } };
g.__ops = { headline: () => get(headline) };
