import OrderPage from "@routes/order/[id]/page";
import { expect, render, screen, test } from "@snapfire/fsr-client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2, line_cents: 4800n }, { product_id: 3n, name: "Nozzle", quantity: 1, line_cents: 2400n }] };

test("the order page renders the placed order, as markup nothing hydrates", async () => {
  const r = await render(<OrderPage order={order} />);
  expect(r.hydrated, "a page with no state of its own is static").toBeNull();
  expect(screen.getByText("Order #5001 placed")).toBeTruthy();
  expect(r.container.textContent?.includes("3 items, $72.00 charged."), "the count comes from the ext helper the server rendered").toBeTruthy();
  expect(screen.getByText("PLA filament").getAttribute("href")).toEqual("/product/1");
  expect(screen.getAllByText("× 2").length).toEqual(1);
  expect(screen.getByText("Back to shopping").getAttribute("href")).toEqual("/");
});
