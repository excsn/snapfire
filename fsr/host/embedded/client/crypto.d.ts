// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { crypto } from "./std.js";
/** SHA-256 of `s` as lowercase hex. Runs in a server render and in the browser. */
export declare const hash: typeof crypto.hash;
/** Whether `hash` is the hash of `s`. Runs in a server render and in the browser. */
export declare const verify: typeof crypto.verify;
/** The hash of the parts' UTF-8 bytes in order as `hex` or `base64`; `sha256` is the algorithm provided. Runs in a server render and in the browser. */
export declare const digest: typeof crypto.digest;
