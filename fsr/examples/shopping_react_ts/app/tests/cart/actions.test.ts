import { addToCart, checkout, removeFromCart } from "@routes/cart/actions";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("adding twice accumulates and reports the count", async () => {
  const c = ctx<{ product_id: bigint; quantity: bigint }>({ session: { cart: {} }, input: { product_id: 1n, quantity: 2n } });
  await addToCart(c);
  const result = await addToCart(c);
  expect(c.session.cart).toEqual({ "1": 4n });
  expect(result.count).toEqual(4n);
  expect(c.trace.session.written).toEqual(["cart"]);
});

test("a negative quantity that empties a line deletes it", async () => {
  const c = ctx<{ product_id: bigint; quantity: bigint }>({ session: { cart: { "1": 1n, "2": 5n } }, input: { product_id: 1n, quantity: -1n } });
  const result = await addToCart(c);
  expect(result.lines).toEqual({ "2": 5n });
});

test("removing a line the cart does not hold is harmless", async () => {
  const c = ctx<{ product_id: bigint }>({ session: { cart: { "2": 5n } }, input: { product_id: 9n } });
  const result = await removeFromCart(c);
  expect(result.count).toEqual(5n);
});

test("checkout refuses an empty cart before any call", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { placeOrder: () => ({ id: 1n, total_cents: 0n, lines: [] }) } } });
  await expect(checkout(c)).rejects.toMatchObject({ kind: "invalid" });
  expect(c.trace.calls).toEqual([]);
});

test("checkout places the held lines and empties the cart", async () => {
  const c = ctx({
    session: { cart: { "1": 2n } },
    services: { shopping: { placeOrder: (args) => ({ id: 7n, total_cents: 4800n, lines: args.lines.map((l) => ({ ...l, name: "PLA filament", line_cents: 4800n })) }) } },
  });
  const order = await checkout(c);
  expect(order.id).toEqual(7n);
  expect(c.session.cart).toEqual({});
  expect(c.trace.calls).toEqual([{ service: "shopping", method: "placeOrder", args: { lines: [{ product_id: 1n, quantity: 2n }] } }]);
});
