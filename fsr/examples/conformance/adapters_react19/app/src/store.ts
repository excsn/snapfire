import { key } from "@snapfire/fsr-client/store";

/** The one count every probe reads and writes, so a write in any framework shows in all of them. */
export const probeCount = key<number>("probe/count");

/** Seeded by the streamed layout and again, differently, by the page that streams in under it. */
export const owner = key<string>("repro/owner");

/** Seeded by two streamed slots of the streamed layout, each with its own name, so which one settles it is a matter of position. */
export const slotOwner = key<string>("repro/slot");
