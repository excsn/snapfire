import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx<"/cart">) {
  const catalog = await services.shopping.listProducts({});
  const lines = catalog
    .filter((p) => session.cart[String(p.id)])
    .map((p) => ({ ...p, quantity: session.cart[String(p.id)] }));
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines, cartCount };
}
