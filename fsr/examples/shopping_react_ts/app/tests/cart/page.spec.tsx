import Cart from "@routes/cart/page";
import { advance, ctx, expect, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: null, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "", tags: [], attributes: [], quantity: 2n };

test("the server renders the cart and React hydrates over it", async () => {
  const r = await render(<Cart lines={[filament]} />);
  expect(r.hydrated).toEqual("routes/cart/page.tsx#default");
  expect(screen.getByText("PLA filament").tagName).toEqual("A");
  expect(screen.getAllByText("$48.00").length, "the line, the subtotal and the buy box").toEqual(3);
});

test("an empty cart says so", async () => {
  await render(<Cart lines={[]} />);
  expect(screen.getByText("Your cart is empty")).toBeTruthy();
  expect(screen.queryByText("Proceed to checkout")).toBeNull();
});

test("adding one runs the action against the session", async () => {
  const c = ctx({ session: { cart: { "1": 2n } } });
  await render(<Cart lines={[filament]} />, { ctx: c });
  await fireEvent.click(screen.getByLabelText("Add one"));
  expect(c.session.cart).toEqual({ "1": 3 });
  expect(c.trace.calls).toEqual([]);
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
  expect(c.trace.calls.map((call) => call.method)).toEqual(["placeOrder"]);
  expect(c.trace.calls[0].args).toEqual({ lines: [{ product_id: 1, quantity: 2 }] });
  expect(c.session.cart).toEqual({});
  expect(screen.getByText("Order #7 placed"), "the toast is the intermission").toBeTruthy();
  expect(screen.getByText("Order processing"), "the cart shows the order going through").toBeTruthy();
  expect(screen.queryByText("Your cart is empty"), "never the empty cart the checkout just made").toBeNull();
  expect(location.pathname, "nothing moves while it shows").toEqual("/");
  await advance(2000);
  expect(location.pathname, "then the page goes to the order").toEqual("/order/7");
  expect(c.trace.calls.map((call) => call.method), "the order page's loader and the layout's promo slot's").toEqual(["placeOrder", "getOrder", "listProducts"]);
});
