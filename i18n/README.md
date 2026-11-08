# snapfire_i18n

[![Crates.io](https://img.shields.io/crates/v/snapfire_i18n.svg)](https://crates.io/crates/snapfire_i18n)
[![Docs.rs](https://docs.rs/snapfire_i18n/badge.svg)](https://docs.rs/snapfire_i18n)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

**snapfire_i18n** writes currency amounts, percentages and relative times the way `Intl.NumberFormat` and `Intl.RelativeTimeFormat` write them, from CLDR 48.2.1 data embedded in the crate. You write the digits in the locale's numbering system with whatever number formatter you use. It places the currency, the percent sign, the sign and the words around them.

It has no dependencies.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_i18n = "0"
```

No feature flags.

## What to reach for

| Problem | Piece |
|---|---|
| An amount with its currency code or symbol: `USD 1,234.50`, `1.234,50 €` | `Currency` |
| A currency's own fraction digits: 2 for `USD`, 0 for `JPY` | `Currency::digits` |
| A percentage: `26%`, `26 %`, `٢٦٪؜` | `Percent` |
| "in 3 days", "2 hr. ago" | `Relative::format` |
| "yesterday", "next week" | `Relative::literal` |
| Which CLDR release the data is | `cldr_version` |

## License

The crate's code is MPL-2.0, in [LICENSE](LICENSE). The CLDR data in `data/cldr.txt` is © Unicode, Inc. under the Unicode License v3, in [data/LICENSE-UNICODE](data/LICENSE-UNICODE).
