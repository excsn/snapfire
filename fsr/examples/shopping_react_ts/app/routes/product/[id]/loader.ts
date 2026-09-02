import type { Ctx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/product/{id}">) {
  const product = await services.shopping.getProduct({ id: BigInt(params.id) });
  const stock = await services.inventory.getStock({ product_id: BigInt(params.id) });
  const inCart = session.cart[params.id] ?? 0n;
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { product, stock, inCart, cartCount };
}
