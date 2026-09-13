import htmx from "htmx.org";
import { boot, enableNavigation } from "@snapfire/fsr-client";
import { bindHtmx } from "@snapfire/fsr-client/htmx";

import "./elements/shed-tally.js";
import "./elements/loan-planner.js";

boot();
enableNavigation();
bindHtmx(htmx);
