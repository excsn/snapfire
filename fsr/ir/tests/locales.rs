//! Which of an application's listed locales a requested tag formats in, as ECMA-402's lookup matcher picks one.

use snapfire_fsr_ir::locales::{Locales, LocalesError, Refused, Unlisted};

fn listed(unlisted: Unlisted) -> Locales {
  Locales::new(&["en", "en-GB", "fr", "zh-Hant"], "fr", unlisted).unwrap()
}

fn resolved(locales: &Locales, tag: &str) -> Result<String, Refused> {
  locales.resolve(tag).map(|locale| locale.to_string())
}

#[test]
fn a_listed_locale_resolves_to_itself() {
  let locales = listed(Unlisted::Fallback);
  assert_eq!(resolved(&locales, "en"), Ok("en".to_owned()));
  assert_eq!(resolved(&locales, "en-GB"), Ok("en-GB".to_owned()));
  assert_eq!(resolved(&locales, "zh-Hant"), Ok("zh-Hant".to_owned()));
}

#[test]
fn a_tag_resolves_whatever_its_case() {
  let locales = listed(Unlisted::Fallback);
  assert_eq!(resolved(&locales, "EN-gb"), Ok("en-GB".to_owned()));
}

#[test]
fn an_unlisted_tag_resolves_to_the_nearest_listed_tag_it_truncates_to() {
  let locales = listed(Unlisted::Fallback);
  assert_eq!(resolved(&locales, "en-AU"), Ok("en".to_owned()));
  assert_eq!(resolved(&locales, "en-GB-oxendict"), Ok("en-GB".to_owned()));
  assert_eq!(resolved(&locales, "fr-Latn-CA"), Ok("fr".to_owned()));
  assert_eq!(resolved(&locales, "zh-Hant-TW"), Ok("zh-Hant".to_owned()));
}

#[test]
fn a_tag_never_resolves_to_a_longer_listed_tag() {
  let locales = listed(Unlisted::Fallback);
  assert_eq!(resolved(&locales, "zh"), Ok("fr".to_owned()));
  assert_eq!(resolved(&locales, "zh-TW"), Ok("fr".to_owned()));
}

#[test]
fn a_tag_no_listed_locale_covers_falls_back_to_the_default() {
  let locales = listed(Unlisted::Fallback);
  assert_eq!(resolved(&locales, "ja"), Ok("fr".to_owned()));
  assert_eq!(resolved(&locales, "de-CH"), Ok("fr".to_owned()));
}

#[test]
fn a_tag_no_listed_locale_covers_is_refused_when_the_application_says_so() {
  let locales = listed(Unlisted::Error);
  assert_eq!(resolved(&locales, "ja"), Err(Refused::Unlisted));
  assert_eq!(resolved(&locales, "zh-TW"), Err(Refused::Unlisted));
  assert_eq!(resolved(&locales, "en-AU"), Ok("en".to_owned()));
  assert_eq!(resolved(&locales, "fr"), Ok("fr".to_owned()));
}

#[test]
fn a_malformed_tag_is_refused_under_either_setting() {
  for unlisted in [Unlisted::Fallback, Unlisted::Error] {
    let locales = listed(unlisted);
    assert_eq!(resolved(&locales, "not a locale"), Err(Refused::Malformed));
    assert_eq!(resolved(&locales, ""), Err(Refused::Malformed));
  }
}

#[test]
fn the_requested_unicode_extension_is_kept_on_the_resolved_locale() {
  let locales = listed(Unlisted::Fallback);
  assert_eq!(resolved(&locales, "en-AU-u-nu-arab"), Ok("en-u-nu-arab".to_owned()));
  assert_eq!(resolved(&locales, "fr-u-ca-buddhist-hc-h23"), Ok("fr-u-ca-buddhist-hc-h23".to_owned()));
  assert_eq!(resolved(&locales, "ja-u-ca-japanese"), Ok("fr-u-ca-japanese".to_owned()));
}

#[test]
fn a_list_is_refused_when_it_is_empty() {
  assert_eq!(Locales::new::<&str>(&[], "en", Unlisted::Fallback).unwrap_err(), LocalesError::Empty);
}

#[test]
fn a_list_is_refused_when_a_tag_in_it_is_malformed() {
  assert_eq!(Locales::new(&["en", "not a locale"], "en", Unlisted::Fallback).unwrap_err(), LocalesError::Malformed("not a locale".to_owned()));
  assert_eq!(Locales::new(&["en"], "??", Unlisted::Fallback).unwrap_err(), LocalesError::Malformed("??".to_owned()));
}

#[test]
fn a_list_is_refused_when_a_tag_in_it_carries_an_extension() {
  assert_eq!(Locales::new(&["en-u-nu-arab"], "en-u-nu-arab", Unlisted::Fallback).unwrap_err(), LocalesError::Extension("en-u-nu-arab".to_owned()));
}

#[test]
fn a_list_is_refused_when_the_default_is_not_in_it() {
  assert_eq!(Locales::new(&["en", "fr"], "de", Unlisted::Fallback).unwrap_err(), LocalesError::DefaultUnlisted("de".to_owned()));
  assert_eq!(Locales::new(&["en-GB"], "en", Unlisted::Fallback).unwrap_err(), LocalesError::DefaultUnlisted("en".to_owned()));
}

#[test]
fn the_default_is_matched_whatever_its_case() {
  let locales = Locales::new(&["en-gb", "FR"], "EN-GB", Unlisted::Fallback).unwrap();
  assert_eq!(resolved(&locales, "ja"), Ok("en-GB".to_owned()));
}
