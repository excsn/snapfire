import Catalog from "@routes/page";
import { expect, render, screen, test } from "@snapfire/fsr-client/testing";

const product = (id: bigint, name: string, category: string) => ({ id, name, brand: "Prusa", category, price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "", tags: [], attributes: [] });

test("the catalog renders its chips and cards and hydrates since each card's add button runs in the browser", async () => {
  const products = [product(1n, "PLA filament", "printing"), product(2n, "Nozzle", "printing")];
  const r = await render(<Catalog products={products} q="" category="printing" />);
  expect(r.hydrated, "each card's add button has a click handler, so the page mounts over the server's markup").toEqual("routes/page.tsx#default");
  expect(screen.getByText("2 results").tagName).toEqual("P");
  expect(screen.getAllByText(/filament|Nozzle/).length).toEqual(2);
  expect(screen.getByText("3D printing", r.container.querySelector("nav.chips")!).className.includes("chip-active")).toBeTruthy();
});

test("a search with nothing matching says so", async () => {
  await render(<Catalog products={[]} q="zzz" category="" />);
  expect(screen.getByText('Results for "zzz"')).toBeTruthy();
  expect(screen.getByText("Nothing matched")).toBeTruthy();
});
