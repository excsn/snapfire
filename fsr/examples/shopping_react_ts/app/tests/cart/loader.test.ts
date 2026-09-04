import { load } from "@routes/cart/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Polymaker", category: "printing", price_cents: 2400n, stock: 12n, rating: 4.7, reviews: 1834n, description: "", tags: [], attributes: [], image: { color: "#000", emoji: "x" } };
const hotend = { ...filament, id: 2n, name: "Hotend" };

test("held lines carry the catalog's rows and the held quantity", async () => {
  const c = ctx<void, "/cart">({
    session: { cart: { "2": 3n } },
    services: { shopping: { listProducts: () => [filament, hotend] } },
  });
  const { lines, cartCount } = await load(c);
  assert.equal(lines, [{ ...hotend, quantity: 3n }]);
  assert.equal(cartCount, 3n);
  assert.equal(c.trace.calls, [{ service: "shopping", method: "listProducts", args: {} }]);
});

test("an empty cart lists nothing and still asks the catalog once", async () => {
  const c = ctx<void, "/cart">({ services: { shopping: { listProducts: () => [filament] } } });
  const { lines, cartCount } = await load(c);
  assert.equal(lines, []);
  assert.equal(cartCount, 0n);
  assert.ok(c.trace.calls.length === 1);
});
