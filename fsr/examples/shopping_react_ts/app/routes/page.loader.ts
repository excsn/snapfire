import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";

export async function load({ query, services }: Ctx<"/">) {
  const products = await services.shopping.listProducts({ q: query.q, category: query.category, tag: query.tag });
  return { products, q: query.q, category: query.category };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({
  title: data.q ? `Results for ${data.q} · Shopping` : "Today's picks · Shopping",
});
