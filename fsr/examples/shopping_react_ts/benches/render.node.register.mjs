// Registers the resolution loader for a child process, which cannot be handed
// the map in memory; FSR_RESOLUTION names the file the bench wrote.
import { readFileSync } from "node:fs";
import { register } from "node:module";

register(new URL("./render.node.loader.mjs", import.meta.url), {
  parentURL: import.meta.url,
  data: JSON.parse(readFileSync(process.env.FSR_RESOLUTION, "utf8")),
});
