/** The standard library's browser half: `intl`, `text`, `time`, `crypto` and `id`, the same names the server answers in Rust. A `render` member agrees with the server byte for byte under the same locale, which is the document's, read from `currentLocale()`; a `body` member runs where it is called and has no such promise. `native` declares an application's own pair. */

import { catalog, currentLocale } from "./locale.js";

/** The document's locale as BCP 47, `fr-FR` for `fr_FR`; `en` before any document says. The server converts the same way. */
export function localeTag(): string {
  const tag = currentLocale();
  return tag ? tag.replace(/_/g, "-") : "en";
}

interface Bridge {
  ext?: (name: string, args: string, locale: string) => string;
}

/** Under `fsr test` the engine has no `Intl`, so a member that needs it asks the runner, which answers with the Rust half; anywhere else this returns `undefined` and the member computes. */
function bridged<T>(name: string, args: unknown[]): T | undefined {
  if (typeof Intl !== "undefined") return undefined;
  const sf = (globalThis as { __sf?: Bridge }).__sf;
  if (!sf?.ext) throw new Error(`${name} needs Intl, which this runtime does not have`);
  const json = JSON.stringify(args, (_, v: unknown) => (typeof v === "bigint" ? Number(v) : v === undefined ? null : v));
  return JSON.parse(sf.ext(name, json, currentLocale())) as T;
}

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

/** A date at a `style` or at the parts given: `weekday`, `year`, `month` and `day` as `Intl.DateTimeFormat` takes them, a text month or weekday setting the width. A numeric month is not read. */
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

export const intl = {
  /** `n` grouped for the locale, rounded half away from zero to at most three fraction digits unless `options` say otherwise, trailing zeros dropped past the minimum. */
  number(n: number | bigint, options?: NumberOptions): string {
    const { locale, ...rest } = options ?? {};
    return bridged<string>("intl.number", [n, options ?? null]) ?? new Intl.NumberFormat(locale ?? localeTag(), rest).format(n);
  },
  /** `n` as an amount of the ISO currency `code`, the code spelled out unless `options.display` says the symbol, the currency's own fraction digits unless the options give them. */
  currency(n: number | bigint, code: string, options?: CurrencyOptions): string {
    const { locale, display, ...rest } = options ?? {};
    return bridged<string>("intl.currency", [n, code, options ?? null]) ?? new Intl.NumberFormat(locale ?? localeTag(), { style: "currency", currency: code, currencyDisplay: display ?? "code", ...rest }).format(n);
  },
  /** The calendar date of `when`, milliseconds since the epoch or an ISO 8601 string, in UTC at `style` (`medium` by default) or at the parts the options name. */
  date(when: number | string, style: DateStyle | DateOptions = "medium"): string {
    const ms = typeof when === "string" ? time.parse(when) : when;
    if (ms === null) throw new Error(`intl.date: \`${when}\` is not an ISO 8601 date`);
    const served = bridged<string>("intl.date", [ms, style]);
    if (served !== undefined) return served;
    if (typeof style === "string") return new Intl.DateTimeFormat(localeTag(), { dateStyle: style, timeZone: "UTC" }).format(new Date(ms));
    const { locale, style: dateStyle, ...parts } = style;
    const options: Intl.DateTimeFormatOptions = { timeZone: "UTC", ...parts };
    if (dateStyle !== undefined && parts.weekday === undefined && parts.year === undefined && parts.month === undefined && parts.day === undefined) options.dateStyle = dateStyle;
    return new Intl.DateTimeFormat(locale ?? localeTag(), options).format(new Date(ms));
  },
  /** The cardinal plural category of `n`: `zero`, `one`, `two`, `few`, `many` or `other`. */
  plural(n: number | bigint): string {
    return bridged<string>("intl.plural", [n]) ?? new Intl.PluralRules(localeTag()).select(Number(n));
  },
  /** `items` joined as the locale joins a list, `a, b and c` in British English. */
  list(items: string[], options?: ListOptions): string {
    const { locale, ...rest } = options ?? {};
    return bridged<string>("intl.list", [items, options ?? null]) ?? new Intl.ListFormat(locale ?? localeTag(), rest).format(items);
  },
  /** -1, 0 or 1 as `a` sorts before, with or after `b` by the locale's collation, as `a.localeCompare(b, locale)` answers. */
  compare(a: string, b: string, options?: { locale?: string }): number {
    return bridged<number>("intl.compare", [a, b, options ?? null]) ?? Math.sign(a.localeCompare(b, options?.locale ?? localeTag()));
  },
};

