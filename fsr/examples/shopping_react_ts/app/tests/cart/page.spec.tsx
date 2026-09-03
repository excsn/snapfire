import Cart from "@routes/cart/page";
import { assert, ctx, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: null, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "", tags: [], attributes: [], quantity: 2n };

test("the server renders the cart and React hydrates over it", async () => {
  const r = await render(<Cart lines={[filament]} cartCount={2n} />);
  assert.equal(r.hydrated, "routes/cart/page.tsx#default");
  assert.equal(screen.getByText("PLA filament").tagName, "A");
  assert.equal(screen.getAllByText("$48.00").length, 3, "the line, the subtotal and the buy box");
});

test("an empty cart says so", async () => {
  await render(<Cart lines={[]} cartCount={0n} />);
  assert.ok(screen.getByText("Your cart is empty"));
  assert.equal(screen.queryByText("Proceed to checkout"), null);
});

test("adding one runs the action against the session", async () => {
  const c = ctx({ session: { cart: { "1": 2n } } });
  await render(<Cart lines={[filament]} cartCount={2n} />, { ctx: c });
  await fireEvent.click(screen.getByLabelText("Add one"));
  assert.equal(c.session.cart, { "1": 3 });
  assert.equal(c.trace.calls, []);
});

test("checkout places the order through the mocked service", async () => {
  const c = ctx({
    session: { cart: { "1": 2n } },
    services: { shopping: { placeOrder: (args: { lines: { product_id: bigint; quantity: bigint }[] }) => ({ id: 7n, total_cents: 4800n, lines: args.lines.map((l) => ({ ...l, name: "PLA filament", line_cents: 4800n })) }) } },
  });
  await render(<Cart lines={[filament]} cartCount={2n} />, { ctx: c });
  await fireEvent.click(screen.getByText("Proceed to checkout"));
  await fireEvent.click(screen.getByText("Place order"));
  assert.equal(c.trace.calls, [{ service: "shopping", method: "placeOrder", args: { lines: [{ product_id: 1, quantity: 2 }] } }]);
  assert.equal(c.session.cart, {});
});
