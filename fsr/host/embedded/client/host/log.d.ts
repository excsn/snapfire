// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { log } from "../std.js";
/** `message` at trace level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. Runs in a server render and in the browser. */
export declare const trace: typeof log.trace;
/** `message` at debug level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. Runs in a server render and in the browser. */
export declare const debug: typeof log.debug;
/** `message` at info level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. Runs in a server render and in the browser. */
export declare const info: typeof log.info;
/** `message` at warn level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. Runs in a server render and in the browser. */
export declare const warn: typeof log.warn;
/** `message` at error level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. Runs in a server render and in the browser. */
export declare const error: typeof log.error;
/** `message` at fatal level with `fields` beside it: the server traces it at error under `fsr::app`, the browser writes it to the console. Runs in a server render and in the browser. */
export declare const fatal: typeof log.fatal;
