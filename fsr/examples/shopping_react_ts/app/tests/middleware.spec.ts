import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

test("loading the old basket path follows the redirect to the cart", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  const { status, path } = await load("/basket", { ctx: c });
  expect(status).toEqual(200);
  expect(path).toEqual("/cart");
  expect(screen.getByText("Shopping cart")).toBeTruthy();
});

test("the shop path is rewritten to the catalog and every other response carries the header", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });

  const shop = await fetch("/shop");
  expect(shop.status).toEqual(200);
  expect((await shop.text()).includes("Today's picks"), "the catalog under another name").toBeTruthy();
  expect(shop.headers.get("x-storefront"), "a rewrite returns before the header is set").toBeNull();

  const cart = await fetch("/api/cart");
  expect(cart.status).toEqual(200);
  expect(cart.headers.get("x-storefront")).toEqual("fsr");

  const redirected = await fetch("/basket");
  expect(redirected.status).toEqual(307);
  expect(redirected.headers.get("location")).toEqual("/cart");
});
