import { assert, ctx, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2n, line_cents: 4800n }] };

test("the order help island runs in server mode: a click round-trips and the markup is patched in place", async () => {
  await load("/order/5001", { ctx: ctx({ services: { shopping: { getOrder: () => order, listProducts: () => [] } } }) });
  const island = document.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"]');
  assert.ok(island, "the island is on the page");
  assert.equal(island?.parentElement?.getAttribute("data-sf-mode"), "server");
  assert.ok(island?.hasAttribute("data-sf-mounted"), "mounted, with no React root");
  const button = island?.querySelector("button[data-sf-on]") as HTMLButtonElement;
  assert.equal(button.getAttribute("data-sf-on"), "click:0", "the handler is bound by the server's marker");
  assert.equal(document.querySelector(".contact-options"), null);
  const heading = island?.querySelector("h2");

  await fireEvent.click(button);
  assert.ok(screen.getByText("help@snapfire.shop"), "the server rendered the open state");
  assert.equal(button.textContent, "Hide contact options");
  assert.equal(island?.querySelector("h2"), heading, "the untouched heading is the same node");
  assert.equal(island?.hasAttribute("data-sf-pending"), false);

  await fireEvent.click(button);
  assert.equal(document.querySelector(".contact-options"), null, "a second click closes it again from the state the browser carried");
});
