import { key } from "@snapfire/fsr-client/store";

/** The one count every probe reads and writes, so a write in any framework shows in all of them. */
export const probeCount = key<number>("probe/count");

/** Seeded by the streamed layout and again, differently, by the page that streams in under it. */
export const owner = key<string>("repro/owner");

/** Read beside `probeCount` by the held probes, so one hydration can carry a key the server held and one it did not. */
export const probeOther = key<number>("probe/other");
