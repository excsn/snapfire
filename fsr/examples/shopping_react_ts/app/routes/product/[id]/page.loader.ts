import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/product/{id}">) {
  const product = await services.shopping.getProduct({ id: BigInt(params.id) });
  const stock = await services.inventory.getStock({ product_id: BigInt(params.id) });
  const inCart = session.cart[params.id] ?? 0n;
  return { product, stock, inCart };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({
  title: `${data.product.name} · Shopping`,
  description: `${data.product.name} for $${(Number(data.product.price_cents) / 100).toFixed(2)}`,
});
