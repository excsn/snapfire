import Loading from "@routes/product/[id]/loading";
import { expect, render, test } from "@snapfire/fsr-client/testing";

test("the loading module renders what the server would, as markup nothing hydrates", async () => {
  const r = await render(<Loading />);
  expect(r.hydrated, "a fallback with no state is static").toBeNull();
  expect(r.container.querySelectorAll(".skeleton").length).toEqual(4);
});
