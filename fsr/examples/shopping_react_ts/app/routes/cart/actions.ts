import { action, fail } from "@snapfire/fsr";
import type { AddToCart, RemoveFromCart } from "../../schemas/cart";

export const addToCart = action<AddToCart>(async ({ input, session }) => {
  const key = String(input.product_id);
  const wanted = (session.cart[key] ?? 0n) + input.quantity;
  if (wanted <= 0n) delete session.cart[key];
  else session.cart = { ...session.cart, [key]: wanted };
  const count = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines: session.cart, count };
});

export const removeFromCart = action<RemoveFromCart>(async ({ input, session }) => {
  const key = String(input.product_id);
  delete session.cart[key];
  const count = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines: session.cart, count };
});

export const checkout = action(async ({ session, services }) => {
  const lines = Object.entries(session.cart).map(([id, quantity]) => ({ product_id: BigInt(id), quantity }));
  if (lines.length === 0) fail("invalid", "the cart is empty");
  const order = await services.shopping.placeOrder({ lines });
  session.cart = {};
  return order;
});
