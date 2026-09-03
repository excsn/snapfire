import OrderPage from "@routes/order/[id]/page";
import { assert, render, screen, test } from "@snapfire/fsr-client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2, line_cents: 4800n }, { product_id: 3n, name: "Nozzle", quantity: 1, line_cents: 2400n }] };

test("the order page hydrates over the placed order", async () => {
  const r = await render(<OrderPage order={order} cartCount={0n} />);
  assert.equal(r.hydrated, "routes/order/[id]/page.tsx#default");
  assert.ok(screen.getByText("Order #5001 placed"));
  assert.equal(screen.getByText("PLA filament").getAttribute("href"), "/product/1");
  assert.equal(screen.getAllByText("× 2").length, 1);
  assert.equal(screen.getByText("Back to shopping").getAttribute("href"), "/");
});
