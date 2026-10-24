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
	/** The YAML block between two delimiter lines at the top of `source` as `data`, the rest as `content`, read as gray-matter reads it. `delimiters` is one string for both lines or `[open, close]`, `---` by default. Terminal pages only. */
	frontMatter<T = {
		[key: string]: unknown;
	}>(source: string, options?: FrontMatterOptions): {
		data: T;
		content: string;
	};
	/** `data` as YAML between the delimiter lines, then `content` ending in a line break, as gray-matter's `stringify` writes it; empty `data` writes no block. Terminal pages only. */
	withFrontMatter(content: string, data: {
		[key: string]: unknown;
	}, options?: FrontMatterOptions): string;
};
export interface FrontMatterOptions {
	delimiters?: string | [string, string];
}
/** What `file.stat` answers. */
export interface FileStat {
	isFile: boolean;
	isDirectory: boolean;
	size: number;
	mtimeMs: number;
}
/** The files of the machine a terminal page runs on. A failure throws an `Error` whose `code` is the POSIX name, `ENOENT` for a missing file. Terminal pages only. */
export declare const file: {
	/** The file as text, bytes that are not UTF-8 as U+FFFD. */
	read(path: string): string;
	/** Writes `data` as the file's text, a value that is not text as JSON. */
	write(path: string, data: unknown): void;
	/** Whether anything is at `path`. */
	exists(path: string): boolean;
	/** The names of a directory's entries, sorted. */
	list(path: string): string[];
	/** Makes the directory, its parents too with `recursive`. */
	makeDir(path: string, options?: {
		recursive?: boolean;
	}): void;
	/** What is at `path`: whether it is a file or a directory, its size in bytes and when it last changed. */
	stat(path: string): FileStat;
};
/** POSIX paths as text. Terminal pages only. */
export declare const path: {
	/** The parts joined by `/` and normalized; `.` for none. */
	join(...parts: string[]): string;
	/** The parts right to left until one is absolute, the working directory before them otherwise. */
	resolve(...parts: string[]): string;
	/** `.` and empty segments dropped, `..` resolved against what precedes it. */
	normalize(p: string): string;
	/** Everything before the last segment. */
	dirname(p: string): string;
	/** The last segment, without `ext` when it ends with it. */
	basename(p: string, ext?: string): string;
	/** The last segment's extension from its last `.`, empty for none. */
	extname(p: string): string;
	/** The path from `from` to `to`, both resolved. */
	relative(from: string, to: string): string;
	/** Whether `p` starts with `/`. */
	isAbsolute(p: string): boolean;
};
/** The system clipboard of the machine a terminal page runs on, through its own copy and paste commands. Terminal pages only. */
export declare const clipboard: {
	/** Puts `text` on the clipboard. */
	write(text: string): void;
	/** What the clipboard holds as text. */
	read(): string;
};
/** The machine a terminal page runs on. Terminal pages only. */
export declare const system: {
	/** The environment variable `name`, null when it is not set. */
	env(name: string): string | null;
	/** The home directory. */
	home(): string;
	/** The operating system: `macos`, `linux`, `windows` or another name Rust gives one. */
	platform(): string;
	/** The temporary directory, no trailing `/`. */
	tempDir(): string;
	/** The line ending: `"\r\n"` on Windows, `"\n"` elsewhere. */
	lineEnd(): string;
	/** The working directory. */
	cwd(): string;
	/** The resident set of the running page, in bytes. */
	memory(): number;
	/** Starts `command` with `args`, its output piped, as a process the page hears from: `stdout.on("data", f)` and `stderr.on("data", f)` hear its output, `on("exit", f)` and `on("close", f)` its code and signal and `on("error", f)` a failure to start. `options` are `cwd`, `env`, `shell` and `stdio: "inherit"` for a process that owns the terminal. Terminal pages only. */
	spawn(command: string, args?: string[], options?: SpawnOptions): Spawned;
};
export interface SpawnOptions {
	cwd?: string;
	env?: {
		[name: string]: string;
	};
	shell?: boolean | string;
	stdio?: "inherit" | "pipe";
}
/** What a process `system.spawn` started writes, handed over as text, so `setEncoding` changes nothing. */
export interface SpawnedOutput {
	on(event: "data", f: (chunk: string) => void): void;
	setEncoding(encoding: string): void;
}
/** A process's standard input, which the page writes text to and then closes. */
export interface SpawnedInput {
	write(text: string): void;
	end(): void;
}
/** A process `system.spawn` started. */
export interface Spawned {
	stdin: SpawnedInput;
	stdout: SpawnedOutput;
	stderr: SpawnedOutput;
	on(event: "exit" | "close", f: (code: number | null, signal: string | null) => void): void;
	on(event: "error", f: (error: Error) => void): void;
	off(event: "exit" | "close" | "error", f: (...args: never[]) => void): void;
}
/** A value scoped to a call: `run(value, fn)` holds `value` while `fn` runs, `get()` reads it, `undefined` outside any `run`, and `set(value)` holds it from then on. A terminal page keeps it across the awaits inside `fn`. */
export declare class Scope<T> {
	#private;
	run<R>(value: T, fn: (...args: never[]) => R, ...args: never[]): R;
	get(): T | undefined;
	set(value: T): void;
}
export declare const time: {
	/** Milliseconds since the time origin, as `performance.now()`; on the server, since the process started. */
	monotonic(): number;
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
/** A structured line at each level, `fields` whatever goes with the message; on the server it is a `tracing` event under `fsr::app`, here a console line. `fatal` is `error` in the console. */
export declare const log: {
	trace(message: string, fields?: unknown): null;
	debug(message: string, fields?: unknown): null;
	info(message: string, fields?: unknown): null;
	warn(message: string, fields?: unknown): null;
	error(message: string, fields?: unknown): null;
	fatal(message: string, fields?: unknown): null;
};
export declare const crypto: {
	/** SHA-256 of `s` as lowercase hex. */
	hash(s: string): string;
	/** Whether `hash` is the hash of `s`. */
	verify(s: string, hash: string): boolean;
	/** `bytes` random bytes as `hex`, the default, or `base64`. Server only on a render path. */
	random(bytes: number, encoding?: "hex" | "base64"): string;
	/** A version 4 UUID. Server only on a render path. */
	uuid(): string;
	/** The hash of the parts' UTF-8 bytes in order as `hex` or `base64`; `sha256` is the algorithm provided. */
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
