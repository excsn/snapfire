// What V8 computes for a `Math` function over generated inputs, for
// tests/jsmath.rs. Evaluate it in a browser with `NAME`, `SEED` and `COUNT`
// replaced, e.g. through `browserc eval`: it answers `digest` as tests/jsmath.rs
// computes it (every input and result as hex when `LIST` is true).
(() => {
  const name = "NAME", seed = SEED, count = COUNT, list = LIST;
  const bands = {
    sin: [[0, -30, 30], [0, -1022, 1023]],
    cos: [[0, -30, 30], [0, -1022, 1023]],
    tan: [[0, -30, 30], [0, -1022, 1023]],
    exp: [[0, -30, 9], [0, -60, 10]],
    expm1: [[0, -60, 9], [0, -30, 10]],
    log: [[1, -1022, 1023], [1, -3, 3]],
    log2: [[1, -1022, 1023], [1, -3, 3]],
    log10: [[1, -1022, 1023], [1, -3, 3]],
    log1p: [[1, -1022, 1023], [2, -60, -1]],
    cbrt: [[0, -1022, 1023], [0, -10, 10]],
    asin: [[0, -60, -1], [0, -4, -1]],
    acos: [[0, -60, -1], [0, -4, -1]],
    atan: [[0, -1022, 1023], [0, -30, 30]],
    atan2: [[0, -1022, 1023], [0, -30, 30]],
    sinh: [[0, -30, 9], [0, -60, 10]],
    cosh: [[0, -30, 9], [0, -60, 10]],
    asinh: [[0, -1022, 1023], [0, -30, 30]],
    acosh: [[1, 0, 1023], [1, 0, 3]],
    atanh: [[0, -60, -1], [0, -4, -1]],
  }[name];
  let s = seed >>> 0 || 1;
  const next = () => { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s; };
  const buf = new ArrayBuffer(8), f = new Float64Array(buf), u = new Uint32Array(buf);
  // Band [sign, emin, emax]: sign 0 either, 1 positive, 2 negative; the unbiased exponent from emin to emax.
  const input = () => {
    const [sign, emin, emax] = bands[next() % bands.length];
    const e = 1023 + emin + (next() % (emax - emin + 1));
    const neg = sign === 2 || (sign === 0 && (next() & 1) === 1);
    u[0] = next();
    u[1] = ((neg ? 0x80000000 : 0) | (e << 20) | (next() & 0xfffff)) >>> 0;
    return f[0];
  };
  const fn = Math[name];
  let h0 = 0x811c9dc5, h1 = 0x811c9dc5;
  const mix = (h, w) => { for (let i = 0; i < 4; i++) { h ^= (w >>> (8 * i)) & 0xff; h = Math.imul(h, 0x01000193) >>> 0; } return h; };
  const hex = (x) => { f[0] = x; return u[1].toString(16).padStart(8, "0") + u[0].toString(16).padStart(8, "0"); };
  const lines = [];
  for (let i = 0; i < count; i++) {
    const x = input();
    const y = fn.length === 2 ? input() : undefined;
    const r = y === undefined ? fn(x) : fn(x, y);
    f[0] = r;
    h0 = mix(h0, u[0]);
    h1 = mix(h1, u[1]);
    if (list) lines.push(hex(x) + (y === undefined ? "" : " " + hex(y)) + " " + hex(r));
  }
  return list ? lines.join("\n") : h1.toString(16).padStart(8, "0") + h0.toString(16).padStart(8, "0");
})()
