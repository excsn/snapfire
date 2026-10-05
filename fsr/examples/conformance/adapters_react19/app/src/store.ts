import { key } from "@snapfire/fsr-client/store";

/** The one count every probe reads and writes, so a write in any framework shows in all of them. */
export const probeCount = key<number>("probe/count");
