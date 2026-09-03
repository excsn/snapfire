import { Header } from "@src/ui/Header";
import { assert, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

test("a component below a page mounts fresh and keeps its state", async () => {
  const r = await render(<Header cartCount={3n} />);
  assert.equal(r.hydrated, null);
  assert.equal(screen.getByLabelText("Cart, 3 items").querySelector(".badge")?.textContent, "3");
  const box = screen.getByPlaceholderText("Search snapfire.shop") as HTMLInputElement;
  await fireEvent.change(box, "pla");
  assert.equal(box.value, "pla");
});
