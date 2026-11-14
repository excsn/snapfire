//! Currency amounts: the locale's currency pattern with the code or a symbol beside the caller's digits.

use crate::Error;
use crate::data::{fields, Data};
use crate::pattern::{Pattern, Sign, Symbols};

/// What stands for the currency, as `Intl.NumberFormat`'s `currencyDisplay` chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Display {
  /// The ISO 4217 code: `USD`.
  #[default]
  Code,
  /// The locale's symbol, else the code: `$`, `US$`.
  Symbol,
  /// The locale's narrow symbol, else its symbol, else the code: `$`.
  NarrowSymbol,
}

/// One currency in one locale, made once and used for any number of amounts.
#[derive(Debug, Clone)]
pub struct Currency {
  code: String,
  shown: String,
  digits: u8,
  pattern: Pattern,
  minus: &'static str,
  plus: &'static str,
}

impl Data {
  /// The currency `code`, an ISO 4217 code of three ASCII letters in any case, in the locale `tag`. A code CLDR does not know is written as itself with two fraction digits.
  pub fn currency(&self, tag: &str, code: &str, display: Display) -> Result<Currency, Error> {
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_alphabetic()) {
      return Err(Error::NotACurrency(code.to_owned()));
    }
    let code = code.to_ascii_uppercase();
    let locale = self.resolve(tag);
    let numbers = self.numbers(locale.numbers);
    let known = self.get(&["cS", locale.symbols, &code]).map(|symbol| {
      let [symbol, symbol_letters, narrow, narrow_letters] = fields(symbol);
      let with = |text: &'static str, letters: &str| (!text.is_empty()).then(|| (text, letters.parse().unwrap_or(0)));
      (with(symbol, symbol_letters), with(narrow, narrow_letters))
    });
    let (shown, letters) = match (display, known) {
      (Display::Code, _) | (_, None) => (None, 0b11),
      (Display::Symbol, Some((symbol, _))) => symbol.map_or((None, 0b11), |(s, l): (&'static str, u8)| (Some(s), l)),
      (Display::NarrowSymbol, Some((symbol, narrow))) => narrow.or(symbol).map_or((None, 0b11), |(s, l): (&'static str, u8)| (Some(s), l)),
    };
    let standard = Pattern::parse(numbers.currency);
    let alpha = match standard.currency_touches_number() {
      Some(true) => letters & 0b01 != 0,
      Some(false) => letters & 0b10 != 0,
      None => false,
    };
    let pattern = match (alpha, numbers.currency_alpha) {
      (true, Some(alpha)) => Pattern::parse(alpha),
      _ => standard,
    };
    let digits = self.get(&["cF", &code]).or_else(|| self.get(&["cF", "*"])).and_then(|digits| digits.parse().ok()).unwrap_or(2);
    let shown = shown.map_or_else(|| code.clone(), str::to_owned);
    Ok(Currency { code, shown, digits, pattern, minus: numbers.minus, plus: numbers.plus })
  }
}

impl Currency {
  /// [`Data::currency`] from the embedded data.
  #[cfg(feature = "embedded")]
  pub fn new(tag: &str, code: &str, display: Display) -> Result<Currency, Error> {
    Data::embedded().currency(tag, code, display)
  }

  /// The ISO 4217 code, upper case.
  pub fn code(&self) -> &str {
    &self.code
  }

  /// What the amount is written with: the code or the symbol `Display` chose.
  pub fn shown(&self) -> &str {
    &self.shown
  }

  /// The currency's fraction digits, which `Intl.NumberFormat` uses for both the minimum and the maximum unless told otherwise: 2 for `USD`, 0 for `JPY`, 3 for `KWD`.
  pub fn digits(&self) -> u8 {
    self.digits
  }

  /// `digits`, the absolute amount already written in the locale's digits and separators, with the currency and `sign` placed as the locale's pattern places them.
  pub fn format(&self, digits: &str, sign: Sign) -> String {
    self.pattern.write(digits, sign, &Symbols { currency: &self.shown, percent: "%", minus: self.minus, plus: self.plus })
  }
}
