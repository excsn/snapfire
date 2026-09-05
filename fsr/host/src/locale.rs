//! The request's locale: resolved once per request from the path prefix,
//! the cookie and `Accept-Language` in the configured order, with the prefix
//! stripped before anything matches the path.

use serde::Deserialize;
use snapfire_fsr_runtime::Locale;

/// The `[locales]` section. Tags are the application's own spelling and
/// stay that way everywhere the application sees them.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalesSection {
  pub supported: Vec<String>,
  /// The default locale, served unprefixed. The first supported one when absent.
  #[serde(default)]
  pub default: Option<String>,
  /// Sources in the order consulted: `prefix`, `cookie`, `header`, any subset.
  #[serde(default = "default_order")]
  pub order: Vec<String>,
  /// Whether a locale chosen by a path prefix is written to the cookie.
  #[serde(default)]
  pub remember: bool,
  #[serde(default = "default_cookie")]
  pub cookie: String,
}

fn default_order() -> Vec<String> {
  vec!["prefix".to_owned(), "cookie".to_owned(), "header".to_owned()]
}

fn default_cookie() -> String {
  "sf_locale".to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
  Prefix,
  Cookie,
  Header,
}

/// What the host holds once the section is checked. Without a section there
/// is one locale, `en`, and no source is consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locales {
  pub supported: Vec<String>,
  pub default: String,
  pub order: Vec<Source>,
  pub remember: bool,
  pub cookie: String,
}

/// One request's locale and what it did to the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
  pub locale: Locale,
  /// The path without its locale prefix.
  pub path: String,
  /// Whether the path carried a prefix.
  pub prefixed: bool,
  /// The cookie to set: a locale the prefix chose that the cookie does not
  /// hold, when `remember` is on.
  pub set_cookie: Option<String>,
}

fn normalise(tag: &str) -> String {
  tag.trim().to_ascii_lowercase().replace('-', "_")
}

fn language_of(tag: &str) -> String {
  normalise(tag).split('_').next().unwrap_or_default().to_owned()
}

