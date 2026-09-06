import type { Ctx } from "@snapfire/fsr";
import { CHAPTERS } from "@src/docs/guide";

export async function load(_ctx: Ctx<"/">) {
  return { chapters: CHAPTERS.length };
}

export const meta = () => ({
  title: "SnapFire · Rust web tooling without Node.js",
  description: "SnapFire FSR is a full-stack runtime that lowers TypeScript into an execution plan Rust evaluates. SnapFire Compiler builds TypeScript for the browser. The snapfire crate serves Tera templates over Actix with live reload.",
});
