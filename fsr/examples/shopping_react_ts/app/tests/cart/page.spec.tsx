import Cart from "@routes/cart/page";
import { advance, assert, ctx, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: null, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "", tags: [], attributes: [], quantity: 2n };

test("the server renders the cart and React hydrates over it", async () => {
  const r = await render(<Cart lines={[filament]} />);
  assert.equal(r.hydrated, "routes/cart/page.tsx#default");
  assert.equal(screen.getByText("PLA filament").tagName, "A");
  assert.equal(screen.getAllByText("$48.00").length, 3, "the line, the subtotal and the buy box");
});

test("an empty cart says so", async () => {
  await render(<Cart lines={[]} />);
  assert.ok(screen.getByText("Your cart is empty"));
  assert.equal(screen.queryByText("Proceed to checkout"), null);
});

test("adding one runs the action against the session", async () => {
  const c = ctx({ session: { cart: { "1": 2n } } });
  await render(<Cart lines={[filament]} />, { ctx: c });
  await fireEvent.click(screen.getByLabelText("Add one"));
  assert.equal(c.session.cart, { "1": 3 });
  assert.equal(c.trace.calls, []);
});

test("checkout places the order through the mocked service", async () => {
  const c = ctx({
    session: { cart: { "1": 2n } },
    services: {
      shopping: {
        placeOrder: (args: { lines: { product_id: bigint; quantity: bigint }[] }) => ({ id: 7n, total_cents: 4800n, lines: args.lines.map((l) => ({ ...l, name: "PLA filament", line_cents: 4800n })) }),
        getOrder: ({ id }: { id: bigint }) => ({ id, total_cents: 4800n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2, line_cents: 4800n }] }),
        listProducts: () => [],
      },
    },
  });
  await render(<Cart lines={[filament]} />, { ctx: c });
  await fireEvent.click(screen.getByText("Proceed to checkout"));
  await fireEvent.click(screen.getByText("Place order"));
  assert.equal(c.trace.calls.map((call) => call.method), ["placeOrder"]);
  assert.equal(c.trace.calls[0].args, { lines: [{ product_id: 1, quantity: 2 }] });
  assert.equal(c.session.cart, {});
  assert.ok(screen.getByText("Order #7 placed"), "the toast is the intermission");
  assert.ok(screen.getByText("Order processing"), "the cart shows the order going through");
  assert.equal(screen.queryByText("Your cart is empty"), null, "never the empty cart the checkout just made");
  assert.equal(location.pathname, "/", "nothing moves while it shows");
  await advance(2000);
  assert.equal(location.pathname, "/order/7", "then the page goes to the order");
  assert.equal(c.trace.calls.map((call) => call.method), ["placeOrder", "getOrder", "listProducts"], "the order page's loader and the layout's promo slot's");
});
