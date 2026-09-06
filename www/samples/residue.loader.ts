import type { Ctx } from "@snapfire/fsr";

export async function load({ session }: Ctx) {
  while (session.retries > 0) {
    retry();
  }
  return { status: "ok" };
}
