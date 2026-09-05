import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 5n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("a page titles the document from its loader's data", async () => {
  const c = ctx({ services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  assert.equal(document.title, "Today's picks · Shopping");

  await load("/?q=pla", { ctx: c });
  assert.equal(document.title, "Results for pla · Shopping");
});

test("a navigation retitles the document, a streamed page once it resolves", async () => {
  const c = ctx({ services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });
  assert.equal(document.title, "Today's picks · Shopping");

  await fireEvent.click(screen.getByText("PLA filament"));
  assert.equal(location.pathname, "/product/1");
  assert.equal(document.title, "PLA filament · Shopping");
  assert.equal(document.head.querySelector('meta[name="description"]')?.getAttribute("content"), "PLA filament for $24.00");
});
