import { action } from "@snapfire/fsr";
import type { ActionCtx, Ctx } from "@snapfire/fsr";
import type { AddToCart } from "@schemas/cart";

export async function GET({ session }: Ctx<"/api/cart">) {
  const count = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines: session.cart, count };
}

export const POST = action(async ({ input, session }: ActionCtx<AddToCart>) => {
  const key = String(input.product_id);
  const wanted = (session.cart[key] ?? 0n) + input.quantity;
  if (wanted <= 0n) delete session.cart[key];
  else session.cart = { ...session.cart, [key]: wanted };
  const count = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines: session.cart, count };
});
