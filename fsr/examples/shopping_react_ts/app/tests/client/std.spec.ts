import { setLocale } from "@snapfire/fsr/client";
import { crypto, text, time } from "@snapfire/fsr/client";
import { localeTag } from "@snapfire/fsr/client/intl";
import { expect, test } from "@snapfire/fsr/client/testing";

test("Intl answers under the document's locale, through the runner where the engine has no Intl of its own", () => {
  setLocale("fr_FR");
  expect(new Intl.NumberFormat(localeTag()).format(1234.5)).toEqual("1\u202f234,5");
  expect(new Intl.PluralRules(localeTag()).select(0)).toEqual("one");
  expect(new Intl.DateTimeFormat(localeTag(), { dateStyle: "long", timeZone: "UTC" }).format(Date.UTC(2026, 8, 5))).toEqual("5 septembre 2026");
  setLocale("en_US");
  expect(new Intl.NumberFormat(localeTag()).format(1234.5)).toEqual("1,234.5");
  expect(new Intl.NumberFormat(localeTag(), { minimumFractionDigits: 2 }).format(2)).toEqual("2.00");
  expect(new Intl.NumberFormat(localeTag(), { style: "currency", currency: "USD", currencyDisplay: "code" }).format(12)).toEqual("USD\u00a012.00");
  expect(new Intl.PluralRules(localeTag()).select(1)).toEqual("one");
  expect(new Intl.PluralRules(localeTag()).select(2)).toEqual("other");
  expect(new Intl.DateTimeFormat(localeTag(), { dateStyle: "medium", timeZone: "UTC" }).format(new Date("2026-09-05T23:59:00Z"))).toEqual("Sep 5, 2026");
  expect(["zebra", "Äpfel", "apple", "B"].sort(new Intl.Collator(localeTag()).compare), "the order Chrome sorts them in under English").toEqual(["Äpfel", "apple", "B", "zebra"]);
  expect(["a".localeCompare("a"), "ö".localeCompare("z", "sv")]).toEqual([0, 1]);
});

test("text, time and crypto compute in the browser half itself", () => {
  expect(text.slug("  Crème Brûlée & Café! ")).toEqual("creme-brulee-cafe");
  expect(text.truncate("héllo wörld", 5)).toEqual("héllo…");
  const at = Date.UTC(2026, 8, 5, 16, 41, 7, 250);
  expect(time.format(at, "YYYY-MM-DD HH:mm:ss.SSS")).toEqual("2026-09-05 16:41:07.250");
  expect(time.add(at, 36, "h")).toEqual(at + 36 * 3_600_000);
  expect(time.diff(at, at - 90_000, "m")).toEqual(1.5);
  expect(time.parse("2026-09-05T18:41+02:00")).toEqual(Date.UTC(2026, 8, 5, 16, 41));
  expect(time.parse("Sep 5 2026")).toBeNull();
  expect(crypto.hash("hello")).toEqual("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
  expect(crypto.verify("hello", "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824")).toBeTruthy();
  expect(crypto.verify("hello!", "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")).toBeFalsy();
});
