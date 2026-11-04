//! Percentages: the locale's percent pattern and percent sign beside the caller's digits.

use crate::data::data;
use crate::locale::resolve;
use crate::pattern::{Pattern, Sign, Symbols};

/// The percent pattern of one locale, in its default numbering system.
#[derive(Debug, Clone)]
pub struct Percent {
  pattern: Pattern,
  percent: &'static str,
  minus: &'static str,
  plus: &'static str,
}

impl Percent {
  /// The percent pattern of the locale `tag`.
  pub fn new(tag: &str) -> Percent {
    let numbers = &data().numbers[resolve(tag).numbers];
    Percent { pattern: Pattern::parse(numbers.percent), percent: numbers.percent_sign, minus: numbers.minus, plus: numbers.plus }
  }

  /// `digits`, the absolute value already multiplied by 100 and written in the locale's digits and separators, with the percent sign and `sign` placed as the locale's pattern places them.
  pub fn format(&self, digits: &str, sign: Sign) -> String {
    self.pattern.write(digits, sign, &Symbols { currency: "", percent: self.percent, minus: self.minus, plus: self.plus })
  }
}
