// One requester in a process of its own: its own V8, its own heap, its own
// copy of React and the page.
import { readFileSync } from "node:fs";
import { register } from "node:module";

const spec = JSON.parse(process.env.FSR_CHILD);
register(new URL("./render.node.loader.mjs", import.meta.url), { parentURL: import.meta.url, data: JSON.parse(readFileSync(spec.resolution, "utf8")) });

await import(spec.module);
const props = globalThis.__decode(readFileSync(spec.props, "utf8"));
globalThis.__render(props);

let done = 0;
let running = false;

function burn() {
  if (!running) return;
  for (let i = 0; i < 16; i++) globalThis.__render(props);
  done += 16;
  setImmediate(burn);
}

process.on("message", (message) => {
  if (message === "go") {
    running = true;
    burn();
  } else {
    running = false;
    process.send(done);
  }
});
process.send("ready");
