import { middleware } from "../middleware";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("the old basket path redirects to the cart", async () => {
  const c = ctx({ request: { method: "GET", path: "/basket" } });
  const result = await middleware(c);
  assert.equal(result, { redirect: "/cart" });
});

test("the shop path is the catalog under another name", async () => {
  const c = ctx({ request: { method: "GET", path: "/shop" } });
  const result = await middleware(c);
  assert.equal(result, { rewrite: "/" });
});

test("every other request carries the storefront header", async () => {
  const c = ctx({ request: { method: "POST", path: "/_sf/action/cart.checkout" } });
  const result = await middleware(c);
  assert.equal(result, { headers: { "x-storefront": "fsr" } });
});
