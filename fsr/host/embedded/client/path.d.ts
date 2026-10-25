import { path } from "./std.js";
/** The parts joined by `/` and normalized; `.` for none. Runs in a server render and in the browser. */
export declare const join: typeof path.join;
/** `.` and empty segments dropped, `..` resolved against what precedes it. Runs in a server render and in the browser. */
export declare const normalize: typeof path.normalize;
/** Everything before the last segment. Runs in a server render and in the browser. */
export declare const dirname: typeof path.dirname;
/** The last segment, without `ext` when it ends with it. Runs in a server render and in the browser. */
export declare const basename: typeof path.basename;
/** The last segment's extension from its last `.`, empty for none. Runs in a server render and in the browser. */
export declare const extname: typeof path.extname;
/** Whether `p` starts with `/`. Runs in a server render and in the browser. */
export declare const isAbsolute: typeof path.isAbsolute;
