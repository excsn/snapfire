import { decodeValue, encodeValue, ref, variant } from "@snapfire/fsr-client";
import { assert, test } from "@snapfire/fsr-client/testing";

test("integers are tagged on the way out and come back as numbers while they fit", () => {
  assert.equal(encodeValue(12n), { $: "i", v: "12" });
  assert.equal(encodeValue(12), 12);
  assert.equal(decodeValue({ $: "i", v: "12" }), 12);
  assert.equal(decodeValue({ $: "u", v: "9007199254740993" }), 9007199254740993n);
  assert.equal(encodeValue(NaN), { $: "f", v: "nan" });
  assert.equal(decodeValue({ $: "f", v: "-inf" }), -Infinity);
});

test("a u8 typed array goes back as the typed array it came as, and bytes the page made stay bytes", () => {
  const typed = decodeValue({ $: "ta", k: "u8", v: "AQID" }) as Uint8Array;
  assert.ok(typed instanceof Uint8Array);
  assert.equal(Array.from(typed), [1, 2, 3]);
  assert.equal(encodeValue(typed), { $: "ta", k: "u8", v: "AQID" });
  assert.equal(encodeValue(new Uint8Array([1, 2, 3])), { $: "b", v: "AQID" });
  assert.equal(Array.from(decodeValue({ $: "b", v: "AQID" }) as Uint8Array), [1, 2, 3]);
  const floats = encodeValue(new Float64Array([1.5])) as { $: string; k: string };
  assert.equal([floats.$, floats.k], ["ta", "f64"]);
  assert.equal(Array.from(decodeValue(floats) as Float64Array), [1.5]);
});

test("maps with a dollar key, variants and references survive a round trip", () => {
  const dollar = { $: "x", y: 1 };
  assert.equal(encodeValue(dollar), { $: "m", v: [["$", "x"], ["y", 1]] });
  assert.equal(decodeValue(encodeValue(dollar)), dollar);
  assert.equal(encodeValue(variant("some", 2n)), { $: "var", t: "some", p: { $: "i", v: "2" } });
  assert.equal(decodeValue(encodeValue(variant("none"))), variant("none"));
  assert.equal(decodeValue(encodeValue(ref("action", "cart.add"))), ref("action", "cart.add"));
});
