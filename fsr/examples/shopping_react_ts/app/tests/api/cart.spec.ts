import { ctx, expect, load, test } from "@snapfire/fsr-client/testing";

test("the cart handler answers GET and POST with JSON under the spec's session", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [] } } });
  await load("/", { ctx: c });

  const got = await fetch("/api/cart");
  expect(got.status).toEqual(200);
  expect(await got.json()).toEqual({ lines: { "1": 2 }, count: 2 });

  const posted = await fetch("/api/cart", { method: "POST", body: JSON.stringify({ product_id: 3, quantity: 1 }) });
  expect(posted.status).toEqual(200);
  expect((await posted.json()).count).toEqual(3);
  expect(c.session.cart).toEqual({ "1": 2, "3": 1 });

  const refused = await fetch("/api/cart", { method: "POST", body: JSON.stringify({ product_id: "three" }) });
  expect(refused.status).toEqual(400);

  const missing = await fetch("/api/nothing");
  expect(missing.status).toEqual(404);
});
