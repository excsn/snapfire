import ProductPage from "@routes/product/[id]/page";
import { advance, ctx, expect, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

const product = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 8n, description: "A spool.", tags: ["pla"], attributes: [{ name: "Ingredients", value: "PLA" }, { name: "Weight", value: "1 kg" }] };
const stock = { product_id: 1n, on_hand: 8n, reserved: 0n, warehouse: "Prague", bins: ["A1", "B2"] };

test("the product page hydrates with its quantity select", async () => {
  const r = await render(<ProductPage product={product} stock={stock} inCart={0n} />);
  expect(r.hydrated).toEqual("routes/product/[id]/page.tsx#default");
  const select = screen.getByLabelText("Quantity") as HTMLSelectElement;
  expect(select.querySelectorAll("option").length).toEqual(8);
  expect(select.value).toEqual("1");
  expect(screen.getByText("In stock").className).toEqual("stock stock-in");
});

test("choosing a quantity and adding runs the action with it", async () => {
  const c = ctx({ session: { cart: {} } });
  await render(<ProductPage product={product} stock={stock} inCart={0n} />, { ctx: c });
  const select = screen.getByLabelText("Quantity") as HTMLSelectElement;
  await fireEvent.change(select, "3");
  expect(select.value).toEqual("3");
  await fireEvent.click(screen.getByText("Add to cart"));
  expect(c.session.cart).toEqual({ "1": 3 });
  expect(screen.getByText("Added to your cart"), "the toast is up until its timer runs").toBeTruthy();
  await advance(5000);
  expect(screen.queryByText("Added to your cart")).toBeNull();
});

test("an action that fails is reported to the page, not the test", async () => {
  const c = ctx({ session: { cart: {} } });
  const sold = { ...product, stock: 0n };
  await render(<ProductPage product={sold} stock={{ ...stock, on_hand: 0n }} inCart={0n} />, { ctx: c });
  expect((screen.getByText("Add to cart") as HTMLButtonElement).disabled).toEqual(true);
  expect(screen.queryByText("Quantity")).toBeNull();
  expect(() => screen.getByLabelText("Quantity")).toThrow("Unable to find an element");
});
