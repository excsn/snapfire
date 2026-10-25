// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { path } from "./std.js";

/** The parts joined by `/` and normalized; `.` for none. Runs in a server render and in the browser. */
export const join: typeof path.join = path.join;

/** `.` and empty segments dropped, `..` resolved against what precedes it. Runs in a server render and in the browser. */
export const normalize: typeof path.normalize = path.normalize;

/** Everything before the last segment. Runs in a server render and in the browser. */
export const dirname: typeof path.dirname = path.dirname;

/** The last segment, without `ext` when it ends with it. Runs in a server render and in the browser. */
export const basename: typeof path.basename = path.basename;

/** The last segment's extension from its last `.`, empty for none. Runs in a server render and in the browser. */
export const extname: typeof path.extname = path.extname;

/** Whether `p` starts with `/`. Runs in a server render and in the browser. */
export const isAbsolute: typeof path.isAbsolute = path.isAbsolute;
