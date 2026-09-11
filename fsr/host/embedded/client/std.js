import { catalog, currentLocale } from "./locale.js";
export function localeTag() {
    const tag = currentLocale();
    return tag ? tag.replace(/_/g, "-") : "en";
}
function bridged(name, args) {
    if (typeof Intl !== "undefined") return undefined;
    const sf = globalThis.__sf;
    if (!sf?.ext) throw new Error(`${name} needs Intl, which this runtime does not have`);
    const json = JSON.stringify(args, (_, v)=>typeof v === "bigint" ? Number(v) : v === undefined ? null : v);
    return JSON.parse(sf.ext(name, json, currentLocale()));
}
export const intl = {
    number (n, options) {
        return bridged("intl.number", [
            n,
            options ?? null
        ]) ?? new Intl.NumberFormat(localeTag(), options).format(n);
    },
    currency (n, code) {
        return bridged("intl.currency", [
            n,
            code
        ]) ?? new Intl.NumberFormat(localeTag(), {
            style: "currency",
            currency: code,
            currencyDisplay: "code"
        }).format(n);
    },
    date (when, style = "medium") {
        const ms = typeof when === "string" ? time.parse(when) : when;
        if (ms === null) throw new Error(`intl.date: \`${when}\` is not an ISO 8601 date`);
        return bridged("intl.date", [
            ms,
            style
        ]) ?? new Intl.DateTimeFormat(localeTag(), {
            dateStyle: style,
            timeZone: "UTC"
        }).format(new Date(ms));
    },
    plural (n) {
        return bridged("intl.plural", [
            n
        ]) ?? new Intl.PluralRules(localeTag()).select(Number(n));
    }
};
export const text = {
    slug (s) {
        return s.normalize("NFD").replace(/\p{M}+/gu, "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
    },
    truncate (s, max, ellipsis = "…") {
        const chars = Array.from(s);
        if (chars.length <= Math.max(0, max)) return s;
        return chars.slice(0, Math.max(0, max)).join("") + ellipsis;
    }
};
const UNITS = {
    ms: 1,
    s: 1_000,
    m: 60_000,
    h: 3_600_000,
    d: 86_400_000
};
function unitMs(what, unit) {
    const ms = UNITS[unit];
    if (ms === undefined) throw new Error(`${what}: \`${unit}\` is not a unit; ms, s, m, h or d`);
    return ms;
}
const pad = (n, width)=>String(n).padStart(width, "0");
export const time = {
    format (when, pattern) {
        const d = new Date(when);
        const tokens = [
            [
                "YYYY",
                pad(d.getUTCFullYear(), 4)
            ],
            [
                "SSS",
                pad(d.getUTCMilliseconds(), 3)
            ],
            [
                "MM",
                pad(d.getUTCMonth() + 1, 2)
            ],
            [
                "DD",
                pad(d.getUTCDate(), 2)
            ],
            [
                "HH",
                pad(d.getUTCHours(), 2)
            ],
            [
                "mm",
                pad(d.getUTCMinutes(), 2)
            ],
            [
                "ss",
                pad(d.getUTCSeconds(), 2)
            ]
        ];
        let out = "";
        let rest = pattern;
        while(rest.length > 0){
            const hit = tokens.find(([token])=>rest.startsWith(token));
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
    add (when, amount, unit) {
        return when + amount * unitMs("time.add", unit);
    },
    diff (later, earlier, unit) {
        return (later - earlier) / unitMs("time.diff", unit);
    },
    parse (s) {
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
            if (frac !== undefined) ms += Math.floor(Number(frac) / 10 ** frac.length * 1000);
            if (zone !== undefined && zone !== "Z") {
                const sign = zone[0] === "+" ? -1 : 1;
                ms += sign * (Number(zone.slice(1, 3)) * 3_600_000 + Number(zone.slice(4, 6)) * 60_000);
            }
        }
        return ms;
    },
    now () {
        return Date.now();
    }
};
function toHex(bytes) {
    let out = "";
    for(let i = 0; i < bytes.length; i++)out += bytes[i].toString(16).padStart(2, "0");
    return out;
}
const K = [
    0x428a2f98,
    0x71374491,
    0xb5c0fbcf,
    0xe9b5dba5,
    0x3956c25b,
    0x59f111f1,
    0x923f82a4,
    0xab1c5ed5,
    0xd807aa98,
    0x12835b01,
    0x243185be,
    0x550c7dc3,
    0x72be5d74,
    0x80deb1fe,
    0x9bdc06a7,
    0xc19bf174,
    0xe49b69c1,
    0xefbe4786,
    0x0fc19dc6,
    0x240ca1cc,
    0x2de92c6f,
    0x4a7484aa,
    0x5cb0a9dc,
    0x76f988da,
    0x983e5152,
    0xa831c66d,
    0xb00327c8,
    0xbf597fc7,
    0xc6e00bf3,
    0xd5a79147,
    0x06ca6351,
    0x14292967,
    0x27b70a85,
    0x2e1b2138,
    0x4d2c6dfc,
    0x53380d13,
    0x650a7354,
    0x766a0abb,
    0x81c2c92e,
    0x92722c85,
    0xa2bfe8a1,
    0xa81a664b,
    0xc24b8b70,
    0xc76c51a3,
    0xd192e819,
    0xd6990624,
    0xf40e3585,
    0x106aa070,
    0x19a4c116,
    0x1e376c08,
    0x2748774c,
    0x34b0bcb5,
    0x391c0cb3,
    0x4ed8aa4a,
    0x5b9cca4f,
    0x682e6ff3,
    0x748f82ee,
    0x78a5636f,
    0x84c87814,
    0x8cc70208,
    0x90befffa,
    0xa4506ceb,
    0xbef9a3f7,
    0xc67178f2
];
function sha256(s) {
    const bytes = new TextEncoder().encode(s);
    const length = bytes.length;
    const padded = new Uint8Array(length + 9 + 63 >> 6 << 6);
    padded.set(bytes);
    padded[length] = 0x80;
    const view = new DataView(padded.buffer);
    view.setUint32(padded.length - 4, length * 8 >>> 0);
    view.setUint32(padded.length - 8, Math.floor(length * 8 / 0x100000000));
    const h = [
        0x6a09e667,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19
    ];
    const w = new Uint32Array(64);
    const rotr = (x, n)=>x >>> n | x << 32 - n;
    for(let offset = 0; offset < padded.length; offset += 64){
        for(let i = 0; i < 16; i++)w[i] = view.getUint32(offset + i * 4);
        for(let i = 16; i < 64; i++){
            const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ w[i - 15] >>> 3;
            const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ w[i - 2] >>> 10;
            w[i] = w[i - 16] + s0 + w[i - 7] + s1 >>> 0;
        }
        let [a, b, c, d, e, f, g, hh] = h;
        for(let i = 0; i < 64; i++){
            const t1 = hh + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + (e & f ^ ~e & g) + K[i] + w[i] >>> 0;
            const t2 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + (a & b ^ a & c ^ b & c) >>> 0;
            hh = g;
            g = f;
            f = e;
            e = d + t1 >>> 0;
            d = c;
            c = b;
            b = a;
            a = t1 + t2 >>> 0;
        }
        h[0] = h[0] + a >>> 0;
        h[1] = h[1] + b >>> 0;
        h[2] = h[2] + c >>> 0;
        h[3] = h[3] + d >>> 0;
        h[4] = h[4] + e >>> 0;
        h[5] = h[5] + f >>> 0;
        h[6] = h[6] + g >>> 0;
        h[7] = h[7] + hh >>> 0;
    }
    const out = new Uint8Array(32);
    const outView = new DataView(out.buffer);
    h.forEach((word, i)=>outView.setUint32(i * 4, word));
    return out;
}
export const crypto = {
    hash (s) {
        return toHex(sha256(s));
    },
    verify (s, hash) {
        const computed = toHex(sha256(s));
        const given = hash.toLowerCase();
        let diff = computed.length ^ given.length;
        for(let i = 0; i < Math.min(computed.length, given.length); i++)diff |= computed.charCodeAt(i) ^ given.charCodeAt(i);
        return diff === 0;
    },
    random (bytes) {
        const buf = new Uint8Array(Math.max(0, Math.min(1024, bytes)));
        globalThis.crypto.getRandomValues(buf);
        return toHex(buf);
    }
};
export function t(key, args) {
    const bridged_ = bridged("i18n.t", [
        key,
        args ?? null
    ]);
    if (bridged_ !== undefined) return bridged_;
    const table = catalog(currentLocale());
    const count = args?.count;
    const forms = typeof count === "number" || typeof count === "bigint" ? [
        `${key}.${intl.plural(count)}`,
        `${key}.other`,
        key
    ] : [
        key
    ];
    const found = table ? forms.map((k)=>table[k]).find((v)=>v !== undefined) : undefined;
    return interpolate(found ?? key, args);
}
function interpolate(text, args) {
    if (!args) return text;
    return text.replace(/\{([A-Za-z0-9_]+)\}/g, (whole, name)=>{
        const value = args[name];
        if (value === undefined || value === null || typeof value === "object") return whole;
        return String(value);
    });
}
export const id = {
    new () {
        return globalThis.crypto.randomUUID();
    }
};
export function native(name, f) {
    const registry = globalThis.__sf_natives ??= {};
    if (f) {
        registry[name] = f;
        return f;
    }
    return (..._)=>{
        throw new Error(`${name} runs on the server only`);
    };
}
//# sourceMappingURL=std.js.map
