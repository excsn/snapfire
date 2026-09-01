import { boot, enableNavigation, registerIsland } from "@snapfire/fsr-client";
import { reactMounter } from "@snapfire/fsr-client/react";

registerIsland("app/main.tsx#Catalog", {
  loader: () => import("./Catalog.js").then((m) => m.default),
  mount: reactMounter,
});

registerIsland("app/main.tsx#Product", {
  loader: () => import("./Product.js").then((m) => m.default),
  mount: reactMounter,
});

registerIsland("app/main.tsx#Failed", {
  loader: () => import("./Failed.js").then((m) => m.default),
  mount: reactMounter,
});

boot();
enableNavigation();
