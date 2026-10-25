import { time } from "../std.js";
/** `when` in UTC with `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced and every other character kept. Runs in a server render and in the browser. */
export declare const format: typeof time.format;
/** The instant `amount` units after `when`. Runs in a server render and in the browser. */
export declare const add: typeof time.add;
/** `later` minus `earlier` in units, fractional. Runs in a server render and in the browser. */
export declare const diff: typeof time.diff;
/** `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`, as milliseconds since the epoch; a date alone or a bare time is UTC. `null` for anything else. Runs in a server render and in the browser. */
export declare const parse: typeof time.parse;
/** The clock now, milliseconds since the epoch. Server only on a render path. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export declare const now: typeof time.now;
/** Milliseconds since the time origin, as `performance.now()`; on the server, since the process started. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export declare const monotonic: typeof time.monotonic;
