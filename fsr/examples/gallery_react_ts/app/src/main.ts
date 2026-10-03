import { boot, enableNavigation } from "@snapfire/fsr-client";
import { registerIslands } from "@generated/islands.js";

import { installZoom } from "./ui/zoom";

registerIslands();
boot();
enableNavigation();
installZoom();
