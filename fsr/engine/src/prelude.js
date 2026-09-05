let now = 0;
let seq = 0;
const timers = [];
const idlers = [];
const pending = new Map();

function schedule(fn, ms, args, repeat) {
  const id = ++seq;
  const delay = Math.max(0, Number(ms) || 0);
  const entry = { id, seq: id, due: now + delay, fn, args, repeat: repeat ? delay : null };
  timers.push(entry);
  return id;
}

function cancel(id) {
  const i = timers.findIndex((t) => t.id === id);
  if (i >= 0) timers.splice(i, 1);
}

globalThis.setTimeout = (fn, ms, ...args) => schedule(fn, ms, args, false);
globalThis.setInterval = (fn, ms, ...args) => schedule(fn, ms, args, true);
globalThis.setImmediate = (fn, ...args) => schedule(fn, 0, args, false);
globalThis.clearTimeout = cancel;
globalThis.clearInterval = cancel;
globalThis.clearImmediate = cancel;
if (typeof globalThis.queueMicrotask !== "function") {
  globalThis.queueMicrotask = (fn) => {
    Promise.resolve().then(fn);
  };
}
globalThis.performance = { now: () => now, timeOrigin: 0 };

function format(args) {
  if (typeof args[0] !== "string" || !/%[sdifoOc]/.test(args[0])) return args.map(text).join(" ");
  let i = 1;
  const head = args[0].replace(/%([sdifoOc])/g, (m, kind) => {
    if (i >= args.length) return m;
    const value = args[i++];
    return kind === "c" ? "" : text(value);
  });
  return [head, ...args.slice(i).map(text)].join(" ");
}

function text(value) {
  if (typeof value === "string") return value;
  if (value instanceof Error) return `${value.name}: ${value.message}${value.stack ? `\n${value.stack}` : ""}`;
  if (typeof value === "bigint") return `${value}n`;
  try {
    return typeof value === "object" && value !== null ? JSON.stringify(value) : String(value);
  } catch {
    return String(value);
  }
}

globalThis.console = {
  log: (...a) => __sf_log("log", format(a)),
  info: (...a) => __sf_log("info", format(a)),
  debug: (...a) => __sf_log("debug", format(a)),
  warn: (...a) => __sf_log("warn", format(a)),
  error: (...a) => __sf_log("error", format(a)),
  trace: (...a) => __sf_log("log", format(a)),
  group: () => {},
  groupEnd: () => {},
  groupCollapsed: () => {},
  table: (a) => __sf_log("log", text(a)),
  assert: (ok, ...a) => {
    if (!ok) __sf_log("error", format(a));
  },
};

globalThis.fetch = (url, init = {}) =>
  new Promise((resolve, reject) => {
    const body = init.body === undefined || init.body === null ? null : String(init.body);
    const headers = [];
    const given = init.headers || {};
    const entries = Array.isArray(given) ? given : Object.entries(given);
    for (const [k, v] of entries) headers.push(String(k), String(v));
    const id = __sf_fetch(String(url), String(init.method || "GET").toUpperCase(), body, headers);
    pending.set(id, { resolve, reject });
  });

globalThis.__sf_complete = (id, status, body, headers) => {
  const p = pending.get(id);
  pending.delete(id);
  if (!p) return;
  const flat = headers || [];
  const pairs = [];
  for (let i = 0; i + 1 < flat.length; i += 2) pairs.push([flat[i], flat[i + 1]]);
  p.resolve({
    ok: status < 400,
    status,
    statusText: "",
    headers: { get: (name) => (pairs.find(([k]) => k.toLowerCase() === String(name).toLowerCase()) || [null, null])[1] },
    json: async () => JSON.parse(body),
    text: async () => body,
  });
};

globalThis.__sf_tick = () => {
  if (timers.length === 0) return false;
  timers.sort((a, b) => a.due - b.due || a.seq - b.seq);
  if (timers[0].due > now) return false;
  const t = timers.shift();
  if (t.repeat !== null) {
    t.due = now + t.repeat;
    t.seq = ++seq;
    timers.push(t);
  }
  t.fn(...t.args);
  return true;
};

globalThis.__sf_flush_idle = () => {
  if (idlers.length === 0) return false;
  for (const r of idlers.splice(0)) r();
  return true;
};

