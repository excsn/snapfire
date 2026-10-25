import { localeTag } from "@snapfire/fsr/client/intl";

/** `n` grouped for the locale with `noun` in the number the locale's plural rules give it: `1 item`, `1,234 items`. */
export function count(n: number, noun: string): string {
  return `${new Intl.NumberFormat(localeTag()).format(n)} ${noun}${new Intl.PluralRules(localeTag()).select(n) === "one" ? "" : "s"}`;
}
