//! Locale data read from the bytes a build cut: what it was cut for, which locale a tag formats in and what bytes are refused.

use snapfire_fsr_ir::intl_data::{Cut, IntlData, IntlDataError, COMPILED_ICU};
use snapfire_fsr_ir::locales::Unlisted;

fn packed(locales: &[&str], default: &str, unlisted: Unlisted) -> &'static [u8] {
  let tables = snapfire_i18n::Data::embedded().trimmed(locales);
  let cut = Cut::new(locales.iter().map(|tag| (*tag).to_owned()).collect(), default.to_owned(), unlisted);
  Box::leak(IntlData::packed(COMPILED_ICU, &tables, &cut).into_boxed_slice())
}

fn formats_in(data: &IntlData, tag: &str) -> Result<String, String> {
  data.locale("test", tag).map(|locale| locale.to_string()).map_err(|fail| fail.message)
}

#[test]
fn data_reports_what_it_was_cut_for() {
  let data = IntlData::from_static(packed(&["en-GB", "fr"], "fr", Unlisted::Error)).unwrap();
  assert_eq!(data.cut(), Some(&Cut::new(vec!["en-GB".to_owned(), "fr".to_owned()], "fr".to_owned(), Unlisted::Error)));
}

#[test]
fn what_bytes_were_cut_for_is_read_without_the_data() {
  let tables = snapfire_i18n::Data::embedded().trimmed(&["th"]);
  let cut = Cut::new(vec!["th".to_owned()], "th".to_owned(), Unlisted::Fallback).words(true).source("a build");
  let bytes = IntlData::packed(COMPILED_ICU, &tables, &cut);
  assert_eq!(IntlData::cut_of(&bytes), Ok(cut));
  assert_eq!(IntlData::cut_of(b"SFINTL"), Err(IntlDataError::NotData));
}

#[test]
fn the_data_built_into_the_crate_is_cut_for_nothing() {
  assert_eq!(IntlData::compiled().cut(), None);
  assert_eq!(formats_in(IntlData::compiled(), "sw-KE"), Ok("sw-KE".to_owned()));
  assert!(formats_in(IntlData::compiled(), "not a locale").unwrap_err().contains("is not a locale"));
}

#[test]
fn a_tag_formats_in_the_listed_locale_that_covers_it() {
  let data = IntlData::from_static(packed(&["en-GB", "fr"], "fr", Unlisted::Fallback)).unwrap();
  assert_eq!(formats_in(&data, "en-GB"), Ok("en-GB".to_owned()));
  assert_eq!(formats_in(&data, "fr-CA"), Ok("fr".to_owned()));
  assert_eq!(formats_in(&data, "fr-CA-u-nu-arab"), Ok("fr-u-nu-arab".to_owned()));
}

#[test]
fn an_unlisted_tag_formats_in_the_default_locale_when_the_data_was_cut_to_fall_back() {
  let data = IntlData::from_static(packed(&["en-GB", "fr"], "fr", Unlisted::Fallback)).unwrap();
  assert_eq!(formats_in(&data, "de"), Ok("fr".to_owned()));
  assert_eq!(formats_in(&data, "en-AU"), Ok("fr".to_owned()));
}

#[test]
fn an_unlisted_tag_is_refused_by_name_when_the_data_was_cut_to_refuse() {
  let data = IntlData::from_static(packed(&["en-GB", "fr"], "fr", Unlisted::Error)).unwrap();
  let refusal = formats_in(&data, "de").unwrap_err();
  assert!(refusal.starts_with("RangeError: test: `de` is not one of the application's locales (en-GB, fr)"), "{refusal}");
  assert_eq!(formats_in(&data, "fr-CA"), Ok("fr".to_owned()));
}

#[test]
fn a_malformed_tag_is_refused_whatever_the_data_was_cut_for() {
  for unlisted in [Unlisted::Fallback, Unlisted::Error] {
    let data = IntlData::from_static(packed(&["en"], "en", unlisted)).unwrap();
    assert!(formats_in(&data, "not a locale").unwrap_err().contains("is not a locale"));
  }
}

#[test]
fn bytes_that_are_not_locale_data_are_refused() {
  assert_eq!(IntlData::from_static(b"").unwrap_err(), IntlDataError::NotData);
  assert_eq!(IntlData::from_static(b"SFINTL\x00\x02rest").unwrap_err(), IntlDataError::NotData);
  let whole = packed(&["en"], "en", Unlisted::Fallback);
  assert_eq!(IntlData::from_static(&whole[..whole.len() - 1]).unwrap_err(), IntlDataError::NotData);
}

