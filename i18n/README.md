# snapfire_i18n

[![Crates.io](https://img.shields.io/crates/v/snapfire_i18n.svg)](https://crates.io/crates/snapfire_i18n)
[![Docs.rs](https://docs.rs/snapfire_i18n/badge.svg)](https://docs.rs/snapfire_i18n)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

**snapfire_i18n** writes currency amounts, percentages and relative times the way `Intl.NumberFormat` and `Intl.RelativeTimeFormat` write them, from CLDR 48.2.1 data embedded in the crate. You write the digits in the locale's numbering system with whatever number formatter you use. It places the currency, the percent sign, the sign and the words around them.

It also maps case under a locale as `toLocaleUpperCase` and `toLocaleLowerCase` do, from Unicode 17 data: the Turkish dotted and dotless i, Lithuanian's dot above and Greek uppercase without accents.

It names a time zone as `Date.prototype.toString` does, `Central European Summer Time` or `heure normale d’Europe centrale`, from CLDR's metazone names. You supply the zone rules through the `ZoneRules` trait.

It has no dependencies.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_i18n = "0"
```

`embedded`, on by default, builds every locale's data into the crate, about 4.4 MB. The crate's own functions read it. Without it a program reads the bytes it holds through `Data::from_static`.

`display_names`, on by default, embeds the names `Intl.DisplayNames` writes, about 8 MB. Without it every display name is `None`.

The data is read in place. Nothing is parsed or held on the heap when a program starts or formats its first value.

## What to reach for

| Problem | Piece |
|---|---|
| An amount with its currency code or symbol: `USD 1,234.50`, `1.234,50 €` | `Currency` |
| A currency's own fraction digits: 2 for `USD`, 0 for `JPY` | `Currency::digits` |
| A percentage: `26%`, `26 %`, `٢٦٪؜` | `Percent` |
| "in 3 days", "2 hr. ago" | `Relative::format` |
| "yesterday", "next week" | `Relative::literal` |
| Which CLDR release the data is | `cldr_version` |
| `"istanbul"` as `"İSTANBUL"` in Turkish, `"άλφα"` as `"ΑΛΦΑ"` in Greek | `upper_case` |
| `"İSTANBUL"` as `"istanbul"` in Turkish | `lower_case` |
| Which Unicode release the case data is | `unicode_version` |
| `Pacific Daylight Time` for a zone and an instant | `in_daylight`, `zone_name` |
| `9/10/2024`, `7:37:20 PM` and `9/10/2024, 7:37:20 PM` with no options, in the Gregorian, Persian or Buddhist calendar | `numeric_date`, `clock`, `date_with_time` |
| `United Kingdom`, `American English`, `Euro` | `display_name`, with the `display_names` feature |
| The data of a few locales in a file, 155 KB for `en` alone | `Data::trimmed`, `Data::from_static` |

## License

The crate's code is MPL-2.0, in [LICENSE](LICENSE). The CLDR data in `data/cldr.txt`, `data/zones.txt` and `data/names.txt` and the Unicode Character Database data in `data/ucd.txt` are © Unicode, Inc. under the Unicode License v3, in [data/LICENSE-UNICODE](data/LICENSE-UNICODE).
