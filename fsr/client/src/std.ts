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
}

export type DateStyle = "short" | "medium" | "long" | "full";

export const intl = {
  /** `n` grouped for the locale, rounded half away from zero to at most three fraction digits unless `options` say otherwise, trailing zeros dropped past the minimum. */
  number(n: number | bigint, options?: NumberOptions): string {
    return bridged<string>("intl.number", [n, options ?? null]) ?? new Intl.NumberFormat(localeTag(), options).format(n);
  },
  /** `n` as an amount of the ISO currency `code`, the code spelled out, the currency's own fraction digits. */
  currency(n: number | bigint, code: string): string {
    return bridged<string>("intl.currency", [n, code]) ?? new Intl.NumberFormat(localeTag(), { style: "currency", currency: code, currencyDisplay: "code" }).format(n);
  },
  /** The calendar date of `when`, milliseconds since the epoch or an ISO 8601 string, in UTC at `style`, `medium` by default. */
  date(when: number | string, style: DateStyle = "medium"): string {
    const ms = typeof when === "string" ? time.parse(when) : when;
    if (ms === null) throw new Error(`intl.date: \`${when}\` is not an ISO 8601 date`);
    return bridged<string>("intl.date", [ms, style]) ?? new Intl.DateTimeFormat(localeTag(), { dateStyle: style, timeZone: "UTC" }).format(new Date(ms));
  },
  /** The cardinal plural category of `n`: `zero`, `one`, `two`, `few`, `many` or `other`. */
  plural(n: number | bigint): string {
    return bridged<string>("intl.plural", [n]) ?? new Intl.PluralRules(localeTag()).select(Number(n));
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
};

const UNITS: { [unit: string]: number } = { ms: 1, s: 1_000, m: 60_000, h: 3_600_000, d: 86_400_000 };

function unitMs(what: string, unit: string): number {
  const ms = UNITS[unit];
  if (ms === undefined) throw new Error(`${what}: \`${unit}\` is not a unit; ms, s, m, h or d`);
  return ms;
}

const pad = (n: number, width: number) => String(n).padStart(width, "0");

export const time = {
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
  /** `bytes` random bytes as hex. Server only on a render path. */
  random(bytes: number): string {
    const buf = new Uint8Array(Math.max(0, Math.min(1024, bytes)));
    globalThis.crypto.getRandomValues(buf);
    return toHex(buf);
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
