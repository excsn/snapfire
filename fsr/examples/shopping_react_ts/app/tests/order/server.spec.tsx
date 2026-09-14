import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2n, line_cents: 4800n }] };

test("the order help island runs in server mode: a click round-trips and the markup is patched in place", async () => {
  await load("/order/5001", { ctx: ctx({ services: { shopping: { getOrder: () => order, listProducts: () => [] } } }) });
  const island = document.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"]');
  expect(island, "the island is on the page").toBeTruthy();
  expect(island?.parentElement?.getAttribute("data-sf-mode")).toEqual("server");
  expect(island?.hasAttribute("data-sf-mounted"), "mounted, with no React root").toBeTruthy();
  const button = island?.querySelector("button[data-sf-on]") as HTMLButtonElement;
  expect(button.getAttribute("data-sf-on"), "the handler is bound by the server's marker").toEqual("click:0");
  expect(document.querySelector(".contact-options")).toBeNull();
  const heading = island?.querySelector("h2");

  await fireEvent.click(button);
  expect(screen.getByText("help@snapfire.shop"), "the server rendered the open state").toBeTruthy();
  expect(button.textContent).toEqual("Hide contact options");
  expect(island?.querySelector("h2"), "the untouched heading is the same node").toBe(heading);
  expect(island?.hasAttribute("data-sf-pending")).toEqual(false);

  await fireEvent.click(button);
  expect(document.querySelector(".contact-options"), "a second click closes it again from the state the browser carried").toBeNull();
});
