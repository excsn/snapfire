import { setLocale } from "@snapfire/fsr-client";
import { crypto, intl, text, time } from "@snapfire/fsr-client/std";
import { assert, test } from "@snapfire/fsr-client/testing";

test("the standard library answers under the document's locale, through the runner where the engine has no Intl", () => {
  setLocale("fr_FR");
  assert.equal(intl.number(1234.5), "1 234,5");
  assert.equal(intl.plural(0), "one");
  assert.equal(intl.date(Date.UTC(2026, 8, 5), "long"), "5 septembre 2026");
  setLocale("en_US");
  assert.equal(intl.number(1234.5), "1,234.5");
  assert.equal(intl.number(2, { minimumFractionDigits: 2 }), "2.00");
  assert.equal(intl.currency(12, "USD"), "USD 12.00");
  assert.equal(intl.plural(1), "one");
  assert.equal(intl.plural(2), "other");
  assert.equal(intl.date("2026-09-05T23:59:00Z"), "Sep 5, 2026");
});

test("text, time and crypto compute in the browser half itself", () => {
  assert.equal(text.slug("  Crème Brûlée & Café! "), "creme-brulee-cafe");
  assert.equal(text.truncate("héllo wörld", 5), "héllo…");
  const at = Date.UTC(2026, 8, 5, 16, 41, 7, 250);
  assert.equal(time.format(at, "YYYY-MM-DD HH:mm:ss.SSS"), "2026-09-05 16:41:07.250");
  assert.equal(time.add(at, 36, "h"), at + 36 * 3_600_000);
  assert.equal(time.diff(at, at - 90_000, "m"), 1.5);
  assert.equal(time.parse("2026-09-05T18:41+02:00"), Date.UTC(2026, 8, 5, 16, 41));
  assert.equal(time.parse("Sep 5 2026"), null);
  assert.equal(crypto.hash("hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
  assert.ok(crypto.verify("hello", "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"));
  assert.ok(!crypto.verify("hello!", "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"));
});
