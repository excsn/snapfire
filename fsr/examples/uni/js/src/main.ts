import htmx from "htmx.org";
import { boot, enableNavigation, registerIsland } from "@snapfire/fsr-client";
import { bindHtmx } from "@snapfire/fsr-client/htmx";
import { reactMounter, reactPatcher } from "@snapfire/fsr-client/react";
import { vueMounter, vuePatcher } from "@snapfire/fsr-client/vue";

registerIsland("js/src/ui/Watch.tsx#default", {
  loader: () => import("./ui/Watch.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
});

registerIsland("js/src/ui/Feed.tsx#default", {
  loader: () => import("./ui/Feed.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
});

registerIsland("js/src/ui/Holdings.vue#default", {
  loader: () => import("./ui/Holdings.js").then((m) => m.default),
  mount: vueMounter,
  patch: vuePatcher,
});

boot();
enableNavigation();
bindHtmx(htmx);
