//! Relative time: "in 3 days", "2 hr. ago", "yesterday".

use crate::data::{Field, data};
use crate::locale::resolve;

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

  fn name(self) -> &'static str {
    match self {
      Unit::Second => "second",
      Unit::Minute => "minute",
      Unit::Hour => "hour",
      Unit::Day => "day",
      Unit::Week => "week",
      Unit::Month => "month",
      Unit::Quarter => "quarter",
      Unit::Year => "year",
    }
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
  field: &'static Field,
}

impl Relative {
  /// The wording of `unit` at `width` in the locale `tag`.
  pub fn new(tag: &str, unit: Unit, width: Width) -> Relative {
    let data = data();
    let name = match width {
      Width::Long => unit.name().to_owned(),
      Width::Short => format!("{}-short", unit.name()),
      Width::Narrow => format!("{}-narrow", unit.name()),
    };
    let id = data.relative[resolve(tag).relative][name.as_str()];
    Relative { field: &data.field_lines[id] }
  }

  /// The word the locale has for `offset` units from now, which `numeric: "auto"` writes in place of the number: -1 day is "yesterday", 0 years is "this year". Most units have words for -1, 0 and 1 and some for -2 and 2.
  pub fn literal(&self, offset: i8) -> Option<&'static str> {
    self.field.literals.iter().find(|(at, _)| *at == offset).map(|(_, text)| *text)
  }

  /// `digits`, the absolute value already written in the locale's digits and separators, in the past or future pattern for `plural`. `exact` is the value when it is exactly 0 or 1, for a locale that words those apart from their category.
  pub fn format(&self, digits: &str, past: bool, plural: Plural, exact: Option<u8>) -> String {
    let forms = match past {
      true => &self.field.past,
      false => &self.field.future,
    };
    let explicit = match exact {
      Some(0) => Some("=0"),
      Some(1) => Some("=1"),
      _ => None,
    };
    let pick = |key: &str| forms.iter().find(|(form, _)| *form == key).map(|(_, pattern)| *pattern);
    let pattern = explicit.and_then(pick).or_else(|| pick(plural.key())).or_else(|| pick("other")).unwrap_or("{0}");
    pattern.replacen("{0}", digits, 1)
  }
}
