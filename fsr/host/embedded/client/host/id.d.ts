import { id } from "../std.js";
/** A fresh identifier: a UUID, version 7 on the server. Server only on a render path. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
declare const new_: typeof id.new;
export { new_ as new };
