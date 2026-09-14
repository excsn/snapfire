import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("a click on a product from the catalog opens it in the layout's modal slot and keeps the catalog", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });
  const catalog = screen.getByText("Today's picks");
  const modal = document.querySelector('sf-s[data-sf-name="modal"]');
  expect(catalog && modal).toBeTruthy();
  expect(modal!.childNodes.length, "the modal slot is empty on a document load").toEqual(0);

  await fireEvent.click(screen.getByText("PLA filament"));

  expect(location.pathname).toEqual("/product/1");
  expect(screen.getByText("Today's picks"), "the catalog kept its DOM").toBe(catalog);
  expect(screen.getByText("Today's picks"), "and its content").toBeTruthy();
  expect(modal!.textContent?.includes("Full details"), "the quick look rendered inside the modal slot").toBeTruthy();
  expect(modal!.querySelector("sf-i"), "the modal hydrates for its close and add buttons").toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"]'), "the page itself was never rendered").toBeNull();
  expect(c.trace.calls.map((call) => call.method), "the catalog and promo loaders ran for the document, the product's for the modal").toEqual(["listProducts", "listProducts", "getProduct", "getStock"]);

  await fireEvent.click(screen.getByText("Full details"));

  expect(location.pathname).toEqual("/product/1");
  expect(modal!.innerHTML, "the modal slot emptied").toEqual("");
  expect(screen.queryByText("Today's picks"), "the catalog gave way to the page").toBeNull();
  expect(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"][data-sf-mounted]'), "the full page hydrated in the content slot").toBeTruthy();
});

test("a document load of the product is the full page and the modal slot stays empty", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/product/1", { ctx: c });
  const sidecar = document.querySelector("script[data-sf-segments]")!.textContent!;
  expect(sidecar.includes('"k":"routes/product/[id]/page.tsx#default?id=1","n":"content"'), sidecar).toBeTruthy();
  expect(!sidecar.includes("page.modal") && !sidecar.includes("keep"), sidecar).toBeTruthy();
  expect(document.querySelector('sf-s[data-sf-name="modal"]')!.innerHTML).toEqual("");
});
