//! Currency, percent and relative-time formatting from CLDR data, as `Intl.NumberFormat` and `Intl.RelativeTimeFormat` write them. The caller writes the digits in the locale's numbering system and this crate places the currency, the percent sign, the sign and the words around them.
//!
//! [`zone_name`] names a time zone as `Date.prototype.toString` does, from CLDR's metazone names. [`numeric_date`] and [`date_with_time`] write a date with no options as `toLocaleDateString` and `toLocaleString` do.
//!
//! [`upper_case`] and [`lower_case`] map case under a locale as `toLocaleUpperCase` and `toLocaleLowerCase` do, from Unicode Character Database data.

mod case;
mod currency;
mod data;
#[cfg(feature = "embedded")]
mod embedded;
mod locale;
mod names;
mod pattern;
mod percent;
mod relative;
mod table;
mod zone;

pub use case::{lower_case, unicode_version, upper_case};
pub use data::Data;
#[cfg(feature = "embedded")]
pub use embedded::{calendar, cldr_version, clock, date_with_time, display_name, duration_unit, has_display_names, hour_cycle, in_daylight, numbering_system, numeric_date, numeric_date_parts, preferred_calendars, region_zones, short_zone_name, supported, time_separator, time_widths, zone_name};
pub use currency::{Currency, Display};
pub use pattern::Sign;
pub use percent::Percent;
pub use relative::{Plural, Relative, Unit, Width};
pub use names::NameKind;
pub use zone::{Calendar, ZoneRules};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
  /// A currency code that is not three ASCII letters.
  NotACurrency(String),
  /// Bytes that are not this crate's data in the format this version reads.
  NotData,
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Error::NotACurrency(code) => write!(f, "`{code}` is not a currency code"),
      Error::NotData => write!(f, "the bytes are not snapfire_i18n data this version reads"),
    }
  }
}

impl std::error::Error for Error {}