fn valid_tag(tag: &str) -> bool {
  !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Locales {
  pub fn single() -> Self {
    Self { supported: vec!["en".to_owned()], default: "en".to_owned(), order: Vec::new(), remember: false, cookie: default_cookie() }
  }

  pub fn from_section(section: &LocalesSection) -> Result<Self, String> {
    if section.supported.is_empty() {
      return Err("locales.supported must name at least one locale".to_owned());
    }
    for tag in &section.supported {
      if !valid_tag(tag) {
        return Err(format!("locales.supported: `{tag}` is not a locale tag; letters, digits, `_` and `-` only"));
      }
    }
    let default = section.default.clone().unwrap_or_else(|| section.supported[0].clone());
    if !section.supported.iter().any(|t| normalise(t) == normalise(&default)) {
      return Err(format!("locales.default `{default}` is not among locales.supported"));
    }
    let mut order = Vec::new();
    for name in &section.order {
      let source = match name.as_str() {
        "prefix" => Source::Prefix,
        "cookie" => Source::Cookie,
        "header" => Source::Header,
        other => return Err(format!("locales.order: `{other}` is not a source; the sources are prefix, cookie and header")),
      };
      if !order.contains(&source) {
        order.push(source);
      }
    }
    if section.cookie.is_empty() {
      return Err("locales.cookie must not be empty".to_owned());
    }
    Ok(Self { supported: section.supported.clone(), default, order, remember: section.remember, cookie: section.cookie.clone() })
  }

  pub fn is_default(&self, tag: &str) -> bool {
    normalise(tag) == normalise(&self.default)
  }

  pub fn locale(&self, tag: &str) -> Locale {
    Locale::new(tag, self.is_default(tag))
  }

  pub fn default_locale(&self) -> Locale {
    Locale::new(self.default.clone(), true)
  }

  /// The supported locale `tag` spells, whatever its case or separator.
  pub fn find(&self, tag: &str) -> Option<&str> {
    let wanted = normalise(tag);
    self.supported.iter().find(|t| normalise(t) == wanted).map(String::as_str)
  }

  /// The supported locale nearest to `tag`: the same spelling, else the
  /// first with the same language.
  pub fn nearest(&self, tag: &str) -> Option<&str> {
    if let Some(found) = self.find(tag) {
      return Some(found);
    }
    let language = language_of(tag);
    if language.is_empty() {
      return None;
    }
    self.supported.iter().find(|t| language_of(t) == language).map(String::as_str)
  }

  /// The locale prefix on `path` and the rest of the path, `/` at least.
  /// None when the path carries no supported prefix or the prefix is not a
  /// source.
  pub fn split_prefix<'p>(&self, path: &'p str) -> Option<(&str, &'p str)> {
    if !self.order.contains(&Source::Prefix) {
      return None;
    }
    let rest = path.strip_prefix('/')?;
    let head = rest.split('/').next().unwrap_or(rest);
    if head.is_empty() {
      return None;
    }
    let locale = self.find(head)?;
    let tail = &path[1 + head.len()..];
    Some((locale, if tail.is_empty() { "/" } else { tail }))
  }

  /// The locale a request's `Accept-Language` asks for, nearest first by
  /// weight, or none the application supports.
  pub fn from_accept_language(&self, header: &str) -> Option<&str> {
    let mut asked: Vec<(f32, &str)> = header
      .split(',')
      .filter_map(|part| {
        let mut pieces = part.split(';');
        let tag = pieces.next()?.trim();
        if tag.is_empty() || tag == "*" {
          return None;
        }
        let weight = pieces
          .find_map(|p| p.trim().strip_prefix("q=").and_then(|q| q.trim().parse::<f32>().ok()))
          .unwrap_or(1.0);
        Some((weight, tag))
      })
      .collect();
    asked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    asked.iter().find_map(|(_, tag)| self.nearest(tag))
  }

  /// The locale the cookie holds, when it names a supported one.
  pub fn from_cookie(&self, header: &str) -> Option<&str> {
    header.split(';').find_map(|pair| {
      let (name, value) = pair.trim().split_once('=')?;
      (name.trim() == self.cookie).then(|| self.find(value.trim())).flatten()
    })
  }

  /// One request's locale from `path`, the `Cookie` header and the
  /// `Accept-Language` header, the sources consulted in the configured
  /// order and the first that answers winning.
  pub fn resolve(&self, path: &str, cookie: Option<&str>, accept_language: Option<&str>) -> Resolution {
    let prefix = self.split_prefix(path);
    let stripped = prefix.map(|(_, rest)| rest).unwrap_or(path).to_owned();
    let held = cookie.and_then(|c| self.from_cookie(c));
    let mut chosen = None;
    for source in &self.order {
      chosen = match source {
        Source::Prefix => prefix.map(|(locale, _)| locale),
        Source::Cookie => held,
        Source::Header => accept_language.and_then(|h| self.from_accept_language(h)),
      };
      if chosen.is_some() {
        break;
      }
    }
    let tag = chosen.unwrap_or(self.default.as_str());
    let set_cookie = match prefix {
      Some((chosen_by_prefix, _)) if self.remember && chosen_by_prefix == tag && held != Some(tag) => {
        Some(format!("{}={}; Path=/; Max-Age=31536000; SameSite=Lax", self.cookie, tag))
      }
      _ => None,
    };
    Resolution { locale: self.locale(tag), path: stripped, prefixed: prefix.is_some(), set_cookie }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn locales() -> Locales {
    Locales::from_section(&LocalesSection {
      supported: vec!["en_US".to_owned(), "fr_FR".to_owned(), "de".to_owned()],
      default: None,
      order: default_order(),
      remember: true,
      cookie: default_cookie(),
    })
    .unwrap()
  }

  #[test]
  fn a_prefix_matches_whatever_its_case_or_separator_and_is_stripped() {
    let l = locales();
    assert_eq!(l.split_prefix("/fr_FR/about"), Some(("fr_FR", "/about")));
    assert_eq!(l.split_prefix("/fr-fr/about"), Some(("fr_FR", "/about")));
    assert_eq!(l.split_prefix("/FR_fr"), Some(("fr_FR", "/")));
    assert_eq!(l.split_prefix("/fr_FR/"), Some(("fr_FR", "/")));
    assert_eq!(l.split_prefix("/about"), None);
    assert_eq!(l.split_prefix("/"), None);
  }

  #[test]
  fn the_header_is_matched_by_weight_then_by_language() {
    let l = locales();
    assert_eq!(l.from_accept_language("fr-CA;q=0.9, en-GB"), Some("en_US"));
    assert_eq!(l.from_accept_language("fr-CA, en-GB;q=0.5"), Some("fr_FR"));
    assert_eq!(l.from_accept_language("de-AT"), Some("de"));
    assert_eq!(l.from_accept_language("*"), None);
    assert_eq!(l.from_accept_language("ja, *;q=0.1"), None);
  }

  #[test]
  fn sources_answer_in_order_and_the_prefix_writes_the_cookie_once() {
    let l = locales();
    let r = l.resolve("/fr_FR/x", Some("sf_locale=en_US"), Some("de"));
    assert_eq!(r.locale, Locale::new("fr_FR", false));
    assert_eq!(r.path, "/x");
    assert!(r.prefixed);
    assert_eq!(r.set_cookie.as_deref(), Some("sf_locale=fr_FR; Path=/; Max-Age=31536000; SameSite=Lax"));

    let r = l.resolve("/fr_FR/x", Some("sf_locale=fr_FR"), None);
    assert_eq!(r.set_cookie, None, "the cookie already holds it");

    let r = l.resolve("/x", Some("other=1; sf_locale=de"), Some("fr"));
    assert_eq!(r.locale, Locale::new("de", false));
    assert!(!r.prefixed);

    let r = l.resolve("/x", None, Some("fr-BE"));
    assert_eq!(r.locale, Locale::new("fr_FR", false));

    let r = l.resolve("/x", None, None);
    assert_eq!(r.locale, Locale::new("en_US", true));

    let r = l.resolve("/en_US/x", None, None);
    assert_eq!(r.locale, Locale::new("en_US", true));
    assert!(r.prefixed, "the default locale may be prefixed");
  }

  #[test]
  fn without_a_section_nothing_is_a_prefix() {
    let l = Locales::single();
    let r = l.resolve("/en/x", Some("sf_locale=en"), Some("fr"));
    assert_eq!(r.path, "/en/x");
    assert_eq!(r.locale, Locale::new("en", true));
  }

  #[test]
  fn the_section_is_checked() {
    let bad = |section: LocalesSection| Locales::from_section(&section).unwrap_err();
    let base = LocalesSection { supported: vec!["en".to_owned()], default: None, order: default_order(), remember: false, cookie: default_cookie() };
    assert!(bad(LocalesSection { supported: vec![], ..base.clone() }).contains("at least one"));
    assert!(bad(LocalesSection { default: Some("fr".to_owned()), ..base.clone() }).contains("not among"));
    assert!(bad(LocalesSection { order: vec!["path".to_owned()], ..base.clone() }).contains("not a source"));
    assert!(bad(LocalesSection { supported: vec!["fr FR".to_owned()], ..base.clone() }).contains("not a locale tag"));
    let ok = Locales::from_section(&LocalesSection { order: vec!["header".to_owned(), "header".to_owned()], ..base }).unwrap();
    assert_eq!(ok.order, vec![Source::Header]);
    assert_eq!(ok.default, "en");
  }
}
