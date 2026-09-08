// The fair version of the concurrency comparison: N separate node processes
// rather than N isolates in one, which is what a Node server under CPU load
// actually does. If this scales where `soak.node.mjs` plateaus, the finding is
// about worker threads rather than about V8.
//
// Run after `cargo bench --bench render`, from this directory:
//   /Users/norm/n/bin/node benches/cluster.node.mjs
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { fork } from "node:child_process";

const TEST_DIR = new URL("../app/.fsr-test/", import.meta.url);
const PAGE = "catalog_12";
const COUNTS = [1, 2, 4, 8];
const SECONDS = Number(process.env.SOAK_SECONDS ?? 10);

const child = {
  resolution: new URL("resolution.json", TEST_DIR).pathname,
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
      const proc = fork(new URL("./cluster.node.child.mjs", import.meta.url), [], { env: { ...process.env, FSR_CHILD: JSON.stringify(child) } });
      return new Promise((resolve, reject) => {
        proc.once("message", () => resolve(proc));
        proc.once("error", reject);
      });
    }),
  );

console.error(`node ${process.version}`);
console.error(`machine before: ${state()}`);
console.error(`window ${SECONDS}s per row, ${PAGE}, one process per requester\n`);
console.log("engine     requesters   renders      renders/sec   system/render   per requester");

for (const n of COUNTS) {
  const procs = await spawn(n);
  const counts = procs.map((proc) => new Promise((resolve) => proc.once("message", resolve)));
  const started = process.hrtime.bigint();
  for (const proc of procs) proc.send("go");
  await new Promise((r) => setTimeout(r, SECONDS * 1000));
  for (const proc of procs) proc.send("stop");
  const done = await Promise.all(counts);
  const elapsed = Number(process.hrtime.bigint() - started) / 1e9;
  for (const proc of procs) proc.kill();
  const total = done.reduce((a, b) => a + b, 0);
  const perSecond = total / elapsed;
  console.log(
    `v8-fork    ${String(n).padEnd(12)} ${String(total).padEnd(12)} ${perSecond.toFixed(0).padEnd(13)} ${`${(1e6 / perSecond).toFixed(2)} µs`.padEnd(15)} ${((1e6 / perSecond) * n).toFixed(2)} µs`,
  );
}

console.error(`\nmachine after: ${state()}`);
