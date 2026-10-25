// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { id } from "../std.js";

/** A fresh identifier: a UUID, version 7 on the server. Server only on a render path. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
const new_: typeof id.new = id.new;
export { new_ as new };
