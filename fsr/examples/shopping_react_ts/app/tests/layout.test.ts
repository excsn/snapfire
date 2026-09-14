import { load } from "../routes/layout.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("the layout counts the cart and passes the search through", async () => {
  const c = ctx({ session: { cart: { "1": 2n, "4": 1n } }, query: { q: "pla" } });
  const result = await load(c);
  expect(result.cartCount).toEqual(3n);
  expect(result.q).toEqual("pla");
  expect(c.trace.calls).toEqual([]);
});
