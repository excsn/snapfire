import { GET, POST } from "@routes/api/cart/route";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("GET reports the cart the session holds", async () => {
  const c = ctx({ session: { cart: { "1": 2n, "4": 1n } } });
  const result = await GET(c);
  expect(result.count).toEqual(3n);
  expect(c.trace.calls).toEqual([]);
});

test("POST adds to a line and writes the session", async () => {
  const c = ctx<{ product_id: bigint; quantity: bigint }>({ session: { cart: { "1": 2n } }, input: { product_id: 1n, quantity: 3n } });
  const result = await POST(c);
  expect(result.count).toEqual(5n);
  expect(c.session.cart).toEqual({ "1": 5n });
  expect(c.trace.session.written).toEqual(["cart"]);
});