globalThis.__sf = {
  ctx: (spec) => __sf_ctx(spec),
  use: (id) => __sf_use(id),
  session: (id) => __sf_session(id),
  locale: (id) => __sf_locale(id),
  calls: (id) => __sf_calls(id),
  render: (module, props) => __sf_render(module, props),
  idle: () => new Promise((r) => idlers.push(r)),
  advance: (ms) => {
    now += Math.max(0, Number(ms) || 0);
    return new Promise((r) => idlers.push(r));
  },
};

globalThis.location = { href: "http://localhost/", origin: "http://localhost", protocol: "http:", host: "localhost", hostname: "localhost", port: "", pathname: "/", search: "", hash: "", reload() {}, assign(url) { globalThis.__sf_location(url); }, replace(url) { globalThis.__sf_location(url); } };
globalThis.__sf_location = (url) => {
  const u = new URL(String(url), location.href);
  Object.assign(location, { href: u.href, origin: u.origin, protocol: u.protocol, host: u.host, hostname: u.hostname, port: u.port, pathname: u.pathname, search: u.search, hash: u.hash });
};
globalThis.navigator = { userAgent: "fsr-test", language: "en" };
globalThis.history = {
  pushState(state, _title, url) {
    this.state = state;
    this.length++;
    if (url != null) globalThis.__sf_location(url);
  },
  replaceState(state, _title, url) {
    this.state = state;
    if (url != null) globalThis.__sf_location(url);
  },
  state: null,
  length: 1,
};
globalThis.CSS = { escape: (s) => String(s).replace(/([^\w-])/g, "\\$1") };
globalThis.requestAnimationFrame = (fn) => setTimeout(() => fn(now), 16);
globalThis.cancelAnimationFrame = cancel;
globalThis.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} });
globalThis.getComputedStyle = (el) => el.style ?? {};
globalThis.scrollTo = () => {};

class URLSearchParams {
  constructor(init = "") {
    this.list = [];
    if (typeof init === "string") {
      for (const pair of init.replace(/^\?/, "").split("&")) {
        if (!pair) continue;
        const eq = pair.indexOf("=");
        const k = eq < 0 ? pair : pair.slice(0, eq);
        const v = eq < 0 ? "" : pair.slice(eq + 1);
        this.list.push([decodeURIComponent(k.replace(/\+/g, " ")), decodeURIComponent(v.replace(/\+/g, " "))]);
      }
    } else if (init instanceof URLSearchParams) {
      this.list = init.list.map(([k, v]) => [k, v]);
    } else if (Array.isArray(init)) {
      this.list = init.map(([k, v]) => [String(k), String(v)]);
    } else if (init && typeof init === "object") {
      this.list = Object.entries(init).map(([k, v]) => [k, String(v)]);
    }
  }
  append(k, v) {
    this.list.push([String(k), String(v)]);
  }
  delete(k) {
    this.list = this.list.filter(([n]) => n !== k);
  }
  get(k) {
    const hit = this.list.find(([n]) => n === k);
    return hit ? hit[1] : null;
  }
  getAll(k) {
    return this.list.filter(([n]) => n === k).map(([, v]) => v);
  }
  has(k) {
    return this.list.some(([n]) => n === k);
  }
  set(k, v) {
    const i = this.list.findIndex(([n]) => n === k);
    if (i < 0) this.list.push([String(k), String(v)]);
    else {
      this.list[i] = [String(k), String(v)];
      this.list = this.list.filter(([n], j) => n !== k || j === i);
    }
  }
  sort() {
    this.list.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  }
  forEach(fn, self) {
    for (const [k, v] of this.list) fn.call(self, v, k, this);
  }
  keys() {
    return this.list.map(([k]) => k)[Symbol.iterator]();
  }
  values() {
    return this.list.map(([, v]) => v)[Symbol.iterator]();
  }
  entries() {
    return this.list.map(([k, v]) => [k, v])[Symbol.iterator]();
  }
  [Symbol.iterator]() {
    return this.entries();
  }
  get size() {
    return this.list.length;
  }
  toString() {
    return this.list.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&");
  }
}

