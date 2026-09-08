// The V8 half of docs/benches/render.md: React's own renderToString under
// Node, on the pages and props `cargo bench --bench render` already prepared.
// Run it after that bench, from this directory:
//   node benches/render.node.mjs
import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { execSync } from "node:child_process";
import { register } from "node:module";
import { pathToFileURL } from "node:url";

const TEST_DIR = new URL("../app/.fsr-test/", import.meta.url);
const PAGES = ["catalog_12", "product", "cart_3"];
const ITERATIONS = 2000;
const WARMUP = 500;
const COLD_RUNS = 10;

const missing = ["resolution.json", ...PAGES.map((p) => `props-${p}.json`)].filter(
  (f) => !existsSync(new URL(f, TEST_DIR)),
);
if (missing.length) {
  console.error(`missing ${missing.join(", ")}: run \`cargo bench --bench render\` first`);
  process.exit(2);
}

const resolution = JSON.parse(readFileSync(new URL("resolution.json", TEST_DIR), "utf8"));
register(new URL("./render.node.loader.mjs", import.meta.url), {
  parentURL: import.meta.url,
  data: resolution,
});

const state = () => {
  const power = execSync("pmset -g").toString().split("\n").find((l) => l.includes("powermode")) ?? "";
  return `${power.trim()}; ${execSync("uptime").toString().trim()}`;
};

console.log(`node ${process.version}`);
console.log(`machine before: ${state()}`);

const rows = [];
for (const page of PAGES) {
  await import(new URL(`bench-${page}.js`, TEST_DIR).href);
  const props = globalThis.__decode(readFileSync(new URL(`props-${page}.json`, TEST_DIR), "utf8"));

  const expected = readFileSync(new URL(`render-${page}.react.html`, TEST_DIR), "utf8");
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
  const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
  const median = samples[Math.floor(samples.length / 2)];
  const p5 = samples[Math.floor(samples.length * 0.05)];
  const p95 = samples[Math.floor(samples.length * 0.95)];
  rows.push({ page, mean, median, p5, p95, fidelity, bytes: got.length });
}

// The counterpart to `quickjs/cold_context`, one level coarser: a whole node
// process, its module graph and one render. Not the same unit as a fresh
// context inside a warm process, and labelled so.
const cold = [];
for (const page of PAGES) {
  const script = `import("${new URL(`bench-${page}.js`, TEST_DIR).href}").then(async () => {` +
    `const fs = await import("node:fs");` +
    `const props = globalThis.__decode(fs.readFileSync(${JSON.stringify(new URL(`props-${page}.json`, TEST_DIR).pathname)}, "utf8"));` +
    `globalThis.__render(props);});`;
  const runs = [];
  for (let i = 0; i < COLD_RUNS; i++) {
    const t = process.hrtime.bigint();
    execSync(`${process.execPath} --import ${JSON.stringify(new URL("./render.node.register.mjs", import.meta.url).href)} --input-type=module -e ${JSON.stringify(script)}`, {
      stdio: "ignore",
      env: { ...process.env, FSR_RESOLUTION: new URL("resolution.json", TEST_DIR).pathname },
    });
    runs.push(Number(process.hrtime.bigint() - t) / 1e6);
  }
  runs.sort((a, b) => a - b);
  cold.push({ page, median: runs[Math.floor(runs.length / 2)] });
}

console.log(`machine after: ${state()}`);
console.log("");
console.log("| Benchmark | p5 | median | mean | p95 | fidelity |");
console.log("| --- | --- | --- | --- | --- | --- |");
for (const r of rows) {
  const us = (n) => `${n.toFixed(2)} µs`;
  console.log(`| \`v8/render/${r.page}\` | ${us(r.p5)} | ${us(r.median)} | ${us(r.mean)} | ${us(r.p95)} | ${r.fidelity} |`);
}
for (const r of cold) {
  console.log(`| \`v8/cold_process/${r.page}\` | | ${r.median.toFixed(2)} ms | | | |`);
}
