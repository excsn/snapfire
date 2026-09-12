import { key } from "@snapfire/fsr-client/store";

/** How many talks are on your schedule. Seeded by the root layout, written optimistically by the save button and read by the header. */
export const savedCount = key<number>("schedule/saved");