class URL {
  constructor(input, base) {
    const text = String(input);
    const absolute = /^[a-z][a-z0-9+.-]*:/i.exec(text);
    let source = text;
    if (!absolute) {
      if (base === undefined) throw new TypeError(`Invalid URL: ${text}`);
      const b = base instanceof URL ? base : new URL(String(base));
      if (text.startsWith("//")) source = `${b.protocol}${text}`;
      else if (text.startsWith("/")) source = `${b.origin}${text}`;
      else if (text.startsWith("?")) source = `${b.origin}${b.pathname}${text}`;
      else if (text.startsWith("#")) source = `${b.origin}${b.pathname}${b.search}${text}`;
      else {
        const dir = b.pathname.slice(0, b.pathname.lastIndexOf("/") + 1);
        source = `${b.origin}${dir}${text}`;
      }
    }
    const m = /^([a-z][a-z0-9+.-]*:)(?:\/\/(?:([^@/?#]*)@)?([^:/?#]*)(?::(\d+))?)?([^?#]*)(\?[^#]*)?(#.*)?$/i.exec(source);
    if (!m) throw new TypeError(`Invalid URL: ${text}`);
    this.protocol = m[1].toLowerCase();
    const auth = m[2] ? m[2].split(":") : ["", ""];
    this.username = auth[0] ?? "";
    this.password = auth[1] ?? "";
    this.hostname = (m[3] ?? "").toLowerCase();
    this.port = m[4] ?? "";
    const segments = [];
    for (const part of (m[5] || "/").split("/")) {
      if (part === "..") segments.pop();
      else if (part !== "." && (part !== "" || segments.length === 0)) segments.push(part);
    }
    let pathname = segments.join("/");
    if (!pathname.startsWith("/")) pathname = `/${pathname}`;
    if ((m[5] || "/").endsWith("/") && !pathname.endsWith("/")) pathname += "/";
    this.pathname = pathname;
    this.search = m[6] && m[6] !== "?" ? m[6] : "";
    this.hash = m[7] && m[7] !== "#" ? m[7] : "";
    this.searchParams = new URLSearchParams(this.search);
  }
  get host() {
    return this.port ? `${this.hostname}:${this.port}` : this.hostname;
  }
  get origin() {
    return `${this.protocol}//${this.host}`;
  }
  get href() {
    return `${this.origin}${this.pathname}${this.search}${this.hash}`;
  }
  toString() {
    return this.href;
  }
  toJSON() {
    return this.href;
  }
  static canParse(input, base) {
    try {
      new URL(input, base);
      return true;
    } catch {
      return false;
    }
  }
}

globalThis.URL = URL;
globalThis.URLSearchParams = URLSearchParams;

class TextEncoder {
  get encoding() {
    return "utf-8";
  }
  encode(input = "") {
    const text = String(input);
    const out = [];
    for (let i = 0; i < text.length; i++) {
      let c = text.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
        const d = text.charCodeAt(i + 1);
        if (d >= 0xdc00 && d <= 0xdfff) {
          c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
          i++;
        }
      }
      if (c < 0x80) out.push(c);
      else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
      else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
      else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 0x3f), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
    }
    return Uint8Array.from(out);
  }
  encodeInto(input, dest) {
    const bytes = this.encode(input);
    const n = Math.min(bytes.length, dest.length);
    dest.set(bytes.subarray(0, n));
    return { read: String(input).length, written: n };
  }
}

class TextDecoder {
  constructor(label = "utf-8") {
    this.encoding = String(label).toLowerCase();
  }
  decode(input) {
    if (input === undefined) return "";
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input.buffer ?? input);
    let out = "";
    for (let i = 0; i < bytes.length; ) {
      const b = bytes[i];
      let c;
      let n;
      if (b < 0x80) {
        c = b;
        n = 1;
      } else if (b >= 0xf0) {
        c = ((b & 0x07) << 18) | ((bytes[i + 1] & 0x3f) << 12) | ((bytes[i + 2] & 0x3f) << 6) | (bytes[i + 3] & 0x3f);
        n = 4;
      } else if (b >= 0xe0) {
        c = ((b & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f);
        n = 3;
      } else {
        c = ((b & 0x1f) << 6) | (bytes[i + 1] & 0x3f);
        n = 2;
      }
      out += String.fromCodePoint(c);
      i += n;
    }
    return out;
  }
}

globalThis.TextEncoder = TextEncoder;
globalThis.TextDecoder = TextDecoder;
