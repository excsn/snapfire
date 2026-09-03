import type { Ctx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/order/{id}">) {
  const order = await services.shopping.getOrder({ id: BigInt(params.id) });
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { order, cartCount };
}
