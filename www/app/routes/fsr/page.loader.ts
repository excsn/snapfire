import type { Ctx } from "@snapfire/fsr";
import { CHAPTERS } from "@src/docs/guide";

export async function load(_ctx: Ctx) {
  return { chapters: CHAPTERS.length };
}

export const meta = () => ({
  title: "SnapFire FSR · A full-stack runtime without Node.js",
  description: "TypeScript loaders, actions and JSX lowered into an execution plan the Rust host evaluates natively. No V8, no node_modules, no cold starts.",
});
