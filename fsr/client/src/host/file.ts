// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { file } from "../std.js";
export type { FileStat } from "../std.js";

/** The file as text, bytes that are not UTF-8 as U+FFFD. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const read: typeof file.read = file.read;

/** Writes `data` as the file's text, a value that is not text as JSON. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const write: typeof file.write = file.write;

/** Whether anything is at `path`. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const exists: typeof file.exists = file.exists;

/** The names of a directory's entries, sorted. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const list: typeof file.list = file.list;

/** Makes the directory, its parents too with `recursive`. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const makeDir: typeof file.makeDir = file.makeDir;

/** What is at `path`: whether it is a file or a directory, its size in bytes and when it last changed. Runs in every host body: loaders, actions, route handlers, middleware and terminal programs. */
export const stat: typeof file.stat = file.stat;
