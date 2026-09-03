import { action, ActionFailure } from "@snapfire/fsr-client";
import { assert, test } from "@snapfire/fsr-client/testing";

test("an action the build did not lower fails with the kind and message the host answered", async () => {
  const call = action("cart.nothing", { revalidate: false });
  let failure: unknown = null;
  try {
    await call({});
  } catch (e) {
    failure = e;
  }
  assert.ok(failure instanceof ActionFailure, "an ActionFailure, not a parse error");
  assert.equal((failure as ActionFailure).kind, "internal");
  assert.ok((failure as ActionFailure).message.includes("not a lowered action"));
});
