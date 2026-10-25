import { enableNavigation } from "@snapfire/fsr/client";
import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr/client/testing";

const order = { id: 5001n, total_cents: 7200n, lines: [{ product_id: 1n, name: "PLA filament", quantity: 2n, line_cents: 4800n }] };

test("with activity on, a step back keeps the order page's markup and its server island with the state it held", async () => {
  await load("/order/5001", { ctx: ctx({ session: { cart: {} }, services: { shopping: { getOrder: () => order, listProducts: () => [] } } }) });
  enableNavigation({ activity: true });
  try {
    await settle();
    const heading = screen.getByText("Order #5001 placed");
    const help = document.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"]');
    await fireEvent.click(screen.getByText("Show contact options"));
    expect(screen.getByText("help@snapfire.shop"), "the island's own state").toBeTruthy();

    await fireEvent.click(screen.getByLabelText("Cart, 0 items"));
    await settle();
    expect(location.pathname).toEqual("/cart");
    history.back();
    await settle();

    expect(location.pathname).toEqual("/order/5001");
    expect(screen.getByText("Order #5001 placed"), "the held markup came back").toBe(heading);
    const again = document.querySelector('sf-i[data-sf-module="src/ui/OrderHelp.tsx#OrderHelp"][data-sf-mounted]');
    expect(again, "a server island runs nothing in the browser, so it was held rather than ended").toBe(help);
    expect(screen.getByText("help@snapfire.shop"), "with its state").toBeTruthy();
  } finally {
    enableNavigation({ activity: false });
  }
});
