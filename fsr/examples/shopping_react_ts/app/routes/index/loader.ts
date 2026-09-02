import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session, services }: Ctx<"/">) {
  const products = await services.shopping.listProducts({ q: query.q, category: query.category, tag: query.tag });
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { products, q: query.q, category: query.category, cartCount };
}
