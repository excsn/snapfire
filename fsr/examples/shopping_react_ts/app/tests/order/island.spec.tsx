import { ctx, expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2n, line_cents: 4800n }] };

test("a component placed with <Island> mounts in its own root at its own timing and keeps its state", async () => {
  const c = ctx({ services: { shopping: { getOrder: () => order, listProducts: () => [] } } });
  await load("/order/5001", { ctx: c });
  expect(document.querySelector('sf-i[data-sf-module="routes/order/[id]/page.tsx#default"]'), "the page has no state of its own, so it is markup rather than an island").toBeNull();
  const heading = screen.getByText("Order #5001 placed");
  const region = document.querySelector("sf-s[data-sf-island]");
  expect(region, "the page's markup holds the island's region").toBeTruthy();
  expect(region!.getAttribute("data-sf-when")).toEqual("visible");
  const help = region!.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"][data-sf-mounted]');
  expect(help, "the island mounted once it was visible").toBeTruthy();
  expect(screen.getByText("Quote order #5001 when you write to us."), "rendered on the server from the page's data").toBeTruthy();

  await fireEvent.click(screen.getByText("Show contact options"));
  expect(screen.getByText("help@snapfire.shop"), "the island's own state").toBeTruthy();
  expect(screen.getByText("Hide contact options")).toBeTruthy();
  expect(screen.getByText("Order #5001 placed"), "the page's markup was untouched").toBe(heading);
});
