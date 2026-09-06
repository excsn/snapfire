import { advance, assert, ctx, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("the layout hydrates over the page, keeps its state across a navigation and takes new props after an action", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });

  const layout = document.querySelector('sf-i[data-sf-module="routes/layout.tsx#default"][data-sf-mounted]');
  assert.ok(layout, "the layout is an island and hydrated");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/page.tsx#default"][data-sf-mounted]'), "the page hydrated inside it");
  assert.ok(layout!.querySelector("sf-s sf-i"), "the page sits in the layout's slot");
  const header = document.querySelector("header.site-header");
  assert.ok(header);

  const input = screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement;
  await fireEvent.change(input, "nozzle");
  assert.equal(input.value, "nozzle");

  const link = screen.getByText("PLA filament");
  link.setAttribute("data-sf-full", "");
  await fireEvent.click(link);
  assert.equal(location.pathname, "/product/1");
  assert.ok(document.querySelector("header.site-header") === header, "the layout's DOM survived the navigation");
  assert.equal((screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement).value, "nozzle", "and so did its state");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"][data-sf-mounted]'), "the new page hydrated in its own root");
  assert.equal(screen.getByLabelText("Cart, 0 items").textContent?.includes("0"), true);

  await fireEvent.click(screen.getByText("Add to cart"));
  await advance(2000);
  assert.equal(c.session.cart, { "1": 1 }, "the action ran once");
  assert.ok(document.querySelector("header.site-header") === header, "revalidation kept the layout's DOM");
  assert.ok(screen.getByLabelText("Cart, 1 items"), "and re-rendered it with the new count");
  assert.equal((screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement).value, "nozzle", "without losing its state");
  await settle();
});
