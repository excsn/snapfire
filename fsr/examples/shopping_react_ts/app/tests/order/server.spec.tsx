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

test("a branch in the handler runs on the host: the count moves only on the clicks that open", async () => {
  await load("/order/5001", { ctx: ctx({ services: { shopping: { getOrder: () => order, listProducts: () => [] } } }) });
  const button = document.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"] button[data-sf-on]') as HTMLButtonElement;
  await fireEvent.click(button);
  expect(document.querySelector(".asked-often"), "one opening is not often").toBeNull();
  await fireEvent.click(button);
  expect(document.querySelector(".asked-often"), "closing did not count").toBeNull();
  await fireEvent.click(button);
  expect(document.querySelector(".asked-often")?.textContent).toEqual("Opened 2 times. Chat is the fastest way to reach us.");
  expect(screen.getByText("help@snapfire.shop"), "the set after the branch still ran").toBeTruthy();
});

test("a component inside the island keeps state of its own, addressed under the island's", async () => {
  await load("/order/5001", { ctx: ctx({ services: { shopping: { getOrder: () => order, listProducts: () => [] } } }) });
  const island = document.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"]')!;
  const own = () => island.querySelector('button[data-sf-on="click:0"]') as HTMLButtonElement;
  const hours = () => island.querySelector(".contact-hours")!;
  await fireEvent.click(own());
  expect(hours().textContent).toContain("Weekdays 9 to 5");
  const toggle = hours().querySelector("button[data-sf-on]")!;
  expect(toggle.getAttribute("data-sf-on")?.startsWith("click:c"), `the nested handler binds under its address: ${toggle.getAttribute("data-sf-on")}`).toBeTruthy();
  await fireEvent.click(toggle);
  expect(hours().textContent).toContain("Saturday 10 to 2");
  expect(screen.getByText("help@snapfire.shop"), "the island's own state stayed open").toBeTruthy();
  await fireEvent.click(own());
  expect(document.querySelector(".contact-hours")).toBeNull();
  await fireEvent.click(own());
  expect(hours().textContent, "a component the render stopped placing starts afresh when it returns").toContain("Weekdays 9 to 5");
});
