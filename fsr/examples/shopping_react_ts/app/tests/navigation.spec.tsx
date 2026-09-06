import { assert, ctx, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

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
    ["listProducts", "listProducts", "listProducts", "listProducts"],
    "each page's loader and the promo slot's ran once per document, through the mocks",
  );
});

test("a click on a streamed route shows its fallback, then the fill when the resolution lands", async () => {
  const stock = { product_id: 1n, on_hand: 5n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });
  const whole = await (await fetch("/product/1?__payload")).text();
  const cut = whole.indexOf("\nS ");
  assert.ok(cut !== -1, "the product page streams behind its loading module");
  document.body.insertAdjacentHTML("beforeend", '<a id="view" href="/product/1" data-sf-full>view</a>');
  const encoder = new TextEncoder();
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const chunks = [async () => encoder.encode(whole.slice(0, cut + 1)), async () => gate.then(() => encoder.encode(whole.slice(cut + 1)))];
  const read = async () => {
    const next = chunks.shift();
    return next ? { done: false, value: await next() } : { done: true, value: undefined };
  };
  const real = globalThis.fetch;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    if (!String(input).includes("__payload")) return real(input, init);
    return Promise.resolve({ ok: true, status: 200, headers: { get: () => null }, body: { getReader: () => ({ read }) } } as unknown as Response);
  }) as typeof fetch;
  try {
    await fireEvent.click(document.getElementById("view")!);
    assert.equal(location.pathname, "/product/1", "history moved with the eager wave");
    assert.equal(document.querySelectorAll(".skeleton").length, 4, "the loading module's fallback shows while the resolution is out");
    assert.equal(document.querySelector("main.product h1"), null);
    release();
    await settle();
    assert.equal(document.querySelectorAll(".skeleton").length, 0, "the resolution replaced the fallback");
    assert.equal(document.querySelector("main.product h1")?.textContent, "PLA filament");
    assert.ok(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"][data-sf-mounted]'), "the page hydrated once its resolution landed");
  } finally {
    globalThis.fetch = real;
  }
});

test("a route nothing matches falls back to a full load", async () => {
  const c = ctx({ services: { shopping: { listProducts: () => [] } } });
  await load("/", { ctx: c });
  document.body.insertAdjacentHTML("beforeend", '<a id="nowhere" href="/nowhere">x</a>');
  await fireEvent.click(document.getElementById("nowhere")!);
  assert.equal(location.pathname, "/nowhere", "location.assign took over");
});
