import { boot, enableNavigation, registerIsland } from "@snapfire/fsr-client";
import { reactMounter } from "@snapfire/fsr-client/react";

registerIsland("components/ServerChart.tsx#default", {
  loader: () => import("./ServerChart.js").then((m) => m.default),
  mount: reactMounter,
});

registerIsland("components/LatencyChart.tsx#default", {
  loader: () => import("./LatencyChart.js").then((m) => m.default),
  mount: reactMounter,
  when: "visible",
});

boot();
enableNavigation();
