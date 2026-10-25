// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { t as translate } from "./std.js";

/** The message under `key` in the document's locale, `{name}` placeholders filled from `args`; with `args.count`, the `key.<plural category>` form, then `key.other`, then `key`. A key the catalog lacks answers as itself. Under `fsr test` the runner answers with the Rust half. Runs in a server render and in the browser. */
export const t: typeof translate = translate;
