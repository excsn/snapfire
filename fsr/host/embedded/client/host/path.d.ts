import { path } from "../std.js";
/** The parts joined by `/` and normalized; `.` for none. Runs in a server render and in the browser. */
export declare const join: typeof path.join;
/** The parts right to left until one is absolute, the working directory before them otherwise. Host only. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export declare const resolve: typeof path.resolve;
/** `.` and empty segments dropped, `..` resolved against what precedes it. Runs in a server render and in the browser. */
export declare const normalize: typeof path.normalize;
/** Everything before the last segment. Runs in a server render and in the browser. */
export declare const dirname: typeof path.dirname;
/** The last segment, without `ext` when it ends with it. Runs in a server render and in the browser. */
export declare const basename: typeof path.basename;
/** The last segment's extension from its last `.`, empty for none. Runs in a server render and in the browser. */
export declare const extname: typeof path.extname;
/** The path from `from` to `to`, both resolved. Host only. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export declare const relative: typeof path.relative;
/** Whether `p` starts with `/`. Runs in a server render and in the browser. */
export declare const isAbsolute: typeof path.isAbsolute;
