/** The document's locale as BCP 47, `fr-FR` for `fr_FR`; `en` before any document says. The server converts the same way. */
export declare function localeTag(): string;
export interface NumberOptions {
	minimumFractionDigits?: number;
	maximumFractionDigits?: number;
}
export type DateStyle = "short" | "medium" | "long" | "full";
export declare const intl: {
	/** `n` grouped for the locale, rounded half away from zero to at most three fraction digits unless `options` say otherwise, trailing zeros dropped past the minimum. */
	number(n: number | bigint, options?: NumberOptions): string;
	/** `n` as an amount of the ISO currency `code`, the code spelled out, the currency's own fraction digits. */
	currency(n: number | bigint, code: string): string;
	/** The calendar date of `when`, milliseconds since the epoch or an ISO 8601 string, in UTC at `style`, `medium` by default. */
	date(when: number | string, style?: DateStyle): string;
	/** The cardinal plural category of `n`: `zero`, `one`, `two`, `few`, `many` or `other`. */
	plural(n: number | bigint): string;
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
