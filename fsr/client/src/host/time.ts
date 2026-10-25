// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { time } from "../std.js";

/** `when` in UTC with `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced and every other character kept. Runs in a server render and in the browser. */
export const format: typeof time.format = time.format;

/** The instant `amount` units after `when`. Runs in a server render and in the browser. */
export const add: typeof time.add = time.add;

/** `later` minus `earlier` in units, fractional. Runs in a server render and in the browser. */
export const diff: typeof time.diff = time.diff;

/** `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`, as milliseconds since the epoch; a date alone or a bare time is UTC. `null` for anything else. Runs in a server render and in the browser. */
export const parse: typeof time.parse = time.parse;

/** The clock now, milliseconds since the epoch. Server only on a render path. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const now: typeof time.now = time.now;

/** Milliseconds since the time origin, as `performance.now()`; on the server, since the process started. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const monotonic: typeof time.monotonic = time.monotonic;
