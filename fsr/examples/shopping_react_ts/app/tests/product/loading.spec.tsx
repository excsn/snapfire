import Loading from "@routes/product/[id]/loading";
import { assert, render, test } from "@snapfire/fsr-client/testing";

test("the loading module hydrates over what the server rendered", async () => {
  const r = await render(<Loading />);
  assert.equal(r.hydrated, "routes/product/[id]/loading.tsx#default");
  assert.equal(r.container.querySelectorAll(".skeleton").length, 4);
});
