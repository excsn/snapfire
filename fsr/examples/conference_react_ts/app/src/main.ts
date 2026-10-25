import htmx from "htmx.org";
import { boot, enableNavigation } from "@snapfire/fsr/client";
import { bindHtmx } from "@snapfire/fsr/client/htmx";
import { registerIslands } from "@generated/islands.js";

registerIslands();
boot();
enableNavigation({ transitions: true, activity: true });
bindHtmx(htmx);
