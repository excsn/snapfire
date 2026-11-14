# API Reference: snapfire_i18n

Currency, percent and relative-time formatting and time zone names from CLDR data, embedded or read from bytes the caller holds. Case mapping under a locale from embedded Unicode data.

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
* [7. Time zone names](#7-time-zone-names)
  * [ZoneRules](#zonerules)
  * [in_daylight](#in_daylight)
  * [zone_name](#zone_name)
  * [short_zone_name](#short_zone_name)
  * [Calendar](#calendar)
  * [calendar](#calendar-1)
  * [numeric_date](#numeric_date)
  * [numeric_date_parts](#numeric_date_parts)
  * [clock](#clock)
  * [date_with_time](#date_with_time)
  * [duration_unit](#duration_unit)
  * [time_separator](#time_separator)
  * [hour_cycle](#hour_cycle)
  * [time_widths](#time_widths)
  * [numbering_system](#numbering_system)
  * [supported](#supported)
  * [preferred_calendars](#preferred_calendars)
  * [region_zones](#region_zones)
* [8. Display names](#8-display-names)
  * [NameKind](#namekind)
  * [display_name](#display_name)
  * [has_display_names](#has_display_names)
* [9. Data](#9-data)
  * [Data](#data)
* [10. Error handling](#10-error-handling)
  * [Error](#error)

## 1. Locales

Every constructor takes a BCP 47 tag as `&str`. `_` separates subtags as `-` does. Anything after the region is ignored. A tag reads the most specific CLDR locale it falls back to: language-script-region, language-region, language-script, language, then the root locale. A tag CLDR lists as another locale's child reads that locale: `zh-TW` reads `zh-Hant` and `es-JP` reads `es-419`. The data is in each locale's default numbering system.

### cldr_version

* `pub fn cldr_version() -> &'static str`: the CLDR release the embedded data was generated from, `"48.2.1"`. Takes the `embedded` feature.

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

## 7. Time zone names

### ZoneRules

What a zone observes, from a time zone database the caller holds. Zones are IANA identifiers.

* `fn observed(&self, zone: &str, ms: f64) -> Option<(i32, i32)>`: the offset from UTC at the instant `ms` and the part of it that is daylight saving, both in seconds. The saving is never negative, as in the rearguard form of the IANA database. `None` for a zone the rules do not know.
* `fn saving_in_year(&self, zone: &str, ms: f64) -> Option<i32>`: the daylight saving observed during the UTC calendar year holding `ms`, in seconds. `None` when there is none that year.

### in_daylight

* `pub fn in_daylight(zone: &str, ms: f64, rules: &impl ZoneRules) -> bool`: whether ICU counts the instant `ms` in `zone` as daylight time. A zone in a metazone at `ms` is in daylight time when its offset is above the standard offset of the metazone's own zone. Any other zone is in daylight time when its saving is not zero.

### zone_name

* `pub fn zone_name(tag: &str, zone: &str, daylight: bool, now: f64, rules: &impl ZoneRules) -> String`: the long specific name of `zone` in `tag`, as ICU's `TimeZone::getDisplayName(daylight, LONG, locale)` writes it. The metazone is the one `zone` is in at the instant `now`. A zone-specific name such as `British Summer Time` comes before the metazone's. A zone with no name is written in the localized GMT format: the standard offset at `now`, plus the year's saving when `daylight` holds, `GMT-05:00`. A zone CLDR does not know is `GMT+00:00`.

### short_zone_name

* `pub fn short_zone_name(locale: &str, zone: &str, ms: f64, rules: &impl ZoneRules) -> String`: the name `Intl.DateTimeFormat` writes under `timeZoneName: "short"` for the zone at the instant `ms`: `EDT` for `America/New_York` in `en-US`, else the short GMT format, `GMT+2` for `Europe/Paris` in `en-US`.

### Calendar

* `pub enum Calendar { Gregorian, Persian, Buddhist }`: a calendar a numeric date is written in.

### calendar

* `pub fn calendar(tag: &str) -> Option<Calendar>`: the calendar the tag's `-u-ca-` keyword names, else the one CLDR's region of the locale prefers: `Persian` for `fa` and `ps`, `Buddhist` for `th`. `None` when the keyword names another calendar.

### numeric_date

* `pub fn numeric_date(tag: &str, year: i64, month: u32, day: u32) -> Option<String>`: a Gregorian date as `toLocaleDateString(tag)` writes it with no options, converted to the locale's `calendar`, in CLDR's pattern for it and the locale's digits: `9/10/2024` in `en`, `10.9.2024` in `de`, `۱۴۰۳/۶/۲۰` in `fa`. `None` when `calendar` is.

### numeric_date_parts

* `pub fn numeric_date_parts(tag: &str, year: i64, month: u32, day: u32) -> Option<Vec<(&'static str, String)>>`: [`numeric_date`](#numeric_date) in the parts `formatToParts` gives, each with its type: `year`, `month`, `day`, `era` or `literal`. `None` when `calendar` is.

### clock

* `pub fn clock(tag: &str, hour: u32, minute: u32, second: u32) -> String`: a time as `toLocaleTimeString(tag)` writes it with no options, on the 12-hour or 24-hour clock CLDR gives the locale's region: `7:37:20 PM` in `en`, `19:37:20` in `de`.

### date_with_time

* `pub fn date_with_time(tag: &str, date: &str, time: &str) -> String`: a date and a time, each already written, joined by the locale's short `atTime` pattern, as `toLocaleString(tag)` joins them with no options: `9/10/2024, 7:37:20 PM`.

### duration_unit

* `pub fn duration_unit(tag: &str, width: &str, unit: &str, plural: &str) -> Option<&'static str>`: the CLDR pattern a duration of `unit` (`year` to `nanosecond`) is written in at `width` (`long`, `short` or `narrow`) for the plural form, `{0} hours`; the `other` form when the locale has none for `plural`.

### time_separator

* `pub fn time_separator(tag: &str) -> &'static str`: the separator between hours, minutes and seconds, `:` in English and `.` in Finnish.

### hour_cycle

* `pub fn hour_cycle(tag: &str) -> &'static str`: the hour cycle `Intl.DateTimeFormat` resolves for the locale, `h12` in `en` and `h23` in `de`. A `-u-hc-` keyword names it.

### time_widths

* `pub fn time_widths(tag: &str, cycle: &str, minute: bool, second: bool) -> [bool; 3]`: whether the hour, the minutes and the seconds are two digits in the pattern the locale writes the hour alone, with `minute` or with `minute` and `second` in on the hour cycle `cycle`, as `resolvedOptions()` reports them: `[false, true, true]` in `en` on `h12` with all three.

### numbering_system

* `pub fn numbering_system(tag: &str) -> &'static str`: the numbering system the locale writes numbers in, `arab` in `ar-EG` and `latn` in a locale whose digits are ASCII.

### supported

* `pub fn supported(tag: &str) -> bool`: whether CLDR holds the locale, by itself or a locale it falls back to other than the root, as `supportedLocalesOf` answers: `true` for `en-US`, `false` for `xx`.

### preferred_calendars

* `pub fn preferred_calendars(region: &str) -> &'static [&'static str]`: the calendars CLDR says the region prefers, most preferred first, by their BCP 47 names, as `Intl.Locale.prototype.getCalendars` lists them: `["gregory", "islamic-umalqura", "islamic", "islamic-rgsa"]` for `SA`. A region with none of its own has the world's, `["gregory"]`.

### region_zones

* `pub fn region_zones(region: &str) -> &'static [&'static str]`: the zones the tz database's `zone.tab` gives the region, by their canonical names and sorted, as `Intl.Locale.prototype.getTimeZones` lists them: `["Europe/Berlin", "Europe/Busingen"]` for `DE`. Empty for a region with none.

## 8. Display names

The names `Intl.DisplayNames` writes, embedded with the `display_names` feature, on by default, which turns `embedded` on.

### NameKind

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`: `Language`, `Region`, `Script`, `Currency`, `Calendar`, `DateTimeField`.

* `pub fn from_type(name: &str) -> Option<NameKind>`: the kind an `Intl.DisplayNames` `type` names.

### display_name

* `pub fn display_name(tag: &str, kind: NameKind, code: &str, style: &str, standard: bool) -> Option<String>`: the name of `code` in `tag`. `style` is `long`, `short` or `narrow`. A short or narrow style takes CLDR's short form where it has one. A language reads as one dialect name when CLDR has its whole tag, `American English`, else as its name with the script and region in the locale's pattern, `Chinese (Simplified, China)`; `standard` always composes it. `None` for a code the data does not name. Without the feature it is `None` for every code.

### has_display_names

* `pub fn has_display_names() -> bool`: whether the build holds the display names.

## 9. Data

### Data

`#[derive(Clone, Copy)]`, implements `Debug`. The CLDR records a call formats from. The bytes are read in place.

* `pub fn embedded() -> Data`: every locale's data, built into the crate. Takes the `embedded` feature. The display names are held with the `display_names` feature.
* `pub fn from_static(bytes: &'static [u8]) -> Result<Data, Error>`: the data `bytes` hold, as `trimmed` wrote them. `Error::NotData` for other bytes.
* `pub fn trimmed(&self, locales: &[&str]) -> Vec<u8>`: the bytes of data holding the records of `locales`, of each locale they fall back to and of the root locale. Records that belong to no locale are kept: currency digits, zone and metazone tables and region tables.
* `pub fn cldr_version(&self) -> &'static str`: the CLDR release the data was generated from.
* `pub fn has_display_names(&self) -> bool`: whether the data holds the display names.
* `pub fn currency(&self, tag: &str, code: &str, display: Display) -> Result<Currency, Error>`: `Currency::new` from this data.
* `pub fn percent(&self, tag: &str) -> Percent`: `Percent::new` from this data.
* `pub fn relative(&self, tag: &str, unit: Unit, width: Width) -> Relative`: `Relative::new` from this data.
* `pub fn preferred_calendars(&self, region: &str) -> Vec<&'static str>` and `pub fn region_zones(&self, region: &str) -> Vec<&'static str>`: the functions of the same names from this data, as vectors.
* `in_daylight`, `zone_name`, `short_zone_name`, `calendar`, `numeric_date`, `numeric_date_parts`, `clock`, `date_with_time`, `duration_unit`, `time_separator`, `hour_cycle`, `time_widths`, `numbering_system`, `supported` and `display_name`: methods with the signatures of the functions of the same names, reading this data.

Every function of sections 3 to 5, 7 and 8 that reads CLDR data reads `Data::embedded()` and takes the `embedded` feature: `Currency::new`, `Percent::new`, `Relative::new` and the free functions.

## 10. Error handling

### Error

`#[derive(Debug, Clone, PartialEq, Eq)]`, `#[non_exhaustive]`, implements `std::error::Error`.

* `Error::NotACurrency(String)`: a currency code that is not three ASCII letters. Displays as `` `dollars` is not a currency code ``.
* `Error::NotData`: bytes that are not this crate's data in the format this version reads.
