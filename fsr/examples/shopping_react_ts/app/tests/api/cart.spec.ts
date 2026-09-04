import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

test("the cart handler answers GET and POST with JSON under the spec's session", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [] } } });
  await load("/", { ctx: c });

  const got = await fetch("/api/cart");
  assert.equal(got.status, 200);
  assert.equal(await got.json(), { lines: { "1": 2 }, count: 2 });

  const posted = await fetch("/api/cart", { method: "POST", body: JSON.stringify({ product_id: 3, quantity: 1 }) });
  assert.equal(posted.status, 200);
  assert.equal((await posted.json()).count, 3);
  assert.equal(c.session.cart, { "1": 2, "3": 1 });

  const refused = await fetch("/api/cart", { method: "POST", body: JSON.stringify({ product_id: "three" }) });
  assert.equal(refused.status, 400);

  const missing = await fetch("/api/nothing");
  assert.equal(missing.status, 404);
});
