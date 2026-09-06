import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";

export const checkout = action(async ({ session, services }: ActionCtx) => {
  if (Object.keys(session.cart).length === 0) {
    fail("invalid", "the cart is empty");
  }
  return await services.shopping.placeOrder({ lines: Object.entries(session.cart) });
});
