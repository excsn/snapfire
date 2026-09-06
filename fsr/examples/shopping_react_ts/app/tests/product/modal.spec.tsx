import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("a click on a product from the catalog opens it in the layout's modal slot and keeps the catalog", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });
  const catalog = document.querySelector('sf-i[data-sf-module="routes/page.tsx#default"]');
  const modal = document.querySelector('sf-s[data-sf-name="modal"]');
  assert.ok(catalog && modal);
  assert.equal(modal!.childNodes.length, 0, "the modal slot is empty on a document load");

  await fireEvent.click(screen.getByText("PLA filament"));

  assert.equal(location.pathname, "/product/1");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/page.tsx#default"]') === catalog, "the catalog kept its DOM");
  assert.ok(screen.getByText("Today's picks"), "and its content");
  assert.ok(modal!.querySelector('sf-i[data-sf-module="routes/product/[id]/page.modal.tsx#default"][data-sf-mounted]'), "the quick look hydrated inside the modal slot");
  assert.equal(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"]'), null, "the page itself was never rendered");
  assert.equal(c.trace.calls.map((call) => call.method), ["listProducts", "listProducts", "getProduct", "getStock"], "the catalog and promo loaders ran for the document, the product's for the modal");

  await fireEvent.click(screen.getByText("Full details"));

  assert.equal(location.pathname, "/product/1");
  assert.equal(modal!.innerHTML, "", "the modal slot emptied");
  assert.equal(screen.queryByText("Today's picks"), null, "the catalog gave way to the page");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"][data-sf-mounted]'), "the full page hydrated in the content slot");
});

test("a document load of the product is the full page and the modal slot stays empty", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/product/1", { ctx: c });
  const sidecar = document.querySelector("script[data-sf-segments]")!.textContent!;
  assert.ok(sidecar.includes('"k":"routes/product/[id]/page.tsx#default?id=1","n":"content"'), sidecar);
  assert.ok(!sidecar.includes("page.modal") && !sidecar.includes("keep"), sidecar);
  assert.equal(document.querySelector('sf-s[data-sf-name="modal"]')!.innerHTML, "");
});
