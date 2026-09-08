// One requester: its own isolate, its own module graph, its own React.
import { readFileSync } from "node:fs";
import { register } from "node:module";
import { parentPort, workerData } from "node:worker_threads";

register(new URL("./render.node.loader.mjs", import.meta.url), { parentURL: import.meta.url, data: workerData.resolution });

await import(workerData.module);
const props = globalThis.__decode(readFileSync(workerData.props, "utf8"));
globalThis.__render(props);

parentPort.on("message", (message) => {
  if (message === "stop") {
    process.exit(0);
  }
  for (let i = 0; i < workerData.renders; i++) globalThis.__render(props);
  parentPort.postMessage("done");
});
parentPort.postMessage("ready");
