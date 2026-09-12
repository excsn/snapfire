import { key } from "@snapfire/fsr-client/store";

/** How many recipes are planned for tonight. Seeded by the root layout, written optimistically by the plan button and read by the masthead. */
export const plannedCount = key<number>("tonight/planned");
