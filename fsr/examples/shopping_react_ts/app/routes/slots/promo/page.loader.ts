import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const snacks = await services.shopping.listProducts({ tag: "snack" });
  return { snacks };
}
