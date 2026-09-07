import type { Ctx } from "@snapfire/fsr";

export async function load({ query, path }: Ctx) {
  return { view: query.view ?? "inbox", path };
}
