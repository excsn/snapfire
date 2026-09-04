import ProductPage from "@routes/product/[id]/page";
import { advance, assert, ctx, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

const product = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [{ name: "Ingredients", value: "PLA" }, { name: "Weight", value: "1 kg" }] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1", "B2"] };

test("the product page hydrates with its quantity select", async () => {
  const r = await render(<ProductPage product={product} stock={stock} inCart={0n} />);
  assert.equal(r.hydrated, "routes/product/[id]/page.tsx#default");
  const select = screen.getByLabelText("Quantity") as HTMLSelectElement;
  assert.equal(select.querySelectorAll("option").length, 8);
  assert.equal(select.value, "1");
  assert.equal(screen.getByText("In stock").className, "stock stock-in");
});

test("choosing a quantity and adding runs the action with it", async () => {
  const c = ctx({ session: { cart: {} } });
  await render(<ProductPage product={product} stock={stock} inCart={0n} />, { ctx: c });
  const select = screen.getByLabelText("Quantity") as HTMLSelectElement;
  await fireEvent.change(select, "3");
  assert.equal(select.value, "3");
  await fireEvent.click(screen.getByText("Add to cart"));
  assert.equal(c.session.cart, { "1": 3 });
  assert.ok(screen.getByText("Added to your cart"), "the toast is up until its timer runs");
  await advance(5000);
  assert.equal(screen.queryByText("Added to your cart"), null);
});

test("an action that fails is reported to the page, not the test", async () => {
  const c = ctx({ session: { cart: {} } });
  const sold = { ...product, stock: 0n };
  await render(<ProductPage product={sold} stock={{ ...stock, on_hand: 0n }} inCart={0n} />, { ctx: c });
  assert.equal((screen.getByText("Add to cart") as HTMLButtonElement).disabled, true);
  assert.equal(screen.queryByText("Quantity"), null);
  assert.throws(() => screen.getByLabelText("Quantity"), "no element");
});
