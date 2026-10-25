// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { text } from "./std.js";
export type { FrontMatterOptions } from "./std.js";

/** Decomposed, marks dropped, lowercased, every run outside `a-z0-9` one hyphen and no hyphen at either end. Runs in a server render and in the browser. */
export const slug: typeof text.slug = text.slug;

/** The first `max` characters and `ellipsis` when `s` is longer, else `s`. Characters are code points. Runs in a server render and in the browser. */
export const truncate: typeof text.truncate = text.truncate;
