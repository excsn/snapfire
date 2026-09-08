// React in V8 under concurrent load: N requesters, each an isolate of its own,
// because server rendering is synchronous CPU work that an event loop cannot
// interleave. Run after `cargo bench --bench render`, from this directory:
//   /Users/norm/n/bin/node benches/concurrency.node.mjs
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { Worker } from "node:worker_threads";

const TEST_DIR = new URL("../app/.fsr-test/", import.meta.url);
const PAGE = "catalog_12";
const COUNTS = [1, 2, 4, 8];
const RENDERS_PER_WORKER = 50;
const ROUNDS = 40;
const WARMUP_ROUNDS = 10;

const resolution = JSON.parse(readFileSync(new URL("resolution.json", TEST_DIR), "utf8"));
const workerData = {
  resolution,
  module: new URL(`bench-${PAGE}.js`, TEST_DIR).href,
  props: new URL(`props-${PAGE}.json`, TEST_DIR).pathname,
  renders: RENDERS_PER_WORKER,
};

const state = () => {
  const power = execSync("pmset -g").toString().split("\n").find((l) => l.includes("powermode")) ?? "";
  return `${power.trim()}; ${execSync("uptime").toString().trim()}`;
};

const spawn = (n) =>
  Promise.all(
    Array.from({ length: n }, () => {
      const worker = new Worker(new URL("./concurrency.node.worker.mjs", import.meta.url), { workerData });
      return new Promise((resolve, reject) => {
        worker.once("message", () => resolve(worker));
        worker.once("error", reject);
      });
    }),
  );

const round = (workers) =>
  Promise.all(
    workers.map(
      (worker) =>
        new Promise((resolve) => {
          worker.once("message", resolve);
          worker.postMessage("go");
        }),
    ),
  );

console.log(`node ${process.version}`);
console.log(`machine before: ${state()}`);

const rows = [];
for (const n of COUNTS) {
  const workers = await spawn(n);
  for (let i = 0; i < WARMUP_ROUNDS; i++) await round(workers);
  const samples = [];
  for (let i = 0; i < ROUNDS; i++) {
    const t = process.hrtime.bigint();
    await round(workers);
    samples.push(Number(process.hrtime.bigint() - t) / 1000);
  }
  for (const worker of workers) worker.postMessage("stop");
  await Promise.all(workers.map((w) => new Promise((r) => w.once("exit", r))));
  samples.sort((a, b) => a - b);
  const median = samples[Math.floor(samples.length / 2)];
  rows.push({ n, median, perRender: median / (n * RENDERS_PER_WORKER) });
}

console.log(`machine after: ${state()}`);
console.log("");
console.log("requesters   wall (median)   per render   throughput");
const base = rows[0].perRender;
for (const r of rows) {
  console.log(
    `${String(r.n).padEnd(12)} ${(r.median / 1000).toFixed(3).padStart(9)} ms ${r.perRender.toFixed(2).padStart(11)} µs ${(base / r.perRender).toFixed(2).padStart(11)}x`,
  );
}
