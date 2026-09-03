import { addToCart, checkout, removeFromCart } from "@routes/cart/actions";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("adding twice accumulates and reports the count", async () => {
  const c = ctx<{ product_id: bigint; quantity: bigint }>({ session: { cart: {} }, input: { product_id: 1n, quantity: 2n } });
  await addToCart(c);
  const result = await addToCart(c);
  assert.equal(c.session.cart, { "1": 4n });
  assert.equal(result.count, 4n);
  assert.equal(c.trace.session.written, ["cart"]);
});

test("a negative quantity that empties a line deletes it", async () => {
  const c = ctx<{ product_id: bigint; quantity: bigint }>({ session: { cart: { "1": 1n, "2": 5n } }, input: { product_id: 1n, quantity: -1n } });
  const result = await addToCart(c);
  assert.equal(result.lines, { "2": 5n });
});

test("removing a line the cart does not hold is harmless", async () => {
  const c = ctx<{ product_id: bigint }>({ session: { cart: { "2": 5n } }, input: { product_id: 9n } });
  const result = await removeFromCart(c);
  assert.equal(result.count, 5n);
});

test("checkout refuses an empty cart before any call", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { placeOrder: () => ({ id: 1n, total_cents: 0n, lines: [] }) } } });
  await assert.rejects(checkout(c), "invalid");
  assert.equal(c.trace.calls, []);
});

test("checkout places the held lines and empties the cart", async () => {
  const c = ctx({
    session: { cart: { "1": 2n } },
    services: { shopping: { placeOrder: (args) => ({ id: 7n, total_cents: 4800n, lines: args.lines.map((l) => ({ ...l, name: "PLA filament", line_cents: 4800n })) }) } },
  });
  const order = await checkout(c);
  assert.equal(order.id, 7n);
  assert.equal(c.session.cart, {});
  assert.equal(c.trace.calls, [{ service: "shopping", method: "placeOrder", args: { lines: [{ product_id: 1n, quantity: 2n }] } }]);
});
