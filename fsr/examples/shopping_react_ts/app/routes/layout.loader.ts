import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session }: Ctx) {
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { cartCount, q: query.q, category: query.category };
}

export const store = ({ data }: { data: { cartCount: bigint } }) => ({ "cart/count": Number(data.cartCount) });
