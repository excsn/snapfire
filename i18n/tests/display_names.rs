//! Each row is a locale, a type, a code, a style and a language display, then what node v26.8.1 (ICU 78.3, CLDR 48) gives for `new Intl.DisplayNames(locale, { type, style, languageDisplay }).of(code)`.

use snapfire_i18n::{display_name, NameKind};

const NODE: &[(&str, &str, &str, &str, &str, &str)] = &[
  ("en", "region", "GB", "long", "dialect", "United Kingdom"),
  ("en", "region", "GB", "short", "dialect", "UK"),
  ("en", "language", "en-US", "long", "dialect", "American English"),
  ("en", "language", "en-US", "long", "standard", "English (United States)"),
  ("en", "language", "fr-CA", "long", "dialect", "Canadian French"),
  ("en", "language", "sr-Latn-RS", "long", "dialect", "Serbian (Latin, Serbia)"),
  ("en", "currency", "EUR", "long", "dialect", "Euro"),
  ("en", "calendar", "gregory", "long", "dialect", "Gregorian Calendar"),
  ("en", "calendar", "ethioaa", "long", "dialect", "Ethiopic Amete Alem Calendar"),
  ("en", "dateTimeField", "year", "long", "dialect", "year"),
  ("en", "dateTimeField", "weekOfYear", "long", "dialect", "week"),
  ("en", "dateTimeField", "dayPeriod", "long", "dialect", "AM/PM"),
  ("en", "dateTimeField", "month", "short", "dialect", "mo."),
  ("de", "script", "Latn", "long", "dialect", "Lateinisch"),
  ("ja", "language", "zh-Hant", "long", "dialect", "繁体中国語"),
  ("en", "language", "zh-Hans-CN", "long", "dialect", "Chinese (Simplified, China)"),
  ("fr", "region", "419", "long", "dialect", "Amérique latine"),
  ("en", "language", "en-GB", "short", "dialect", "UK English"),
  ("ar", "region", "EG", "long", "dialect", "مصر"),
  ("de-AT", "region", "DE", "long", "dialect", "Deutschland"),
  ("zh-TW", "language", "en", "long", "dialect", "英文"),
  ("pt-BR", "currency", "USD", "long", "dialect", "Dólar americano"),
];

#[test]
fn a_code_is_named_as_node_names_it() {
  let mut failed = Vec::new();
  for &(tag, kind, code, style, display, want) in NODE {
    let got = display_name(tag, NameKind::from_type(kind).unwrap(), code, style, display == "standard");
    if got.as_deref() != Some(want) {
      failed.push(format!("{tag} {kind} {code} {style}: got {got:?}, want {want}"));
    }
  }
  assert!(failed.is_empty(), "{}", failed.join("\n"));
}
