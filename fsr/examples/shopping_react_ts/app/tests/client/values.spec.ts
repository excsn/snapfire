import { decodeValue, encodeValue, f64, ref, variant } from "@snapfire/fsr-client";
import { expect, test } from "@snapfire/fsr-client/testing";

test("integers are tagged on the way out and come back as numbers while they fit", () => {
  expect(encodeValue(12n)).toEqual({ $: "i", v: "12" });
  expect(encodeValue(12)).toEqual(12);
  expect(decodeValue({ $: "i", v: "12" })).toEqual(12);
  expect(decodeValue({ $: "u", v: "9007199254740993" })).toEqual(9007199254740993n);
  expect(encodeValue(NaN)).toEqual({ $: "f", v: "nan" });
  expect(decodeValue({ $: "f", v: "-inf" })).toEqual(-Infinity);
});

test("a u8 typed array goes back as the typed array it came as, and bytes the page made stay bytes", () => {
  const typed = decodeValue({ $: "ta", k: "u8", v: "AQID" }) as Uint8Array;
  expect(typed instanceof Uint8Array).toBeTruthy();
  expect(Array.from(typed)).toEqual([1, 2, 3]);
  expect(encodeValue(typed)).toEqual({ $: "ta", k: "u8", v: "AQID" });
  expect(encodeValue(new Uint8Array([1, 2, 3]))).toEqual({ $: "b", v: "AQID" });
  expect(Array.from(decodeValue({ $: "b", v: "AQID" }) as Uint8Array)).toEqual([1, 2, 3]);
  const floats = encodeValue(new Float64Array([1.5])) as { $: string; k: string };
  expect([floats.$, floats.k]).toEqual(["ta", "f64"]);
  expect(Array.from(decodeValue(floats) as Float64Array)).toEqual([1.5]);
});

test("maps with a dollar key, variants and references survive a round trip", () => {
  const dollar = { $: "x", y: 1 };
  expect(encodeValue(dollar)).toEqual({ $: "m", v: [["$", "x"], ["y", 1]] });
  expect(decodeValue(encodeValue(dollar))).toEqual(dollar);
  expect(encodeValue(variant("some", 2n))).toEqual({ $: "var", t: "some", p: { $: "i", v: "2" } });
  expect(decodeValue(encodeValue(variant("none")))).toEqual(variant("none"));
  expect(decodeValue(encodeValue(ref("action", "cart.add")))).toEqual(ref("action", "cart.add"));
});

test("f64 says double where the number alone cannot", () => {
  expect(encodeValue(0), "a whole number is an integer, which is what a contract saying f64 refuses").toEqual(0);
  expect(encodeValue(f64(0))).toEqual({ $: "f", v: 0 });
  expect(encodeValue(f64(2.5))).toEqual({ $: "f", v: 2.5 });
  expect(encodeValue(f64(Infinity))).toEqual({ $: "f", v: "inf" });
  expect(decodeValue(encodeValue(f64(0))), "it comes back a plain number: the tag is for the way out").toEqual(0);
  expect(encodeValue({ cpu: f64(0), name: "idle" }), "and it nests").toEqual({ cpu: { $: "f", v: 0 }, name: "idle" });
});
