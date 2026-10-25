import { text } from "./std.js";
export type { FrontMatterOptions } from "./std.js";
/** Decomposed, marks dropped, lowercased, every run outside `a-z0-9` one hyphen and no hyphen at either end. Runs in a server render and in the browser. */
export declare const slug: typeof text.slug;
/** The first `max` characters and `ellipsis` when `s` is longer, else `s`. Characters are code points. Runs in a server render and in the browser. */
export declare const truncate: typeof text.truncate;
