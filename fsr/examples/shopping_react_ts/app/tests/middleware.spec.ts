import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

test("loading the old basket path follows the redirect to the cart", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  const { status, path } = await load("/basket", { ctx: c });
  assert.equal(status, 200);
  assert.equal(path, "/cart");
  assert.ok(screen.getByText("Shopping cart"));
});

test("the shop path is rewritten to the catalog and every other response carries the header", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });

  const shop = await fetch("/shop");
  assert.equal(shop.status, 200);
  assert.ok((await shop.text()).includes("Today's picks"), "the catalog under another name");
  assert.equal(shop.headers.get("x-storefront"), null, "a rewrite returns before the header is set");

  const cart = await fetch("/api/cart");
  assert.equal(cart.status, 200);
  assert.equal(cart.headers.get("x-storefront"), "fsr");

  const redirected = await fetch("/basket");
  assert.equal(redirected.status, 307);
  assert.equal(redirected.headers.get("location"), "/cart");
});
