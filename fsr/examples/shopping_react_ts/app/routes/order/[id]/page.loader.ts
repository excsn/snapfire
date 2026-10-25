import type { Ctx } from "@snapfire/fsr/host";

export async function load({ params, services }: Ctx<"/order/{id}">) {
  const order = await services.shopping.getOrder({ id: BigInt(params.id) });
  return { order };
}
