/** The document's locale as BCP 47, `fr-FR` for `fr_FR`; `en` before any document says. The server converts the same way. */
export declare function localeTag(): string;
export interface NumberOptions {
	minimumFractionDigits?: number;
	maximumFractionDigits?: number;
	/** A locale written in the code, formatted the same everywhere; the document's otherwise. */
	locale?: string;
}
export interface CurrencyOptions extends NumberOptions {
	/** `code` writes the ISO code, the default; `symbol` and `narrowSymbol` the symbol CLDR gives the locale. */
	display?: "code" | "symbol" | "narrowSymbol";
}
export type DateStyle = "short" | "medium" | "long" | "full";
export type TextWidth = "long" | "short" | "narrow";
/** A date at a `style`, or at the parts given: `weekday`, `year`, `month` and `day` as `Intl.DateTimeFormat` takes them, a text month or weekday setting the width. A numeric month is not read. */
export interface DateOptions {
	style?: DateStyle;
	weekday?: TextWidth;
	year?: "numeric";
	month?: TextWidth;
	day?: "numeric";
	locale?: string;
}
/** How `intl.list` joins: `type` `conjunction` by default, `disjunction` or `unit`, at a `style` width, `long` by default. */
export interface ListOptions {
	type?: "conjunction" | "disjunction" | "unit";
	style?: TextWidth;
	locale?: string;
}
export declare const intl: {
	/** `n` grouped for the locale, rounded half away from zero to at most three fraction digits unless `options` say otherwise, trailing zeros dropped past the minimum. */
	number(n: number | bigint, options?: NumberOptions): string;
	/** `n` as an amount of the ISO currency `code`, the code spelled out unless `options.display` says the symbol, the currency's own fraction digits unless the options give them. */
	currency(n: number | bigint, code: string, options?: CurrencyOptions): string;
	/** The calendar date of `when`, milliseconds since the epoch or an ISO 8601 string, in UTC at `style`, `medium` by default, or at the parts the options name. */
	date(when: number | string, style?: DateStyle | DateOptions): string;
	/** The cardinal plural category of `n`: `zero`, `one`, `two`, `few`, `many` or `other`. */
	plural(n: number | bigint): string;
	/** `items` joined as the locale joins a list, `a, b and c` in British English. */
	list(items: string[], options?: ListOptions): string;
	/** -1, 0 or 1 as `a` sorts before, with or after `b` by the locale's collation, as `a.localeCompare(b, locale)` answers. */
	compare(a: string, b: string, options?: {
		locale?: string;
	}): number;
};
export declare const text: {
	/** Decomposed, marks dropped, lowercased, every run outside `a-z0-9` one hyphen and no hyphen at either end. */
	slug(s: string): string;
	/** The first `max` characters and `ellipsis` when `s` is longer, else `s`. Characters are code points. */
	truncate(s: string, max: number, ellipsis?: string): string;
};
export declare const time: {
	/** `when` in UTC with `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced and every other character kept. */
	format(when: number, pattern: string): string;
	/** The instant `amount` units after `when`. */
	add(when: number, amount: number, unit: string): number;
	/** `later` minus `earlier` in units, fractional. */
	diff(later: number, earlier: number, unit: string): number;
	/** `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`, as milliseconds since the epoch; a date alone or a bare time is UTC. `null` for anything else. */
	parse(s: string): number | null;
	/** The clock now, milliseconds since the epoch. Server only on a render path. */
	now(): number;
};
export declare const crypto: {
	/** SHA-256 of `s` as lowercase hex. */
	hash(s: string): string;
	/** Whether `hash` is the hash of `s`. */
	verify(s: string, hash: string): boolean;
	/** `bytes` random bytes as hex. Server only on a render path. */
	random(bytes: number): string;
	/** `n` random bytes as Node's `randomBytes` gives them; `toString("hex")` or `toString("base64")` reads them. Server only on a render path. */
	randomBytes(n: number): Uint8Array;
	/** A version 4 UUID, as Node's `randomUUID`. Server only on a render path. */
	uuid(): string;
	/** `createHash(algorithm).update(part)….digest(encoding)`: SHA-256 of the parts' UTF-8 bytes in order as `hex` or `base64`; `sha256` is the algorithm provided. */
	digest(algorithm: string, encoding: string, ...parts: string[]): string;
};
/** The message under `key` in the document's locale, `{name}` placeholders filled from `args`; with `args.count`, the `key.<plural category>` form, then `key.other`, then `key`. A key the catalog lacks answers as itself. Under `fsr test` the runner answers with the Rust half. */
export declare function t(key: string, args?: {
	[name: string]: unknown;
}): string;
export declare const id: {
	/** A fresh identifier: a UUID, version 7 on the server. Server only on a render path. */
	new(): string;
};
/** Declares the browser half of a native pair under `name`, `module.member`, whose Rust half the host registers under the same name. With `f`, the pair has `render` reach and `f` is what the browser runs; without, it has `body` reach and calling it in the browser throws. The build reads the declaration, so `name` must be a string literal. */
export declare function native<F extends (...args: never[]) => unknown>(name: string, f?: F): F;
