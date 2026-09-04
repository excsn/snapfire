import { load } from "../routes/layout.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("the layout counts the cart and passes the search through", async () => {
  const c = ctx({ session: { cart: { "1": 2n, "4": 1n } }, query: { q: "pla" } });
  const result = await load(c);
  assert.equal(result.cartCount, 3n);
  assert.equal(result.q, "pla");
  assert.equal(c.trace.calls, []);
});
