import type { Ctx } from "@snapfire/fsr/host";

export async function load({ query }: Ctx) {
  return { tally: Number(query.tally ?? "0") };
}
