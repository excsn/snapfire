import { text } from "../std.js";
export type { FrontMatterOptions } from "../std.js";
/** Decomposed, marks dropped, lowercased, every run outside `a-z0-9` one hyphen and no hyphen at either end. Runs in a server render and in the browser. */
export declare const slug: typeof text.slug;
/** The first `max` characters and `ellipsis` when `s` is longer, else `s`. Characters are code points. Runs in a server render and in the browser. */
export declare const truncate: typeof text.truncate;
/** The YAML block between two delimiter lines at the top of `source` as `data`, the rest as `content`, read as gray-matter reads it. `delimiters` is one string for both lines or `[open, close]`, `---` by default. Terminal pages only. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export declare const frontMatter: typeof text.frontMatter;
/** `data` as YAML between the delimiter lines, then `content` ending in a line break, as gray-matter's `stringify` writes it; empty `data` writes no block. Terminal pages only. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export declare const withFrontMatter: typeof text.withFrontMatter;
