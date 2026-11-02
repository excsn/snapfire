import htmx from "htmx.org";
import { boot, enableNavigation, registerIsland } from "@snapfire/fsr/client";
import { reactMounter } from "@snapfire/fsr/client/react";
import { bindHtmx } from "@snapfire/fsr/client/htmx";
import { registerIslands } from "@generated/islands.js";

registerIslands();
// With no `patch` these cannot take new props, so a navigation replaces them. An application may register a mounter this way.
registerIsland("src/ui/TrackNote.tsx#default", { loader: () => import("./ui/TrackNote.js").then((m) => m.default), mount: reactMounter });
registerIsland("routes/desk/layout.tsx#default", { loader: () => import("../routes/desk/layout.js").then((m) => m.default), mount: reactMounter });
boot();
enableNavigation({ transitions: true, activity: true });
bindHtmx(htmx);
