# API Reference: snapfire_i18n

Currency, percent and relative-time formatting from embedded CLDR data. Case mapping under a locale from embedded Unicode data.

## Contents

* [1. Locales](#1-locales)
  * [cldr_version](#cldr_version)
* [2. Signs](#2-signs)
  * [Sign](#sign)
* [3. Currency](#3-currency)
  * [Display](#display)
  * [Currency](#currency)
* [4. Percent](#4-percent)
  * [Percent](#percent)
* [5. Relative time](#5-relative-time)
  * [Unit](#unit)
  * [Width](#width)
  * [Plural](#plural)
  * [Relative](#relative)
* [6. Case](#6-case)
  * [upper_case](#upper_case)
  * [lower_case](#lower_case)
  * [unicode_version](#unicode_version)
* [7. Error handling](#7-error-handling)
  * [Error](#error)

## 1. Locales

Every constructor takes a BCP 47 tag as `&str`. `_` separates subtags as `-` does. Anything after the region is ignored. A tag reads the most specific CLDR locale it falls back to: language-script-region, language-region, language-script, language, then the root locale. A tag CLDR lists as another locale's child reads that locale: `zh-TW` reads `zh-Hant` and `es-JP` reads `es-419`. The data is in each locale's default numbering system.

### cldr_version

* `pub fn cldr_version() -> &'static str`: the CLDR release the data was generated from, `"48.2.1"`.

## 2. Signs

### Sign

Whether the number is written with a sign. `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]`.

* `Sign::None` (default): no sign.
* `Sign::Minus`: the locale's minus sign.
* `Sign::Plus`: the locale's plus sign.

A pattern with a negative subpattern writes both signs with it. A pattern without one writes the sign before the positive pattern.

## 3. Currency

### Display

What stands for the currency, as `currencyDisplay` chooses. `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]`.

* `Display::Code` (default): the ISO 4217 code.
* `Display::Symbol`: the locale's symbol, else the code.
* `Display::NarrowSymbol`: the locale's narrow symbol, else its symbol, else the code.

### Currency

One currency in one locale. `#[derive(Debug, Clone)]`.

* `pub fn new(tag: &str, code: &str, display: Display) -> Result<Currency, Error>`: `code` is three ASCII letters in any case, else `Error::NotACurrency`. A code CLDR does not know is written as itself with 2 fraction digits.
* `pub fn code(&self) -> &str`: the code, upper case.
* `pub fn shown(&self) -> &str`: the code or symbol the amount is written with.
* `pub fn digits(&self) -> u8`: the currency's fraction digits.
* `pub fn format(&self, digits: &str, sign: Sign) -> String`: `digits` is the absolute amount already written in the locale's digits and separators. The locale's alpha-next-to-number pattern applies when the shown text has a letter on the side that touches the number.

## 4. Percent

### Percent

The percent pattern of one locale. `#[derive(Debug, Clone)]`.

* `pub fn new(tag: &str) -> Percent`
* `pub fn format(&self, digits: &str, sign: Sign) -> String`: `digits` is the absolute value already multiplied by 100 and written in the locale's digits and separators.

## 5. Relative time

### Unit

`#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`: `Second`, `Minute`, `Hour`, `Day`, `Week`, `Month`, `Quarter`, `Year`.

* `pub fn from_name(name: &str) -> Option<Unit>`: the singular or plural English name, `day` or `days`.

### Width

`#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]`: `Long` (default), `Short`, `Narrow`.

### Plural

A CLDR plural category. `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]`: `Zero`, `One`, `Two`, `Few`, `Many`, `Other`.

### Relative

The wording of one unit at one width in one locale. `#[derive(Debug, Clone, Copy)]`.

* `pub fn new(tag: &str, unit: Unit, width: Width) -> Relative`
* `pub fn literal(&self, offset: i8) -> Option<&'static str>`: the word for `offset` units from now, as `numeric: "auto"` writes it. CLDR has words for at most -2 to 2.
* `pub fn format(&self, digits: &str, past: bool, plural: Plural, exact: Option<u8>) -> String`: `digits` is the absolute value written in the locale's digits and separators. `exact` is `Some(0)` or `Some(1)` when the value is exactly that, which selects a locale's `=0` or `=1` pattern where it has one. A category the locale has no pattern for uses `Other`.

## 6. Case

Only the tag's language subtag is read. `tr` and `az` take the Turkish rules, `lt` the Lithuanian and `el` the Greek. Every other tag maps as Unicode's default does. So does a tag that is not well formed.

### upper_case

* `pub fn upper_case(text: &str, tag: &str) -> String`: `text.toLocaleUpperCase(tag)`. Turkish and Azerbaijani write `i` as `İ`. Lithuanian drops a combining dot above after a soft-dotted letter. Greek drops accents and breathings, adds a dialytika to an iota or upsilon after an accented vowel and keeps the tonos of a lone `ή`, as ICU does.

### lower_case

* `pub fn lower_case(text: &str, tag: &str) -> String`: `text.toLocaleLowerCase(tag)`. Turkish and Azerbaijani write `I` as `ı` and `İ` as `i`. Lithuanian keeps a dot above `i` and `j` under a following accent.

### unicode_version

* `pub fn unicode_version() -> &'static str`: the Unicode release the case data was generated from, `"17.0.0"`.

## 7. Error handling

### Error

`#[derive(Debug, Clone, PartialEq, Eq)]`, `#[non_exhaustive]`, implements `std::error::Error`.

* `Error::NotACurrency(String)`: a currency code that is not three ASCII letters. Displays as `` `dollars` is not a currency code ``.
