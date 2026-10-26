import { key } from "@snapfire/fsr/client/store";

/** How many talks are on your schedule. Seeded by the root layout, written optimistically by the save button and read by the header. */
export const savedCount = key<number>("schedule/saved");

/** What the visitor has packed for the day. Only the browser holds it: the venue's checklist writes it and the help desk reads it. */
export const packed = key<string[]>("venue/packed");

/** How many notices are posted. Seeded by the notices page htmx fetches into the venue and read by the help desk. */
export const noticeCount = key<number>("venue/notices");
