import type { Ctx } from "@snapfire/fsr/host";

export async function load({ query, path }: Ctx) {
  return { view: query.view ?? "inbox", path };
}
