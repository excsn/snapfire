import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };

test("a quick look opened from a search keeps the layout on the search", async () => {
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/?q=PLA", { ctx: c });
  const box = document.querySelector<HTMLInputElement>('input[name="q"]')!;
  expect(box.value).toEqual("PLA");
  const island = document.querySelector('sf-i[data-sf-module="routes/layout.tsx#default"]')!;
  const before = document.querySelector(`script[data-sf-props="${island.id}"]`)?.textContent ?? "";
  expect(before.includes('"PLA"'), before).toBeTruthy();

  await fireEvent.click(screen.getByText("PLA filament"));

  expect(location.pathname).toEqual("/product/1");
  expect(document.querySelector('sf-s[data-sf-name="modal"]')?.textContent?.includes("Full details"), "the quick look opened").toBeTruthy();
  const after = document.querySelector(`script[data-sf-props="${island.id}"]`)?.textContent ?? "";
  expect(after.includes('"PLA"'), `the kept layout's props still carry the search: ${after}`).toBeTruthy();
  expect(document.querySelector<HTMLInputElement>('input[name="q"]')?.value, "the search box still holds the term").toEqual("PLA");
});
