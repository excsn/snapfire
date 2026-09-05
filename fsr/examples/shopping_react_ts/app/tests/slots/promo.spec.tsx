import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const crackers = { id: 8n, name: "Sea salt crackers", brand: "Peter's Yard", category: "food", price_cents: 395n, list_price_cents: null, image: { color: "#c9a66b", emoji: "🥟" }, rating: 4.4, reviews: 688n, stock: 3n, description: "Thin.", tags: ["food", "snack"], attributes: [] };

test("the promo slot renders from its own loader beside the page and survives a navigation", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: ({ tag }: { tag?: string }) => (tag === "snack" ? [crackers] : [filament]) } } });
  await load("/", { ctx: c });
  const promo = document.querySelector('sf-s[data-sf-name="promo"] sf-i[data-sf-module="routes/slots/promo/page.tsx#default"][data-sf-mounted]');
  assert.ok(promo, "the promo hydrated in its own root inside the layout's slot");
  assert.ok(screen.getByText("Snacks at the counter"));
  assert.ok(screen.getByText("Sea salt crackers"));
  assert.equal(screen.queryByText("Sea salt crackers", document.querySelector("main.catalog")!), null, "the catalog shows the catalog's answer, not the promo's");

  await fireEvent.click(screen.getByLabelText("Cart, 2 items"));

  assert.equal(location.pathname, "/cart");
  assert.ok(document.querySelector('sf-s[data-sf-name="promo"] sf-i') === promo, "the promo kept its DOM across the navigation");
  assert.ok(screen.getByText("Shopping cart"));
});
