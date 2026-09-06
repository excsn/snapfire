import { intl } from "@snapfire/fsr-client/std";

/** `n` grouped for the locale with `noun` in the number the locale's plural rules give it: `1 item`, `1,234 items`. */
export function count(n: number, noun: string): string {
  return `${intl.number(n)} ${noun}${intl.plural(n) === "one" ? "" : "s"}`;
}
