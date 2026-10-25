import type { Ctx } from "@snapfire/fsr/host";

export async function load(_ctx: Ctx<"/">) {
  return { greeting: "Hello from Rust" };
}
