import { get, set } from "@snapfire/fsr-client";
import { expect, test } from "@snapfire/fsr-client/testing";

import { headline, openAlerts, watching } from "@src/store";

test("the entry module's derive is registered under a spec, so a store write recomputes the headline", () => {
  set(openAlerts, 2);
  set(watching, 1);
  expect(get(headline)).toEqual("2 to look at, watching 1");
  set(openAlerts, 0);
  expect(get(headline), "and it follows every later write").toEqual("quiet, watching 1");
});

test("and the globals the entry module hangs are there", () => {
  const g = globalThis as { __ops?: { headline: () => string | undefined } };
  expect(typeof g.__ops?.headline).toEqual("function");
  set(openAlerts, 5);
  expect(g.__ops?.headline()).toEqual(get(headline));
});
