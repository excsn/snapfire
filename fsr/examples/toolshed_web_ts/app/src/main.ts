import htmx from "htmx.org";
import { adopt, boot, enableNavigation, scan } from "@snapfire/fsr-client";

import "./elements/shed-tally.js";
import "./elements/loan-planner.js";
import "./elements/time-ago.js";

boot();
enableNavigation();

document.body.addEventListener("htmx:afterSettle", () => {
  adopt();
  scan(document);
});

const rewire = () => htmx.process(document.body);
document.addEventListener("sf:navigate", rewire);
document.addEventListener("sf:fill", rewire);
