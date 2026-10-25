// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { crypto } from "../std.js";

/** SHA-256 of `s` as lowercase hex. Runs in a server render and in the browser. */
export const hash: typeof crypto.hash = crypto.hash;

/** Whether `hash` is the hash of `s`. Runs in a server render and in the browser. */
export const verify: typeof crypto.verify = crypto.verify;

/** `bytes` random bytes as `hex` (the default) or `base64`. Server only on a render path. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const random: typeof crypto.random = crypto.random;

/** A version 4 UUID. Server only on a render path. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const uuid: typeof crypto.uuid = crypto.uuid;

/** The hash of the parts' UTF-8 bytes in order as `hex` or `base64`; `sha256` is the algorithm provided. Runs in a server render and in the browser. */
export const digest: typeof crypto.digest = crypto.digest;
