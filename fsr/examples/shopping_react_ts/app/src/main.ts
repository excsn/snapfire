import { boot, enableNavigation, registerIsland } from "@snapfire/fsr-client";
import { reactMounter } from "@snapfire/fsr-client/react";
import { registerIslands } from "../generated/islands.js";

registerIslands();

registerIsland("src/About.tsx#default", {
  loader: () => import("./About.js").then((m) => m.default),
  mount: reactMounter,
});

boot();
enableNavigation();
