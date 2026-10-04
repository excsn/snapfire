import { set } from "@snapfire/fsr-client";
import { expect, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

import { cartCount } from "@src/store";
import { Header } from "@src/ui/Header";

test("the header, mounted fresh, reads the store and keeps its state", async () => {
  set(cartCount, 3);
  const r = await render(<Header />, { hydrate: false });
  expect(r.hydrated).toBeNull();
  expect(screen.getByLabelText("Cart, 3 items").querySelector(".badge")?.textContent).toEqual("3");
  const box = screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement;
  await fireEvent.change(box, "pla");
  expect(box.value).toEqual("pla");
});
