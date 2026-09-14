import { action, ActionFailure } from "@snapfire/fsr-client";
import { expect, test } from "@snapfire/fsr-client/testing";

test("an action the build did not lower fails with the kind and message the host answered", async () => {
  const call = action("cart.nothing", { revalidate: false });
  let failure: unknown = null;
  try {
    await call({});
  } catch (e) {
    failure = e;
  }
  expect(failure instanceof ActionFailure, "an ActionFailure, not a parse error").toBeTruthy();
  expect((failure as ActionFailure).kind).toEqual("internal");
  expect((failure as ActionFailure).message.includes("not a lowered action")).toBeTruthy();
});
