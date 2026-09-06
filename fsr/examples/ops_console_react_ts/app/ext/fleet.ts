import { native } from "@snapfire/fsr-client/std";

/** An agent's queue as a phrase: `idle`, `1 queued`, `12 queued`. The Rust half is `ops_console_react_ts::ext::queue_label`, registered in `main.rs`. */
export const queueLabel = native("fleet.queueLabel", (depth: number): string => (depth === 0 ? "idle" : depth === 1 ? "1 queued" : `${depth} queued`));
