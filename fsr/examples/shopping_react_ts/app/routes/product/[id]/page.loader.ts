import type { Ctx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/product/{id}">) {
  const product = await services.shopping.getProduct({ id: BigInt(params.id) });
  const stock = await services.inventory.getStock({ product_id: BigInt(params.id) });
  const inCart = session.cart[params.id] ?? 0n;
  return { product, stock, inCart };
}
