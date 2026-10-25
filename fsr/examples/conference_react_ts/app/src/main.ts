import { boot, enableNavigation } from "@snapfire/fsr/client";
import { registerIslands } from "@generated/islands.js";

registerIslands();
boot();
enableNavigation({ transitions: true, activity: true });
