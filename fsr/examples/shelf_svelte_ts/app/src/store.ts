import { key } from "@snapfire/fsr-client/store";

/** How many books are finished. Seeded by the root layout, written optimistically by the progress island and read by the masthead. */
export const doneCount = key<number>("shelf/done");
