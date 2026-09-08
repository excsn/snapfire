// One requester rendering flat out until told to stop.
import { readFileSync } from "node:fs";
import { register } from "node:module";
import { parentPort, workerData } from "node:worker_threads";

register(new URL("./render.node.loader.mjs", import.meta.url), { parentURL: import.meta.url, data: workerData.resolution });

await import(workerData.module);
const props = globalThis.__decode(readFileSync(workerData.props, "utf8"));
globalThis.__render(props);

let done = 0;
let running = false;

function burn() {
  if (!running) return;
  for (let i = 0; i < 16; i++) globalThis.__render(props);
  done += 16;
  setImmediate(burn);
}

parentPort.on("message", (message) => {
  if (message === "go") {
    running = true;
    burn();
  } else {
    running = false;
    parentPort.postMessage(done);
  }
});
parentPort.postMessage("ready");
