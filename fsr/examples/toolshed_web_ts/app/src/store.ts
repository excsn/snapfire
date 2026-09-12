import { key } from "@snapfire/fsr-client/store";

/** How many tools the visitor has reserved. Seeded by the root layout and by every fragment, read by the masthead tally. */
export const reservedCount = key<number>("shed/reserved");
