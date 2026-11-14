//! Relative time: "in 3 days", "2 hr. ago", "yesterday".

use crate::data::{fields, Data};

/// The unit of a relative time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
  Second,
  Minute,
  Hour,
  Day,
  Week,
  Month,
  Quarter,
  Year,
}

impl Unit {
  /// The unit `Intl.RelativeTimeFormat` names `name`, singular or plural: `day` or `days`.
  pub fn from_name(name: &str) -> Option<Unit> {
    Some(match name.strip_suffix('s').unwrap_or(name) {
      "second" => Unit::Second,
      "minute" => Unit::Minute,
      "hour" => Unit::Hour,
      "day" => Unit::Day,
      "week" => Unit::Week,
      "month" => Unit::Month,
      "quarter" => Unit::Quarter,
      "year" => Unit::Year,
      _ => return None,
    })
  }

  /// The CLDR date field of the unit at `width`.
  fn field(self, width: Width) -> &'static str {
    let names = match self {
      Unit::Second => ["second", "second-short", "second-narrow"],
      Unit::Minute => ["minute", "minute-short", "minute-narrow"],
      Unit::Hour => ["hour", "hour-short", "hour-narrow"],
      Unit::Day => ["day", "day-short", "day-narrow"],
      Unit::Week => ["week", "week-short", "week-narrow"],
      Unit::Month => ["month", "month-short", "month-narrow"],
      Unit::Quarter => ["quarter", "quarter-short", "quarter-narrow"],
      Unit::Year => ["year", "year-short", "year-narrow"],
    };
    names[width as usize]
  }
}

/// How long the wording is, as `Intl.RelativeTimeFormat`'s `style` chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Width {
  /// "in 3 months"
  #[default]
  Long,
  /// "in 3 mo."
  Short,
  /// "in 3mo"
  Narrow,
}

/// A CLDR plural category, as the locale's plural rules choose one for the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plural {
  Zero,
  One,
  Two,
  Few,
  Many,
  Other,
}

impl Plural {
  fn key(self) -> &'static str {
    match self {
      Plural::Zero => "zero",
      Plural::One => "one",
      Plural::Two => "two",
      Plural::Few => "few",
      Plural::Many => "many",
      Plural::Other => "other",
    }
  }
}

/// The wording of one unit at one width in one locale.
#[derive(Debug, Clone, Copy)]
pub struct Relative {
  literals: &'static str,
  future: &'static str,
  past: &'static str,
}

/// The pairs of a field: `name US text` joined by RS.
fn pairs(text: &'static str) -> impl Iterator<Item = (&'static str, &'static str)> {
  let mut rest = text;
  std::iter::from_fn(move || {
    while !rest.is_empty() {
      let end = rest.as_bytes().iter().position(|byte| *byte == 0x1e).unwrap_or(rest.len());
      let pair = &rest[..end];
      rest = rest.get(end + 1..).unwrap_or("");
      if let Some(at) = pair.as_bytes().iter().position(|byte| *byte == 0x1f) {
        return Some((&pair[..at], &pair[at + 1..]));
      }
    }
    None
  })
}

impl Data {
  /// The wording of `unit` at `width` in the locale `tag`.
  pub fn relative(&self, tag: &str, unit: Unit, width: Width) -> Relative {
    let id = self.get(&["cR", self.resolve(tag).relative, unit.field(width)]).unwrap_or("");
    let [literals, future, past] = fields(self.get(&["cr", id]).unwrap_or(""));
    Relative { literals, future, past }
  }
}

impl Relative {
  /// [`Data::relative`] from the embedded data.
  #[cfg(feature = "embedded")]
  pub fn new(tag: &str, unit: Unit, width: Width) -> Relative {
    Data::embedded().relative(tag, unit, width)
  }

  /// The word the locale has for `offset` units from now, which `numeric: "auto"` writes in place of the number: -1 day is "yesterday", 0 years is "this year". Most units have words for -1, 0 and 1 and some for -2 and 2.
  pub fn literal(&self, offset: i8) -> Option<&'static str> {
    pairs(self.literals).find(|(at, _)| at.parse() == Ok(offset)).map(|(_, text)| text)
  }

  /// `digits`, the absolute value already written in the locale's digits and separators, in the past or future pattern for `plural`. `exact` is the value when it is exactly 0 or 1, for a locale that words those apart from their category.
  pub fn format(&self, digits: &str, past: bool, plural: Plural, exact: Option<u8>) -> String {
    let forms = match past {
      true => self.past,
      false => self.future,
    };
    let explicit = match exact {
      Some(0) => Some("=0"),
      Some(1) => Some("=1"),
      _ => None,
    };
    let pick = |key: &str| pairs(forms).find(|(form, _)| *form == key).map(|(_, pattern)| pattern);
    let pattern = explicit.and_then(pick).or_else(|| pick(plural.key())).or_else(|| pick("other")).unwrap_or("{0}");
    pattern.replacen("{0}", digits, 1)
  }
}