#[test]
fn data_another_icu_release_wrote_is_refused() {
  let whole = packed(&["en"], "en", Unlisted::Fallback).to_vec();
  let at = whole.windows(8).position(|window| window == b"icu\t2.3\n").unwrap();
  let mut other = whole.clone();
  other[at + 6] = b'9';
  assert_eq!(IntlData::from_static(Box::leak(other.into_boxed_slice())).unwrap_err(), IntlDataError::Icu("2.9".to_owned()));
}

#[test]
fn data_whose_default_is_not_one_of_its_locales_is_refused() {
  let tables = snapfire_i18n::Data::embedded().trimmed(&["en"]);
  let cut = Cut::new(vec!["en".to_owned()], "fr".to_owned(), Unlisted::Fallback);
  let bytes: &'static [u8] = Box::leak(IntlData::packed(COMPILED_ICU, &tables, &cut).into_boxed_slice());
  assert!(matches!(IntlData::from_static(bytes).unwrap_err(), IntlDataError::Locales(_)));
}

fn printed(expr: &snapfire_fsr_ir::Expr) -> String {
  snapfire_fsr_ir::sexpr::print(&[snapfire_fsr_ir::sexpr::expr_to_sx(expr)])
}

fn formatter(name: &str, args: Vec<snapfire_fsr_ir::Expr>) -> snapfire_fsr_ir::Expr {
  snapfire_fsr_ir::Expr::Ext { module: "intl".to_owned(), name: name.to_owned(), args }
}

fn locale_option(tag: snapfire_fsr_ir::Expr) -> snapfire_fsr_ir::Expr {
  snapfire_fsr_ir::Expr::Object(vec![snapfire_fsr_ir::ast::Entry::Field("locale".to_owned(), tag)])
}

#[test]
fn the_locales_a_plan_names_as_literals_are_read_from_it() {
  use snapfire_fsr_ir::intl_data::named_locales;
  use snapfire_fsr_ir::Expr;
  let amount = || Expr::lit_str("1");
  let plan = [
    printed(&formatter("number", vec![amount(), locale_option(Expr::lit_str("de-DE"))])),
    printed(&Expr::Array(vec![snapfire_fsr_ir::ast::Entry::Item(formatter("date", vec![amount(), locale_option(Expr::lit_str("fr"))]))])),
    printed(&formatter("numeric", vec![amount(), Expr::lit_str("ja"), Expr::lit_str("date")])),
    printed(&formatter("number", vec![amount(), locale_option(Expr::lit_str("de-DE"))])),
    printed(&formatter("number", vec![amount(), locale_option(Expr::Var("chosen".to_owned()))])),
    printed(&formatter("upper", vec![amount(), Expr::lit_str("tr")])),
    printed(&locale_option(Expr::lit_str("sv"))),
  ]
  .join("\n");
  assert_eq!(named_locales(&plan), ["de-DE", "fr", "ja"]);
  assert!(named_locales("(not a plan").is_empty());
}

#[test]
fn a_plan_splits_words_unless_every_segmenter_names_another_granularity() {
  use snapfire_fsr_ir::ast::Builtin;
  use snapfire_fsr_ir::intl_data::splits_words;
  use snapfire_fsr_ir::Expr;
  let segments = |granularity: Expr| printed(&Expr::Builtin { name: Builtin::Segments, args: vec![Expr::lit_str("text"), granularity] });
  let wrapped = |name: &str| Expr::Str(Box::new(Expr::lit_str(name)));
  assert!(!splits_words(&printed(&Expr::lit_str("no segmenter"))));
  assert!(!splits_words(&segments(Expr::lit_str("grapheme"))));
  assert!(!splits_words(&segments(wrapped("sentence"))));
  assert!(splits_words(&segments(Expr::lit_str("word"))));
  assert!(splits_words(&segments(wrapped("word"))));
  assert!(splits_words(&segments(Expr::Var("chosen".to_owned()))));
  assert!(splits_words(&format!("{}\n{}", segments(Expr::lit_str("grapheme")), segments(Expr::lit_str("word")))));
  assert!(splits_words("(not a plan"));
}
