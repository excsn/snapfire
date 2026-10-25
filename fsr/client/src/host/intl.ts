// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.
import { intl } from "../std.js";
export type { NumberOptions, CurrencyOptions, DateOptions, ListOptions, DateStyle, TextWidth } from "../std.js";

/** `n` grouped for the locale, rounded half away from zero to at most three fraction digits unless `options` say otherwise, trailing zeros dropped past the minimum. Runs in a server render and in the browser. */
export const number: typeof intl.number = intl.number;

/** `n` as an amount of the ISO currency `code`, the code spelled out unless `options.display` says the symbol, the currency's own fraction digits unless the options give them. Runs in a server render and in the browser. */
export const currency: typeof intl.currency = intl.currency;

/** The calendar date of `when`, milliseconds since the epoch or an ISO 8601 string, in UTC at `style` (`medium` by default) or at the parts the options name. Runs in a server render and in the browser. */
export const date: typeof intl.date = intl.date;

/** The cardinal plural category of `n`: `zero`, `one`, `two`, `few`, `many` or `other`. Runs in a server render and in the browser. */
export const plural: typeof intl.plural = intl.plural;

/** `items` joined as the locale joins a list, `a, b and c` in British English. Runs in a server render and in the browser. */
export const list: typeof intl.list = intl.list;

/** -1, 0 or 1 as `a` sorts before, with or after `b` by the locale's collation, as `a.localeCompare(b, locale)` answers. Runs in a server render and in the browser. */
export const compare: typeof intl.compare = intl.compare;
