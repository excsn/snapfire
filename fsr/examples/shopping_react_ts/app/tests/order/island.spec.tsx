import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2n, line_cents: 4800n }] };

test("a component placed with <Island> mounts in its own root at its own timing and keeps its state", async () => {
  const c = ctx({ services: { shopping: { getOrder: () => order, listProducts: () => [] } } });
  await load("/order/5001", { ctx: c });
  assert.equal(document.querySelector('sf-i[data-sf-module="routes/order/[id]/page.tsx#default"]'), null, "the page has no state of its own, so it is markup rather than an island");
  const heading = screen.getByText("Order #5001 placed");
  const region = document.querySelector("sf-s[data-sf-island]");
  assert.ok(region, "the page's markup holds the island's region");
  assert.equal(region!.getAttribute("data-sf-when"), "visible");
  const help = region!.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"][data-sf-mounted]');
  assert.ok(help, "the island mounted once it was visible");
  assert.ok(screen.getByText("Quote order #5001 when you write to us."), "rendered on the server from the page's data");

  await fireEvent.click(screen.getByText("Show contact options"));
  assert.ok(screen.getByText("help@snapfire.shop"), "the island's own state");
  assert.ok(screen.getByText("Hide contact options"));
  assert.ok(screen.getByText("Order #5001 placed") === heading, "the page's markup was untouched");
});
