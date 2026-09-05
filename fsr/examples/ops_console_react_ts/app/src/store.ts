import { key } from "@snapfire/fsr-client/store";

/** Open alerts, seeded by the root layout and acknowledged from the alerts slot. */
export const openAlerts = key<number>("alerts/open");

/** Agents the operator watches, seeded by the root layout and written optimistically. */
export const watching = key<number>("fleet/watching");

/** The region in view: the root layout seeds `all`, the agents layout wins it with the query. */
export const region = key<string>("fleet/region");

/** Nothing seeds this one: the agent list writes it and the header reads it. */
export const selected = key<string>("fleet/selected");

/** Computed from the two the server seeds, so the header line follows both. */
export const headline = key<string>("fleet/headline");

/** Row density, a setting held in the session, seeded by the root layout and read by the list. */
export const density = key<string>("ui/density");