export const text = {
  /** Decomposed, marks dropped, lowercased, every run outside `a-z0-9` one hyphen and no hyphen at either end. */
  slug(s: string): string {
    return s
      .normalize("NFD")
      .replace(/\p{M}+/gu, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
  },
  /** The first `max` characters and `ellipsis` when `s` is longer, else `s`. Characters are code points. */
  truncate(s: string, max: number, ellipsis = "…"): string {
    const chars = Array.from(s);
    if (chars.length <= Math.max(0, max)) return s;
    return chars.slice(0, Math.max(0, max)).join("") + ellipsis;
  },
  /** The YAML block between two delimiter lines at the top of `source` as `data`, the rest as `content`, read as gray-matter reads it. `delimiters` is one string for both lines or `[open, close]`, `---` by default. Terminal pages only. */
  frontMatter<T = { [key: string]: unknown }>(source: string, options?: FrontMatterOptions): { data: T; content: string } {
    return terminalOnly("text.frontMatter", [source, options ?? null]);
  },
  /** `data` as YAML between the delimiter lines, then `content` ending in a line break, as gray-matter's `stringify` writes it; empty `data` writes no block. Terminal pages only. */
  withFrontMatter(content: string, data: { [key: string]: unknown }, options?: FrontMatterOptions): string {
    return terminalOnly("text.withFrontMatter", [content, data, options ?? null]);
  },
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
export const file = {
  /** The file as text, bytes that are not UTF-8 as U+FFFD. */
  read(path: string): string {
    return terminalOnly("file.read", [path]);
  },
  /** Writes `data` as the file's text, a value that is not text as JSON. */
  write(path: string, data: unknown): void {
    terminalOnly("file.write", [path, data]);
  },
  /** Whether anything is at `path`. */
  exists(path: string): boolean {
    return terminalOnly("file.exists", [path]);
  },
  /** The names of a directory's entries, sorted. */
  list(path: string): string[] {
    return terminalOnly("file.list", [path]);
  },
  /** Makes the directory, its parents too with `recursive`. */
  makeDir(path: string, options?: { recursive?: boolean }): void {
    terminalOnly("file.makeDir", [path, options ?? null]);
  },
  /** What is at `path`: whether it is a file or a directory, its size in bytes and when it last changed. */
  stat(path: string): FileStat {
    return terminalOnly("file.stat", [path]);
  },
};

/** POSIX paths as text. `resolve` and `relative` read the working directory and run in a host only; the rest give the same answer on the server and in the browser. */
export const path = {
  /** The parts joined by `/` and normalized; `.` for none. */
  join(...parts: string[]): string {
    const joined = parts.filter((p) => p !== "");
    return joined.length === 0 ? "." : normalizePath(joined.join("/"));
  },
  /** The parts right to left until one is absolute, the working directory before them otherwise. Host only. */
  resolve(...parts: string[]): string {
    return terminalOnly("path.resolve", parts);
  },
  /** `.` and empty segments dropped, `..` resolved against what precedes it. */
  normalize(p: string): string {
    return normalizePath(p);
  },
  /** Everything before the last segment. */
  dirname(p: string): string {
    const trimmed = p.replace(/\/+$/, "");
    if (trimmed === "") return p.startsWith("/") ? "/" : ".";
    const at = trimmed.lastIndexOf("/");
    if (at === -1) return ".";
    if (at === 0) return "/";
    return trimmed.slice(0, at).replace(/\/+$/, "");
  },
  /** The last segment, without `ext` when it ends with it. */
  basename(p: string, ext?: string): string {
    const trimmed = p.replace(/\/+$/, "");
    const base = trimmed.slice(trimmed.lastIndexOf("/") + 1);
    return ext !== undefined && base !== ext && base.endsWith(ext) ? base.slice(0, base.length - ext.length) : base;
  },
  /** The last segment's extension from its last `.`, empty for none. */
  extname(p: string): string {
    const base = path.basename(p);
    const at = base.lastIndexOf(".");
    return at > 0 ? base.slice(at) : "";
  },
  /** The path from `from` to `to`, both resolved. Host only. */
  relative(from: string, to: string): string {
    return terminalOnly("path.relative", [from, to]);
  },
  /** Whether `p` starts with `/`. */
  isAbsolute(p: string): boolean {
    return p.startsWith("/");
  },
};

/** `.` and empty segments dropped, `..` resolved against what precedes it, a leading and a trailing `/` kept; `.` for nothing. */
function normalizePath(p: string): string {
  const absolute = p.startsWith("/");
  const trailing = p.length > 1 && p.endsWith("/");
  const out: string[] = [];
  for (const part of p.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      const last = out[out.length - 1];
      if (!absolute && (last === undefined || last === "..")) out.push("..");
      else if (last !== undefined) out.pop();
      continue;
    }
    out.push(part);
  }
  let joined = out.join("/");
  if (absolute) joined = "/" + joined;
  if (joined === "") return ".";
  if (trailing && joined !== "/") joined += "/";
  return joined;
}

/** The system clipboard of the machine a terminal page runs on, through its own copy and paste commands. Terminal pages only. */
export const clipboard = {
  /** Puts `text` on the clipboard. */
  write(text: string): void {
    return terminalOnly("clipboard.write", [text]);
  },
  /** What the clipboard holds as text. */
  read(): string {
    return terminalOnly("clipboard.read", []);
  },
};

/** The machine a terminal page runs on. Terminal pages only. */
export const system = {
  /** The environment variable `name`, null when it is not set. */
  env(name: string): string | null {
    return terminalOnly("system.env", [name]);
  },
  /** The home directory. */
  home(): string {
    return terminalOnly("system.home", []);
  },
  /** The operating system: `macos`, `linux`, `windows` or another name Rust gives one. */
  platform(): string {
    return terminalOnly("system.platform", []);
  },
  /** The temporary directory, no trailing `/`. */
  tempDir(): string {
    return terminalOnly("system.tempDir", []);
  },
  /** The line ending: `"\r\n"` on Windows, `"\n"` elsewhere. */
  lineEnd(): string {
    return terminalOnly("system.lineEnd", []);
  },
  /** The working directory. */
  cwd(): string {
    return terminalOnly("system.cwd", []);
  },
  /** The resident set of the running page, in bytes. */
  memory(): number {
    return terminalOnly("system.memory", []);
  },
  /** Starts `command` with `args`, its output piped, as a process the page hears from: `stdout.on("data", f)` and `stderr.on("data", f)` hear its output, `on("exit", f)` and `on("close", f)` its code and signal and `on("error", f)` a failure to start. `options` are `cwd`, `env`, `shell` and `stdio: "inherit"` for a process that owns the terminal. Terminal pages only. */
  spawn(command: string, args?: string[], options?: SpawnOptions): Spawned {
    return terminalOnly("system.spawn", [command, args ?? [], options ?? null]);
  },
};

export interface SpawnOptions {
  cwd?: string;
  env?: { [name: string]: string };
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

/** A value scoped to a call: `run(value, fn)` holds `value` while `fn` runs, `get()` reads it (`undefined` outside any `run`) and `set(value)` holds it from then on. A terminal page keeps it across the awaits inside `fn`. */
export class Scope<T> {
  #value: T | undefined = undefined;
  run<R>(value: T, fn: (...args: never[]) => R, ...args: never[]): R {
    const before = this.#value;
    this.#value = value;
    try {
      return fn(...args);
    } finally {
      this.#value = before;
    }
  }
  get(): T | undefined {
    return this.#value;
  }
  set(value: T): void {
    this.#value = value;
  }
}

/** A member whose Rust half is the only one: `fsr test` answers it through the runner. A browser has no answer. */
function terminalOnly<T>(name: string, args: unknown[]): T {
  const sf = (globalThis as { __sf?: Bridge }).__sf;
  if (!sf?.ext) throw new Error(`${name} runs in a terminal page only`);
  const json = JSON.stringify(args, (_, v: unknown) => (typeof v === "bigint" ? Number(v) : v === undefined ? null : v));
  return JSON.parse(sf.ext(name, json, currentLocale())) as T;
}

const UNITS: { [unit: string]: number } = { ms: 1, s: 1_000, m: 60_000, h: 3_600_000, d: 86_400_000 };

function unitMs(what: string, unit: string): number {
  const ms = UNITS[unit];
  if (ms === undefined) throw new Error(`${what}: \`${unit}\` is not a unit; ms, s, m, h or d`);
  return ms;
}

const pad = (n: number, width: number) => String(n).padStart(width, "0");

export const time = {
  /** Milliseconds since the time origin, as `performance.now()`; on the server, since the process started. */
  monotonic(): number {
    return performance.now();
  },
  /** `when` in UTC with `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced and every other character kept. */
  format(when: number, pattern: string): string {
    const d = new Date(when);
    const tokens: [string, string][] = [
      ["YYYY", pad(d.getUTCFullYear(), 4)],
      ["SSS", pad(d.getUTCMilliseconds(), 3)],
      ["MM", pad(d.getUTCMonth() + 1, 2)],
      ["DD", pad(d.getUTCDate(), 2)],
      ["HH", pad(d.getUTCHours(), 2)],
      ["mm", pad(d.getUTCMinutes(), 2)],
      ["ss", pad(d.getUTCSeconds(), 2)],
    ];
    let out = "";
    let rest = pattern;
    while (rest.length > 0) {
      const hit = tokens.find(([token]) => rest.startsWith(token));
      if (hit) {
        out += hit[1];
        rest = rest.slice(hit[0].length);
      } else {
        const c = Array.from(rest)[0];
        out += c;
        rest = rest.slice(c.length);
      }
    }
    return out;
  },
  /** The instant `amount` units after `when`. */
  add(when: number, amount: number, unit: string): number {
    return when + amount * unitMs("time.add", unit);
  },
  /** `later` minus `earlier` in units, fractional. */
  diff(later: number, earlier: number, unit: string): number {
    return (later - earlier) / unitMs("time.diff", unit);
  },
  /** `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`, as milliseconds since the epoch; a date alone or a bare time is UTC. `null` for anything else. */
  parse(s: string): number | null {
    const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?(Z|[+-]\d{2}:\d{2})?)?$/.exec(s.trim());
    if (!m) return null;
    const [, y, mo, d, hh, mi, ss, frac, zone] = m;
    const month = Number(mo);
    const day = Number(d);
    if (month < 1 || month > 12 || day < 1 || day > 31) return null;
    let ms = Date.UTC(Number(y), month - 1, day);
    if (hh !== undefined) {
      const h = Number(hh);
      const min = Number(mi);
      const sec = ss === undefined ? 0 : Number(ss);
      if (h > 24 || min > 59 || sec > 59) return null;
      ms += h * 3_600_000 + min * 60_000 + sec * 1_000;
      if (frac !== undefined) ms += Math.floor((Number(frac) / 10 ** frac.length) * 1000);
      if (zone !== undefined && zone !== "Z") {
        const sign = zone[0] === "+" ? -1 : 1;
        ms += sign * (Number(zone.slice(1, 3)) * 3_600_000 + Number(zone.slice(4, 6)) * 60_000);
      }
    }
    return ms;
  },
  /** The clock now, milliseconds since the epoch. Server only on a render path. */
  now(): number {
    return Date.now();
  },
};

function toHex(bytes: ArrayLike<number>): string {
  let out = "";
  for (let i = 0; i < bytes.length; i++) out += bytes[i].toString(16).padStart(2, "0");
  return out;
}

const K = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/** SHA-256 of the UTF-8 bytes of `s`, synchronous, since a render path cannot await `crypto.subtle`. */
function sha256(s: string): Uint8Array {
  const bytes = new TextEncoder().encode(s);
  const length = bytes.length;
  const padded = new Uint8Array(((length + 9 + 63) >> 6) << 6);
  padded.set(bytes);
  padded[length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 4, (length * 8) >>> 0);
  view.setUint32(padded.length - 8, Math.floor((length * 8) / 0x100000000));
  const h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
  const w = new Uint32Array(64);
  const rotr = (x: number, n: number) => (x >>> n) | (x << (32 - n));
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let i = 0; i < 16; i++) w[i] = view.getUint32(offset + i * 4);
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i++) {
      const t1 = (hh + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i]) >>> 0;
      const t2 = ((rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      hh = g;
      g = f;
      f = e;
      e = (d + t1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0;
    h[1] = (h[1] + b) >>> 0;
    h[2] = (h[2] + c) >>> 0;
    h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0;
    h[5] = (h[5] + f) >>> 0;
    h[6] = (h[6] + g) >>> 0;
    h[7] = (h[7] + hh) >>> 0;
  }
  const out = new Uint8Array(32);
  const outView = new DataView(out.buffer);
  h.forEach((word, i) => outView.setUint32(i * 4, word));
  return out;
}

/** A structured line at each level, `fields` whatever goes with the message; on the server it is a `tracing` event under `fsr::app`, here a console line. `fatal` is `error` in the console. */
export const log = {
  /** `message` at trace level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. */
  trace(message: string, fields?: unknown): null {
    console.debug(message, ...(fields === undefined ? [] : [fields]));
    return null;
  },
  /** `message` at debug level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. */
  debug(message: string, fields?: unknown): null {
    console.debug(message, ...(fields === undefined ? [] : [fields]));
    return null;
  },
  /** `message` at info level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. */
  info(message: string, fields?: unknown): null {
    console.info(message, ...(fields === undefined ? [] : [fields]));
    return null;
  },
  /** `message` at warn level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. */
  warn(message: string, fields?: unknown): null {
    console.warn(message, ...(fields === undefined ? [] : [fields]));
    return null;
  },
  /** `message` at error level with `fields` beside it: the server traces it under `fsr::app`, the browser writes it to the console. */
  error(message: string, fields?: unknown): null {
    console.error(message, ...(fields === undefined ? [] : [fields]));
    return null;
  },
  /** `message` at fatal level with `fields` beside it: the server traces it at error under `fsr::app`, the browser writes it to the console. */
  fatal(message: string, fields?: unknown): null {
    console.error(message, ...(fields === undefined ? [] : [fields]));
    return null;
  },
};

export const crypto = {
  /** SHA-256 of `s` as lowercase hex. */
  hash(s: string): string {
    return toHex(sha256(s));
  },
  /** Whether `hash` is the hash of `s`. */
  verify(s: string, hash: string): boolean {
    const computed = toHex(sha256(s));
    const given = hash.toLowerCase();
    let diff = computed.length ^ given.length;
    for (let i = 0; i < Math.min(computed.length, given.length); i++) diff |= computed.charCodeAt(i) ^ given.charCodeAt(i);
    return diff === 0;
  },
  /** `bytes` random bytes as `hex` (the default) or `base64`. Server only on a render path. */
  random(bytes: number, encoding: "hex" | "base64" = "hex"): string {
    const buf = new Uint8Array(Math.max(0, Math.min(1024, bytes)));
    globalThis.crypto.getRandomValues(buf);
    return encoding === "base64" ? btoa(String.fromCharCode(...buf)) : toHex(buf);
  },
  /** A version 4 UUID. Server only on a render path. */
  uuid(): string {
    return globalThis.crypto.randomUUID();
  },
  /** The hash of the parts' UTF-8 bytes in order as `hex` or `base64`; `sha256` is the algorithm provided. */
  digest(algorithm: string, encoding: string, ...parts: string[]): string {
    if (algorithm.toLowerCase().replace("-", "") !== "sha256") throw new Error(`crypto.digest: \`${algorithm}\` is not provided; \`sha256\` is`);
    const bytes = sha256(parts.join(""));
    if (encoding === "hex") return toHex(bytes);
    if (encoding === "base64") return btoa(String.fromCharCode(...bytes));
    throw new Error(`crypto.digest: an encoding of \`${encoding}\`; \`hex\` and \`base64\` are read`);
  },
};

/** The message under `key` in the document's locale, `{name}` placeholders filled from `args`; with `args.count`, the `key.<plural category>` form, then `key.other`, then `key`. A key the catalog lacks answers as itself. Under `fsr test` the runner answers with the Rust half. */
export function t(key: string, args?: { [name: string]: unknown }): string {
  const bridged_ = bridged<string>("i18n.t", [key, args ?? null]);
  if (bridged_ !== undefined) return bridged_;
  const table = catalog(currentLocale());
  const count = args?.count;
  const forms = typeof count === "number" || typeof count === "bigint" ? [`${key}.${intl.plural(count)}`, `${key}.other`, key] : [key];
  const found = table ? forms.map((k) => table[k]).find((v) => v !== undefined) : undefined;
  return interpolate(found ?? key, args);
}

function interpolate(text: string, args?: { [name: string]: unknown }): string {
  if (!args) return text;
  return text.replace(/\{([A-Za-z0-9_]+)\}/g, (whole, name: string) => {
    const value = args[name];
    if (value === undefined || value === null || typeof value === "object") return whole;
    return String(value);
  });
}

export const id = {
  /** A fresh identifier: a UUID, version 7 on the server. Server only on a render path. */
  new(): string {
    return globalThis.crypto.randomUUID();
  },
};

/** Declares the browser half of a native pair under `name`, `module.member`, whose Rust half the host registers under the same name. With `f`, the pair has `render` reach and `f` is what the browser runs; without, it has `body` reach and calling it in the browser throws. The build reads the declaration, so `name` must be a string literal. */
export function native<F extends (...args: never[]) => unknown>(name: string, f?: F): F {
  const registry = ((globalThis as { __sf_natives?: { [name: string]: unknown } }).__sf_natives ??= {});
  if (f) {
    registry[name] = f;
    return f;
  }
  return ((..._: unknown[]) => {
    throw new Error(`${name} runs on the server only`);
  }) as unknown as F;
}
