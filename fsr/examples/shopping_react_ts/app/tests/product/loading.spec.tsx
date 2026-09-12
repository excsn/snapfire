import Loading from "@routes/product/[id]/loading";
import { assert, render, test } from "@snapfire/fsr-client/testing";

test("the loading module renders what the server would, as markup nothing hydrates", async () => {
  const r = await render(<Loading />);
  assert.equal(r.hydrated, null, "a fallback with no state is static");
  assert.equal(r.container.querySelectorAll(".skeleton").length, 4);
});
