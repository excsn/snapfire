import type { Ctx } from "@snapfire/fsr/host";

export async function load({ query }: Ctx) {
  return { track: query.track ?? "all" };
}
