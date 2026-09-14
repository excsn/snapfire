import { navigate } from "@snapfire/fsr-client";
import { ctx, expect, fireEvent, load, screen, settle, spyOn, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

test("a click from the catalog to the cart swaps the page and keeps the document", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const app = document.getElementById("app");
  expect(app, "the shell mounted the page under #app").toBeTruthy();
  expect(screen.getByText("Today's picks")).toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/page.tsx#default"]'), "the catalog hydrates since its cards' add buttons run in the browser").toBeTruthy();

  await fireEvent.click(screen.getByLabelText("Cart, 2 items"));

  expect(location.pathname).toEqual("/cart");
  expect(document.getElementById("app"), "the shell's DOM survived the navigation").toBe(app);
  expect(screen.queryByText("Today's picks")).toBeNull();
  expect(screen.getByText("Shopping cart")).toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/cart/page.tsx#default"][data-sf-mounted]'), "the cart hydrated in place").toBeTruthy();
  expect(
    c.trace.calls.map((call) => call.method),
    "each page's loader and the promo slot's ran once per document, through the mocks",
  ).toEqual(["listProducts", "listProducts", "listProducts", "listProducts"]);
});

test("a click on a streamed route shows its fallback, then the fill when the resolution lands", async () => {
  const stock = { product_id: 1n, on_hand: 5n, reserved: 0n, warehouse: "Prague", bins: ["A1"] };
  const c = ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament], getProduct: () => filament }, inventory: { getStock: () => stock } } });
  await load("/", { ctx: c });
  const whole = await (await fetch("/product/1?__payload")).text();
  const cut = whole.indexOf("\nS ");
  expect(cut !== -1, "the product page streams behind its loading module").toBeTruthy();
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
    expect(location.pathname, "history moved with the eager wave").toEqual("/product/1");
    expect(document.querySelectorAll(".skeleton").length, "the loading module's fallback shows while the resolution is out").toEqual(4);
    expect(document.querySelector("main.product h1")).toBeNull();
    release();
    await settle();
    expect(document.querySelectorAll(".skeleton").length, "the resolution replaced the fallback").toEqual(0);
    expect(document.querySelector("main.product h1")?.textContent).toEqual("PLA filament");
    expect(document.querySelector('sf-i[data-sf-module="routes/product/[id]/page.tsx#default"][data-sf-mounted]'), "the page hydrated once its resolution landed").toBeTruthy();
  } finally {
    globalThis.fetch = real;
  }
});

test("a route nothing matches falls back to a full load", async () => {
  const c = ctx({ services: { shopping: { listProducts: () => [] } } });
  await load("/", { ctx: c });
  document.body.insertAdjacentHTML("beforeend", '<a id="nowhere" href="/nowhere">x</a>');
  await fireEvent.click(document.getElementById("nowhere")!);
  expect(location.pathname, "location.assign took over").toEqual("/nowhere");
});

test("a link to a fragment of the page it is on scrolls to the element without fetching", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const target = document.createElement("div");
  target.id = "picks";
  document.body.append(target);
  const link = document.createElement("a");
  link.setAttribute("href", "#picks");
  link.textContent = "to the picks";
  document.body.append(link);
  const scrolled = spyOn(target, "scrollIntoView");
  const fetched = spyOn(globalThis, "fetch");
  await fireEvent.click(link);
  expect(location.pathname + location.hash).toEqual("/#picks");
  expect(scrolled).toHaveBeenCalledTimes(1);
  expect(fetched, "the page is already showing").not.toHaveBeenCalled();
  fetched.mockRestore();
});

test("a link to a fragment of another page scrolls to the element it names once the page is in", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const app = document.getElementById("app")!;
  const link = document.createElement("a");
  link.setAttribute("href", "/cart#app");
  link.textContent = "to the cart";
  document.body.append(link);
  const scrolled = spyOn(app, "scrollIntoView");
  const top = spyOn(window, "scrollTo");
  await fireEvent.click(link);
  expect(location.pathname + location.hash).toEqual("/cart#app");
  expect(screen.getByText("Shopping cart")).toBeTruthy();
  expect(scrolled).toHaveBeenCalledTimes(1);
  expect(top, "rather than to the top").not.toHaveBeenCalled();
  top.mockRestore();
});

const CATALOG = 'sf-i[data-sf-module="routes/page.tsx#default"]';

test("a click that changes only the query keeps the page's island and hands it the new props", async () => {
  await load("/", { ctx: ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament] } } }) });
  const island = document.querySelector(CATALOG);
  const chip = document.querySelectorAll("a.chip")[1];
  const href = chip.getAttribute("href") ?? "";
  await fireEvent.click(chip);
  expect(location.pathname + location.search).toEqual(href);
  expect(document.querySelector(CATALOG), "the catalog is the island that was there").toBe(island);
  expect(document.querySelector("a.chip-active")?.getAttribute("href"), "and it rendered the category it was sent").toEqual(href);
});

test("a navigation told not to keep replaces the page's island and one told to replace adds no history entry", async () => {
  await load("/", { ctx: ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament] } } }) });
  const island = document.querySelector(CATALOG);
  const entries = history.length;
  await navigate("/?category=printing", true, { keep: false, replace: true });
  expect(location.pathname + location.search).toEqual("/?category=printing");
  expect(document.querySelector(CATALOG), "a catalog is placed").toBeTruthy();
  expect(document.querySelector(CATALOG), "and it is a new one").not.toBe(island);
  expect(history.length, "in place of the entry it moved from").toEqual(entries);
});

test("a link that says not to keep replaces the page's island on a change of query", async () => {
  await load("/", { ctx: ctx({ session: { cart: {} }, services: { shopping: { listProducts: () => [filament] } } }) });
  const island = document.querySelector(CATALOG);
  const chip = document.querySelectorAll("a.chip")[1];
  const href = chip.getAttribute("href") ?? "";
  chip.setAttribute("data-sf-keep", "false");
  await fireEvent.click(chip);
  expect(location.pathname + location.search).toEqual(href);
  expect(document.querySelector(CATALOG), "a new catalog in place of the one that was there").not.toBe(island);
});
