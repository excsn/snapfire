import { get, set } from "@snapfire/fsr-client";
import { advance, ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

import { cartCount } from "@src/store";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("the layout's seed reaches the header, and a write from another root re-renders it", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });

  expect(get(cartCount), "the document seeded the store").toEqual(2);
  expect(screen.getByLabelText("Cart, 2 items"), "and the header rendered from it").toBeTruthy();

  set(cartCount, 9);
  await settle();
  expect(screen.getByLabelText("Cart, 9 items"), "a write outside the layout's root re-rendered the header").toBeTruthy();
  await settle();
});

test("an optimistic add shows in the header, and the revalidation replaces it with what the session holds", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });

  expect(get(cartCount)).toEqual(0);
  const link = screen.getByText("PLA filament");
  link.setAttribute("data-sf-full", "");
  await fireEvent.click(link);
  await fireEvent.click(screen.getByText("Add to cart"));
  await advance(2000);
  expect(c.session.cart, "the action ran").toEqual({ "1": 1 });
  expect(get(cartCount), "the seed the revalidation carried is what the header shows").toEqual(1);
  expect(screen.getByLabelText("Cart, 1 items")).toBeTruthy();
  await settle();
});
