import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

test("a click from the catalog to the cart swaps the page and keeps the document", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const app = document.getElementById("app");
  assert.ok(app, "the shell mounted the page under #app");
  assert.ok(screen.getByText("Today's picks"));
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/index/page.tsx#default"][data-sf-mounted]'), "the catalog hydrated");

  await fireEvent.click(screen.getByLabelText("Cart, 2 items"));

  assert.equal(location.pathname, "/cart");
  assert.ok(document.getElementById("app") === app, "the shell's DOM survived the navigation");
  assert.equal(screen.queryByText("Today's picks"), null);
  assert.ok(screen.getByText("Shopping cart"));
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/cart/page.tsx#default"][data-sf-mounted]'), "the cart hydrated in place");
  assert.equal(
    c.trace.calls.map((call) => call.method),
    ["listProducts", "listProducts"],
    "each page's loader ran once, through the mocks",
  );
});

test("a route nothing matches falls back to a full load", async () => {
  const c = ctx({ services: { shopping: { listProducts: () => [] } } });
  await load("/", { ctx: c });
  document.body.insertAdjacentHTML("beforeend", '<a id="nowhere" href="/nowhere">x</a>');
  await fireEvent.click(document.getElementById("nowhere")!);
  assert.equal(location.pathname, "/nowhere", "location.assign took over");
});
