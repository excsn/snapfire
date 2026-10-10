import { boot, derive, enableNavigation, get } from "@snapfire/fsr-client";
import { registerIslands } from "@generated/islands.js";

import { t } from "@snapfire/fsr-client/std";

import { headline, openAlerts, watching } from "./store.js";

registerIslands();

/** The server computes `fleet/headline` from this derive too, so registering it costs no re-render at hydration and every later change to either key updates the header. */
derive(headline, [openAlerts, watching], (read) => {
  const open = read(openAlerts) ?? 0;
  const held = read(watching) ?? 0;
  return `${open === 0 ? t("headline.quiet") : t("headline.open", { count: open })}${held === 0 ? "" : t("headline.watching", { count: held })}`;
});

boot();
enableNavigation();

const g = globalThis as { __ops?: { headline: () => string | undefined } };
g.__ops = { headline: () => get(headline) };
