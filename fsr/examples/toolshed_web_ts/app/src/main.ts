import htmx from "htmx.org";
import { boot, enableNavigation } from "@snapfire/fsr-client";
import { bindHtmx } from "@snapfire/fsr-client/htmx";
import { registerIslands } from "@generated/islands.js";

import "./elements/shed-tally.js";
import "./elements/loan-planner.js";

registerIslands();
boot();
enableNavigation();
bindHtmx(htmx);
