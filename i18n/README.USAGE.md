# Usage Guide: snapfire_i18n

How to write currency amounts, percentages and relative times with snapfire_i18n, how to map case under a locale and how to regenerate its data.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Writing a Currency Amount](#writing-a-currency-amount)
  * [Choosing the code or a symbol](#choosing-the-code-or-a-symbol)
  * [Rounding to the currency's digits](#rounding-to-the-currencys-digits)
* [Writing a Percentage](#writing-a-percentage)
* [Writing a Relative Time](#writing-a-relative-time)
  * [Words for nearby offsets](#words-for-nearby-offsets)
  * [Choosing the plural](#choosing-the-plural)
* [Writing the Digits](#writing-the-digits)
* [Mapping Case Under a Locale](#mapping-case-under-a-locale)
* [Regenerating the Data](#regenerating-the-data)
* [Error Handling](#error-handling)

## Core Concepts

* **Tag**: a BCP 47 locale tag such as `en-US`, `de-DE` or `zh-TW`, passed as `&str`.
* **Locale fallback**: a tag reads the most specific CLDR locale it falls back to, ending at the root locale.
* **Digits**: the absolute number already written in the locale's digits and separators, `1,234.50` or `١٬٢٣٤٫٥٠`. The caller writes them.
* **Sign**: `None`, `Minus` or `Plus`, placed where the locale's pattern puts it.
* **Pattern**: CLDR's text around a number, such as `¤#,##0.00` or `#,##0 %`.
* **Negative subpattern**: the part of a pattern after `;`, used for a negative number instead of a minus before the positive pattern.
* **Display**: whether a currency is written as its code, its symbol or its narrow symbol.
* **Alpha next to number**: the pattern a locale uses when the currency text has a letter touching the number, which adds a space: `USD 1,234.50` against `$1,234.50`.
* **Fraction digits**: the decimals a currency is written with by default: 2 for `USD`, 0 for `JPY`, 3 for `KWD`.
* **Unit**: the unit of a relative time, `Second` to `Year`.
* **Width**: `Long`, `Short` or `Narrow` wording.
* **Plural**: the CLDR plural category the locale's rules give the number.
* **Literal**: a word for a nearby offset, such as "yesterday", which `numeric: "auto"` writes instead of a number.
* **Case mapping**: upper or lower case as Unicode defines it, changed for a few languages. Turkish, Azerbaijani, Lithuanian and Greek have their own rules. Every other tag maps as Unicode's default does.

## Quick Start

```rust
use snapfire_i18n::{Currency, Display, Percent, Plural, Relative, Sign, Unit, Width};

fn main() {
  let euro = Currency::new("de-DE", "EUR", Display::Symbol).unwrap();
  assert_eq!(euro.format("1.234,50", Sign::None), "1.234,50\u{a0}€");

  assert_eq!(Percent::new("fr-FR").format("26", Sign::None), "26\u{a0}%");

  let days = Relative::new("en-US", Unit::Day, Width::Long);
  assert_eq!(days.format("3", false, Plural::Other, None), "in 3 days");
  assert_eq!(days.literal(-1), Some("yesterday"));
}
```

## Writing a Currency Amount

Make a `Currency` once for a tag, a code and a display, then format any number of amounts with it:

```rust
use snapfire_i18n::{Currency, Display, Sign};

let usd = Currency::new("en-US", "USD", Display::Code)?;
assert_eq!(usd.format("1,234.50", Sign::None), "USD\u{a0}1,234.50");
assert_eq!(usd.format("1,234.50", Sign::Minus), "-USD\u{a0}1,234.50");
```

### Choosing the code or a symbol

`Display::Code` writes the ISO code. `Display::Symbol` writes the locale's symbol and `Display::NarrowSymbol` its narrow one, each falling back to the code:

```rust
use snapfire_i18n::{Currency, Display};

assert_eq!(Currency::new("en-US", "USD", Display::Symbol)?.shown(), "$");
assert_eq!(Currency::new("zh-TW", "USD", Display::Symbol)?.shown(), "US$");
assert_eq!(Currency::new("es-ES", "EGP", Display::NarrowSymbol)?.shown(), "EGP");
```

### Rounding to the currency's digits

`digits()` is what `Intl.NumberFormat` uses for both fraction bounds unless told otherwise. Round the amount to it, half away from zero as the browser does, before writing the digits:

```rust
use snapfire_i18n::{Currency, Display, Sign};

let yen = Currency::new("ja-JP", "JPY", Display::Code)?;
assert_eq!(yen.digits(), 0);
assert_eq!(yen.format("1,235", Sign::None), "JPY\u{a0}1,235");
assert_eq!(Currency::new("en-US", "KWD", Display::Code)?.digits(), 3);
```

## Writing a Percentage

Multiply by 100 before writing the digits. The percent sign and its spacing come from the locale:

```rust
use snapfire_i18n::{Percent, Sign};

assert_eq!(Percent::new("en-US").format("26", Sign::None), "26%");
assert_eq!(Percent::new("de-DE").format("4,2", Sign::Minus), "-4,2\u{a0}%");
assert_eq!(Percent::new("ar-EG").format("٢٦", Sign::None), "٢٦٪\u{61c}");
```

## Writing a Relative Time

`past` is true for a negative value. Pass the absolute value as the digits:

```rust
use snapfire_i18n::{Plural, Relative, Unit, Width};

let hours = Relative::new("en-US", Unit::Hour, Width::Short);
assert_eq!(hours.format("2", false, Plural::Other, None), "in 2 hr.");
assert_eq!(hours.format("2", true, Plural::Other, None), "2 hr. ago");
```

### Words for nearby offsets

`literal` is the word `numeric: "auto"` writes for a whole offset, where the locale has one. Fall back to `format` when it returns `None`:

```rust
use snapfire_i18n::{Plural, Relative, Unit, Width};

let weeks = Relative::new("en-US", Unit::Week, Width::Long);
let text = weeks.literal(1).map(str::to_owned).unwrap_or_else(|| weeks.format("1", false, Plural::One, Some(1)));
assert_eq!(text, "next week");
```

### Choosing the plural

The plural category comes from the locale's plural rules, for example from `icu_plurals`. `exact` is the value when it is exactly 0 or 1:

```rust
use snapfire_i18n::{Plural, Relative, Unit, Width};

let days = Relative::new("ar-EG", Unit::Day, Width::Long);
assert_eq!(days.format("١", true, Plural::One, Some(1)), "قبل يوم واحد");
```

## Writing the Digits

The digits are the caller's, so grouping, rounding and the numbering system follow whatever formatter writes them. With `icu_decimal`:

```rust
use fixed_decimal::Decimal;
use icu_decimal::DecimalFormatter;
use icu_locale_core::locale;
use snapfire_i18n::{Currency, Display, Sign};

let formatter = DecimalFormatter::try_new(locale!("de-DE").into(), Default::default())?;
let amount: Decimal = "1234.50".parse()?;
let digits = formatter.format(&amount).to_string();
let euro = Currency::new("de-DE", "EUR", Display::Symbol)?;
assert_eq!(euro.format(&digits, Sign::None), "1.234,50\u{a0}€");
```

## Mapping Case Under a Locale

`upper_case` and `lower_case` take the text and a tag and answer what `toLocaleUpperCase` and `toLocaleLowerCase` answer in a browser:

```rust
use snapfire_i18n::{lower_case, upper_case};

assert_eq!(upper_case("istanbul", "tr"), "İSTANBUL");
assert_eq!(lower_case("ISPARTA", "tr-TR"), "ısparta");
assert_eq!(upper_case("άλφα", "el"), "ΑΛΦΑ");
assert_eq!(upper_case("άλφα", "en"), "ΆΛΦΑ");
assert_eq!(lower_case("I\u{300}", "lt"), "i\u{307}\u{300}");
assert_eq!(upper_case("straße", "de"), "STRASSE");
```

Only the language subtag matters. A tag that is not well formed maps as Unicode's default does, so check it first if it comes from outside.

## Regenerating the Data

`data/cldr.txt` is written by the `datagen` crate beside this one from a cldr-json release. It reads the `cldr-core`, `cldr-numbers-full` and `cldr-dates-full` packages:

```sh
curl -LO https://github.com/unicode-org/cldr-json/releases/download/48.2.1/cldr-48.2.1-json-full.zip
unzip -q cldr-48.2.1-json-full.zip -d cldr
cargo run --release -p snapfire_i18n_datagen -- cldr i18n/data/cldr.txt
```

Use the CLDR release the number formatter's data was built from, so the digits and the patterns agree.

`data/ucd.txt` is written by the same crate from a Unicode Character Database release, with `--ucd`:

```sh
mkdir ucd
for f in UnicodeData.txt DerivedCoreProperties.txt PropList.txt Scripts.txt; do curl -so ucd/$f https://www.unicode.org/Public/17.0.0/ucd/$f; done
cargo run --release -p snapfire_i18n_datagen -- --ucd ucd i18n/data/ucd.txt
```

Use the Unicode release of the Rust toolchain that builds the crate, since the default mapping is the standard library's.

## Error Handling

`Currency::new` is the only call that fails. Its `Error::NotACurrency` holds the code that is not three ASCII letters:

```rust
use snapfire_i18n::{Currency, Display, Error};

match Currency::new("en-US", "dollars", Display::Code) {
  Err(Error::NotACurrency(code)) => assert_eq!(code, "dollars"),
  Err(other) => panic!("{other}"),
  Ok(_) => unreachable!(),
}
```
