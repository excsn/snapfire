import { clipboard } from "../std.js";
/** Puts `text` on the clipboard. Runs in a terminal program only. */
export declare const write: typeof clipboard.write;
/** What the clipboard holds as text. Runs in a terminal program only. */
export declare const read: typeof clipboard.read;
