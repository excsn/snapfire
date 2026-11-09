// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { clipboard } from "../std.js";
/** Puts `text` on the clipboard. Runs in a terminal program only. */
export declare const write: typeof clipboard.write;
/** What the clipboard holds as text. Runs in a terminal program only. */
export declare const read: typeof clipboard.read;
