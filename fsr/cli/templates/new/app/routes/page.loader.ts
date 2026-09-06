import type { Ctx } from "@snapfire/fsr";

export async function load(_ctx: Ctx<"/">) {
  return { greeting: "Hello from Rust" };
}
