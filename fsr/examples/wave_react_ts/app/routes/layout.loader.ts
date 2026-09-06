import type { Ctx } from "@snapfire/fsr";

export async function load({ session }: Ctx) {
  return { name: session.name };
}
