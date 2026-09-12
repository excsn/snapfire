import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const stalls = await services.kitchen.listMarket();
  return { stalls };
}
