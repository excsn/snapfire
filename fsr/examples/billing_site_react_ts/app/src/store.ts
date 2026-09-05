import { key } from "@snapfire/fsr-client/store";
import type { ShellStore } from "@generated/shell";

/** Seeded by the portal's root layout on every document; typed by the shell contract the site was built against. */
export const who = key<ShellStore["portal/who"]>("portal/who");
