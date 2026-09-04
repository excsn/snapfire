import { GET, POST } from "@routes/api/cart/route";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("GET reports the cart the session holds", async () => {
  const c = ctx({ session: { cart: { "1": 2n, "4": 1n } } });
  const result = await GET(c);
  assert.equal(result.count, 3n);
  assert.equal(c.trace.calls, []);
});

test("POST adds to a line and writes the session", async () => {
  const c = ctx<{ product_id: bigint; quantity: bigint }>({ session: { cart: { "1": 2n } }, input: { product_id: 1n, quantity: 3n } });
  const result = await POST(c);
  assert.equal(result.count, 5n);
  assert.equal(c.session.cart, { "1": 5n });
  assert.equal(c.trace.session.written, ["cart"]);
});
