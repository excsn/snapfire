import type { Ctx } from "@snapfire/fsr";

export async function load({ query, services }: Ctx<"/">) {
  const products = await services.shopping.listProducts({ q: query.q, category: query.category, tag: query.tag });
  return { products, q: query.q, category: query.category };
}
