import { key } from "@snapfire/fsr-client/store";

/** Who is signed in, seeded by the root layout for every document, the mounted sites' included. */
export const who = key<string>("portal/who");

/** How many teams the directory lists, seeded by the root layout. */
export const teams = key<number>("portal/teams");
