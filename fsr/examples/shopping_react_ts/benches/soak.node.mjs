// The V8 half of the soak: N requesters, each an isolate of its own, rendering
// flat out for a fixed window. Server rendering is synchronous CPU work, so an
// event loop cannot interleave it and a requester needs its own isolate.
//
// Run after `cargo bench --bench render`, from this directory:
//   /Users/norm/n/bin/node benches/soak.node.mjs
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { Worker } from "node:worker_threads";

const TEST_DIR = new URL("../app/.fsr-test/", import.meta.url);
const PAGE = "catalog_12";
const COUNTS = [1, 2, 4, 8];
const SECONDS = Number(process.env.SOAK_SECONDS ?? 10);

const workerData = {
  resolution: JSON.parse(readFileSync(new URL("resolution.json", TEST_DIR), "utf8")),
  module: new URL(`bench-${PAGE}.js`, TEST_DIR).href,
  props: new URL(`props-${PAGE}.json`, TEST_DIR).pathname,
};

const state = () => {
  const power = execSync("pmset -g").toString().split("\n").find((l) => l.includes("powermode")) ?? "";
  return `${power.trim()}; ${execSync("uptime").toString().trim()}`;
};

const spawn = (n) =>
  Promise.all(
    Array.from({ length: n }, () => {
      const worker = new Worker(new URL("./soak.node.worker.mjs", import.meta.url), { workerData });
      return new Promise((resolve, reject) => {
        worker.once("message", () => resolve(worker));
        worker.once("error", reject);
      });
    }),
  );

console.error(`node ${process.version}`);
console.error(`machine before: ${state()}`);
console.error(`window ${SECONDS}s per row, catalog with twelve products\n`);
console.log("engine     requesters   renders      renders/sec   system/render   per requester");

for (const n of COUNTS) {
  const workers = await spawn(n);
  const counts = workers.map(
    (worker) =>
      new Promise((resolve) => {
        worker.once("message", resolve);
      }),
  );
  const started = process.hrtime.bigint();
  for (const worker of workers) worker.postMessage("go");
  await new Promise((r) => setTimeout(r, SECONDS * 1000));
  for (const worker of workers) worker.postMessage("stop");
  const done = await Promise.all(counts);
  const elapsed = Number(process.hrtime.bigint() - started) / 1e9;
  // `terminate` rather than waiting on `exit`: a worker that has posted its
  // count still has a live message listener, so nothing would end it.
  await Promise.all(workers.map((w) => w.terminate()));
  const total = done.reduce((a, b) => a + b, 0);
  const perSecond = total / elapsed;
  console.log(
    `v8         ${String(n).padEnd(12)} ${String(total).padEnd(12)} ${perSecond.toFixed(0).padEnd(13)} ${`${(1e6 / perSecond).toFixed(2)} µs`.padEnd(15)} ${((1e6 / perSecond) * n).toFixed(2)} µs`,
  );
}

console.error(`\nmachine after: ${state()}`);
