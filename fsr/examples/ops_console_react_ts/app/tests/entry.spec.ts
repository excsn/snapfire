import { get, set } from "@snapfire/fsr-client";
import { assert, test } from "@snapfire/fsr-client/testing";

import { headline, openAlerts, watching } from "@src/store";

test("the entry module's derive is registered under a spec, so a store write recomputes the headline", () => {
  set(openAlerts, 2);
  set(watching, 1);
  assert.equal(get(headline), "2 to look at, watching 1");
  set(openAlerts, 0);
  assert.equal(get(headline), "quiet, watching 1", "and it follows every later write");
});

test("and the globals the entry module hangs are there", () => {
  const g = globalThis as { __ops?: { headline: () => string | undefined } };
  assert.equal(typeof g.__ops?.headline, "function");
  set(openAlerts, 5);
  assert.equal(g.__ops?.headline(), get(headline));
});
