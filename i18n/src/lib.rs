//! Currency, percent and relative-time formatting from CLDR data, as `Intl.NumberFormat` and `Intl.RelativeTimeFormat` write them. The caller writes the digits in the locale's numbering system and this crate places the currency, the percent sign, the sign and the words around them.
//!
//! [`zone_name`] names a time zone as `Date.prototype.toString` does, from CLDR's metazone names.
//!
//! [`upper_case`] and [`lower_case`] map case under a locale as `toLocaleUpperCase` and `toLocaleLowerCase` do, from Unicode Character Database data.

mod case;
mod currency;
mod data;
mod locale;
mod pattern;
mod percent;
mod relative;
mod zone;

pub use case::{lower_case, unicode_version, upper_case};
pub use currency::{Currency, Display};
pub use pattern::Sign;
pub use percent::Percent;
pub use relative::{Plural, Relative, Unit, Width};
pub use zone::{in_daylight, zone_name, ZoneRules};

/// The CLDR release the embedded data was generated from.
pub fn cldr_version() -> &'static str {
  data::data().version
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
  /// A currency code that is not three ASCII letters.
  NotACurrency(String),
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Error::NotACurrency(code) => write!(f, "`{code}` is not a currency code"),
    }
  }
}

impl std::error::Error for Error {}
