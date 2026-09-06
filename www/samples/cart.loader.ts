import type { Ctx } from "@snapfire/fsr";

export async function load({ session }: Ctx) {
  const count = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { count };
}
