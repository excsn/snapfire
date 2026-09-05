import { key } from "@snapfire/fsr-client/store";

/** Everything in the cart, seeded by the root layout's loader and shown by the header. */
export const cartCount = key<number>("cart/count");
