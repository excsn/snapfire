//! Each expected string is what Chrome's `Intl` writes for the same call.

use snapfire_i18n::{Currency, Display, Error, Percent, Plural, Relative, Sign, Unit, Width};

#[test]
fn a_currency_takes_the_locales_pattern_and_symbol() {
  let usd = Currency::new("en-US", "USD", Display::Code).unwrap();
  assert_eq!(usd.format("1,234.50", Sign::None), "USD\u{a0}1,234.50");
  assert_eq!(Currency::new("en-US", "USD", Display::Symbol).unwrap().format("1,234.50", Sign::Minus), "-$1,234.50");
  assert_eq!(Currency::new("de-DE", "EUR", Display::Symbol).unwrap().format("1.234,50", Sign::None), "1.234,50\u{a0}€");
  assert_eq!(Currency::new("pt-BR", "BRL", Display::Symbol).unwrap().format("1.234,50", Sign::Minus), "-R$\u{a0}1.234,50");
  assert_eq!(Currency::new("es-ES", "JPY", Display::Symbol).unwrap().format("1.234.568", Sign::None), "1.234.568\u{a0}JPY");
  assert_eq!(Currency::new("ar-EG", "CHF", Display::Code).unwrap().format("١٬٢٣٥", Sign::Minus), "\u{61c}-\u{200f}١٬٢٣٥\u{a0}CHF");
}

#[test]
fn a_narrow_symbol_falls_back_to_the_symbol_then_the_code() {
  assert_eq!(Currency::new("ar-EG", "KWD", Display::NarrowSymbol).unwrap().shown(), "د.ك.\u{200f}");
  assert_eq!(Currency::new("es-ES", "EGP", Display::NarrowSymbol).unwrap().shown(), "EGP");
  assert_eq!(Currency::new("en-US", "USD", Display::NarrowSymbol).unwrap().shown(), "$");
}

#[test]
fn a_currency_knows_its_own_fraction_digits() {
  assert_eq!(Currency::new("en-US", "usd", Display::Code).unwrap().digits(), 2);
  assert_eq!(Currency::new("en-US", "JPY", Display::Code).unwrap().digits(), 0);
  assert_eq!(Currency::new("en-US", "KWD", Display::Code).unwrap().digits(), 3);
  assert_eq!(Currency::new("en-US", "XYZ", Display::Symbol).unwrap().shown(), "XYZ");
  assert_eq!(Currency::new("en-US", "dollars", Display::Code).unwrap_err(), Error::NotACurrency("dollars".to_owned()));
}

#[test]
fn a_percent_takes_the_locales_pattern_and_signs() {
  assert_eq!(Percent::new("en-US").format("26", Sign::None), "26%");
  assert_eq!(Percent::new("fr-FR").format("4,2", Sign::Minus), "-4,2\u{a0}%");
  assert_eq!(Percent::new("ar-EG").format("٢٦", Sign::None), "٢٦٪\u{61c}");
  assert_eq!(Percent::new("ar-EG").format("٤٫٢", Sign::Minus), "\u{61c}-٤٫٢٪\u{61c}");
}

#[test]
fn relative_time_picks_the_pattern_for_the_plural_and_the_word_for_auto() {
  let day = Relative::new("en-US", Unit::Day, Width::Long);
  assert_eq!(day.format("1", true, Plural::One, Some(1)), "1 day ago");
  assert_eq!(day.format("3", false, Plural::Other, None), "in 3 days");
  assert_eq!(day.literal(-1), Some("yesterday"));
  assert_eq!(day.literal(3), None);
  assert_eq!(Relative::new("en-US", Unit::Month, Width::Narrow).format("3", true, Plural::Other, None), "3mo ago");
  assert_eq!(Relative::new("ar-EG", Unit::Day, Width::Long).format("١", true, Plural::One, Some(1)), "قبل يوم واحد");
  assert_eq!(Unit::from_name("weeks"), Some(Unit::Week));
  assert_eq!(Unit::from_name("fortnight"), None);
}

#[test]
fn a_tag_falls_back_through_its_parents() {
  assert_eq!(Currency::new("zh-TW", "USD", Display::Symbol).unwrap().shown(), "US$");
  assert_eq!(snapfire_i18n::cldr_version(), "48.2.1");
}
