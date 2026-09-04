import Catalog from "@routes/index/page";
import { assert, render, screen, test } from "@snapfire/fsr-client/testing";

const product = (id: bigint, name: string, category: string) => ({ id, name, brand: "Prusa", category, price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "", tags: [], attributes: [] });

test("the catalog hydrates with its chips and cards", async () => {
  const products = [product(1n, "PLA filament", "printing"), product(2n, "Nozzle", "printing")];
  const r = await render(<Catalog products={products} q="" category="printing" />);
  assert.equal(r.hydrated, "routes/index/page.tsx#default");
  assert.equal(screen.getByText("2 results").tagName, "P");
  assert.equal(screen.getAllByText(/filament|Nozzle/).length, 2);
  assert.ok(screen.getByText("3D printing", r.container.querySelector("nav.chips")!).className.includes("chip-active"));
});

test("a search with nothing matching says so", async () => {
  await render(<Catalog products={[]} q="zzz" category="" />);
  assert.ok(screen.getByText('Results for "zzz"'));
  assert.ok(screen.getByText("Nothing matched"));
});
