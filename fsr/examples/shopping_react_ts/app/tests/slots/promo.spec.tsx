import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const crackers = { id: 8n, name: "Sea salt crackers", brand: "Peter's Yard", category: "food", price_cents: 395n, list_price_cents: null, image: { color: "#c9a66b", emoji: "🥟" }, rating: 4.4, reviews: 688n, stock: 3n, description: "Thin.", tags: ["food", "snack"], attributes: [] };

test("the promo slot renders from its own loader beside the page and survives a navigation", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: ({ tag }: { tag?: string }) => (tag === "snack" ? [crackers] : [filament]) } } });
  await load("/", { ctx: c });
  const promo = document.querySelector('sf-s[data-sf-name="promo"]');
  expect(promo?.textContent?.includes("Snacks at the counter"), "the promo rendered from its own loader inside the layout's slot").toBeTruthy();
  expect(promo!.querySelector("sf-i"), "as markup, since it has no state").toBeNull();
  const heading = screen.getByText("Snacks at the counter");
  expect(screen.getByText("Sea salt crackers")).toBeTruthy();
  expect(screen.queryByText("Sea salt crackers", document.querySelector("main.catalog")!), "the catalog shows the catalog's answer, not the promo's").toBeNull();

  await fireEvent.click(screen.getByLabelText("Cart, 2 items"));

  expect(location.pathname).toEqual("/cart");
  expect(screen.getByText("Snacks at the counter"), "the promo kept its DOM across the navigation").toBe(heading);
  expect(screen.getByText("Shopping cart")).toBeTruthy();
});
