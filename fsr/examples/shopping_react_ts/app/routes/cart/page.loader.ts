import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx<"/cart">) {
  const catalog = await services.shopping.listProducts({});
  const lines = catalog
    .filter((p) => session.cart[String(p.id)])
    .map((p) => ({ ...p, quantity: session.cart[String(p.id)] }));
  return { lines };
}
