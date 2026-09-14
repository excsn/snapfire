import { advance, ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("the layout hydrates over the page, keeps its state across a navigation and takes new props after an action", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });

  const layout = document.querySelector('sf-i[data-sf-module="routes/layout.tsx#default"][data-sf-mounted]');
  expect(layout, "the layout is an island and hydrated").toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/page.tsx#default"]'), "the catalog hydrates since its cards' add buttons run in the browser").toBeTruthy();
  expect(layout!.querySelector("sf-s:not([data-sf-name]):not([data-sf-island])")?.textContent?.includes("Today's picks"), "the page sits in the layout's slot").toBeTruthy();
  const header = document.querySelector("header.site-header");
  expect(header).toBeTruthy();

  const input = screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement;
  await fireEvent.change(input, "nozzle");
  expect(input.value).toEqual("nozzle");

  const link = screen.getByText("PLA filament");
  link.setAttribute("data-sf-full", "");
  await fireEvent.click(link);
  expect(location.pathname).toEqual("/product/1");
  expect(document.querySelector("header.site-header"), "the layout's DOM survived the navigation").toBe(header);
  expect((screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement).value, "and so did its state").toEqual("nozzle");
  expect(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"][data-sf-mounted]'), "the new page hydrated in its own root").toBeTruthy();
  expect(screen.getByLabelText("Cart, 0 items").textContent?.includes("0")).toEqual(true);

  await fireEvent.click(screen.getByText("Add to cart"));
  await advance(2000);
  expect(c.session.cart, "the action ran once").toEqual({ "1": 1 });
  expect(document.querySelector("header.site-header"), "revalidation kept the layout's DOM").toBe(header);
  expect(screen.getByLabelText("Cart, 1 items"), "and re-rendered it with the new count").toBeTruthy();
  expect((screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement).value, "without losing its state").toEqual("nozzle");
  await settle();
});
