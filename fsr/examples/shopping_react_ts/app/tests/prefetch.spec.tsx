import { advance, assert, ctx, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

function hover(el: Element): Promise<void> {
  el.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
  return settle();
}

test("hovering a link fetches its payload and the click that follows makes no request", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const cart = screen.getByLabelText("Cart, 2 items");

  await hover(cart);
  assert.equal(c.trace.calls.map((call) => call.method), ["listProducts", "listProducts"], "the cart's loader ran on hover");

  await hover(cart);
  assert.equal(c.trace.calls.length, 2, "a payload already held is not fetched again");

  await fireEvent.click(cart);
  assert.equal(location.pathname, "/cart");
  assert.ok(screen.getByText("Shopping cart"));
  assert.equal(c.trace.calls.length, 2, "the click applied the held payload");
});

test("a held payload expires and the next navigation fetches again", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const cart = screen.getByLabelText("Cart, 2 items");
  await hover(cart);
  assert.equal(c.trace.calls.length, 2);

  await advance(30_000);
  await fireEvent.click(cart);
  assert.equal(location.pathname, "/cart");
  assert.equal(c.trace.calls.length, 3, "thirty seconds later the payload is fetched again");
});

test("a link marked data-sf-prefetch=none is left alone until it is clicked", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const cart = screen.getByLabelText("Cart, 2 items");
  cart.setAttribute("data-sf-prefetch", "none");

  await hover(cart);
  assert.equal(c.trace.calls.length, 1, "no fetch on hover");

  await fireEvent.click(cart);
  assert.equal(location.pathname, "/cart");
  assert.equal(c.trace.calls.length, 2, "the click fetched it");
});
