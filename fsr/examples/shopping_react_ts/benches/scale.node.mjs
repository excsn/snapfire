// React in V8 on the list lengths `cargo bench --bench scale` prepared, so the
// two renderers are compared on the axis every per-item change is judged on.
//
// Needs both Rust benches to have run first: `render` writes `bench-catalog_12.js`
// and `resolution.json`, `scale` writes `props-list-<n>.json` and the IR's own
// markup to check against. Run from this directory:
//   node benches/scale.node.mjs
//
// Only the list axis is here. `depth` and `compute` are synthetic IR with no
// TSX behind them, so React cannot render them and the crossover this bench
// most wants, the point where a JIT beats the interpreter on arithmetic, needs
// a real component in the app before it can be measured.

import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { register } from "node:module";

const TEST_DIR = new URL("../app/.fsr-test/", import.meta.url);
const LENGTHS = [1, 12, 100, 1000];
const ITERATIONS = 500;
const WARMUP = 200;

const missing = ["resolution.json", "bench-catalog_12.js", ...LENGTHS.flatMap((n) => [`props-list-${n}.json`, `render-list-${n}.ir.html`])].filter(
  (f) => {
    try {
      readFileSync(new URL(f, TEST_DIR));
      return false;
    } catch {
      return true;
    }
  },
);
if (missing.length) {
  console.error(`missing ${missing.join(", ")}: run cargo bench --bench render and --bench scale first`);
  process.exit(1);
}

const state = () => {
  const power = execSync("pmset -g").toString().split("\n").find((l) => l.includes("powermode")) ?? "";
  return `${power.trim()}; ${execSync("uptime").toString().trim()}`;
};

register(new URL("./render.node.loader.mjs", import.meta.url), {
  parentURL: import.meta.url,
  data: JSON.parse(readFileSync(new URL("resolution.json", TEST_DIR), "utf8")),
});

await import(new URL("bench-catalog_12.js", TEST_DIR).href);

console.log(`node ${process.version}`);
console.log(`machine before: ${state()}`);

const rows = [];
for (const n of LENGTHS) {
  const props = globalThis.__decode(readFileSync(new URL(`props-list-${n}.json`, TEST_DIR), "utf8"));
  const expected = readFileSync(new URL(`render-list-${n}.ir.html`, TEST_DIR), "utf8");
  const got = globalThis.__render(props);
  const fidelity = got === expected ? "identical" : "DIFFERENT";

  for (let i = 0; i < WARMUP; i++) globalThis.__render(props);
  const samples = [];
  for (let i = 0; i < ITERATIONS; i++) {
    const t = process.hrtime.bigint();
    globalThis.__render(props);
    samples.push(Number(process.hrtime.bigint() - t) / 1000);
  }
  samples.sort((a, b) => a - b);
  rows.push({
    n,
    fidelity,
    bytes: got.length,
    median: samples[Math.floor(samples.length / 2)],
    p5: samples[Math.floor(samples.length * 0.05)],
    p95: samples[Math.floor(samples.length * 0.95)],
  });
}

console.log(`machine after: ${state()}`);
console.log("");
console.log("items   median      p5       p95      bytes   vs IR");
for (const r of rows) {
  const per = (r.median / r.n).toFixed(3);
  console.log(`${String(r.n).padEnd(7)} ${r.median.toFixed(2).padStart(8)} ${r.p5.toFixed(2).padStart(8)} ${r.p95.toFixed(2).padStart(9)} ${String(r.bytes).padStart(8)}   ${per} us/item   ${r.fidelity}`);
}
if (rows.some((r) => r.fidelity !== "identical")) {
  console.error("\nfidelity check failed: the two renderers disagree, so the timings are not comparable");
  process.exit(1);
}
