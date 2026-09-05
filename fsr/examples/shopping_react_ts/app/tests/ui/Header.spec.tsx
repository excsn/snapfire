import { set } from "@snapfire/fsr-client";
import { assert, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

import { cartCount } from "@src/store";
import { Header } from "@src/ui/Header";

test("a component below a page mounts fresh and keeps its state", async () => {
  set(cartCount, 3);
  const r = await render(<Header />);
  assert.equal(r.hydrated, null);
  assert.equal(screen.getByLabelText("Cart, 3 items").querySelector(".badge")?.textContent, "3");
  const box = screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement;
  await fireEvent.change(box, "pla");
  assert.equal(box.value, "pla");
});
