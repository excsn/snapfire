import htmx from "htmx.org";
import { boot, enableNavigation, live, registerIsland } from "@snapfire/fsr-client";
import { bindHtmx } from "@snapfire/fsr-client/htmx";
import { reactMounter, reactPatcher, reactUnmounter } from "@snapfire/fsr-client/react";
import { vueMounter, vuePatcher, vueUnmounter } from "@snapfire/fsr-client/vue";

registerIsland("js/src/ui/Watch.tsx#default", {
  loader: () => import("./ui/Watch.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
  unmount: reactUnmounter,
});

registerIsland("js/src/ui/Feed.tsx#default", {
  loader: () => import("./ui/Feed.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
  unmount: reactUnmounter,
});

registerIsland("js/src/ui/Chip.tsx#default", {
  loader: () => import("./ui/Chip.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
  unmount: reactUnmounter,
});

registerIsland("js/src/ui/Sparkline.vue#default", {
  loader: () => import("./ui/Sparkline.vue").then((m) => m.default),
  mount: vueMounter,
  patch: vuePatcher,
  unmount: vueUnmounter,
});

registerIsland("js/src/ui/Holdings.vue#default", {
  loader: () => import("./ui/Holdings.vue").then((m) => m.default),
  mount: vueMounter,
  patch: vuePatcher,
  unmount: vueUnmounter,
});

boot();
enableNavigation();
bindHtmx(htmx);

// The desk publishes `prices` when its clock moves; every island on the page
// is rendered from that clock, so one push patches both frameworks.
live(["prices"]);
