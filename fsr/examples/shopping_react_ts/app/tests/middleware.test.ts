import { middleware } from "../middleware";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("the old basket path redirects to the cart", async () => {
  const c = ctx({ request: { method: "GET", path: "/basket" } });
  await middleware(c);
  expect(c.trace.redirect).toEqual("/cart");
  expect(c.trace.next).toEqual([]);
});

test("the shop path is the catalog under another name", async () => {
  const c = ctx({ request: { method: "GET", path: "/shop" } });
  await middleware(c);
  expect(c.trace.next).toEqual(["/"]);
});

test("every other request goes on to its route and carries the storefront header", async () => {
  const c = ctx({ request: { method: "POST", path: "/_sf/action/cart.checkout" } });
  const response = await middleware(c);
  expect(c.trace.next).toEqual(["/_sf/action/cart.checkout"]);
  expect(response.headers.get("x-storefront")).toEqual("fsr");
});
