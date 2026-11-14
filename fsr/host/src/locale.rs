//! The request's locale: resolved once per request from the path prefix,
//! the cookie and `Accept-Language` in the configured order, with the prefix
//! stripped before anything matches the path.

use serde::Deserialize;
use snapfire_fsr_runtime::Locale;

/// The `[locales]` section. Tags are the application's own spelling and
/// stay that way everywhere the application sees them.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
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
  /// What a formatter gets when code names a locale outside `supported`: `fallback` formats in the default locale, as a browser formats in its own default. `error` refuses with a `RangeError`.
  #[serde(default)]
  pub unlisted: Unlisted,
  /// Whether the locale data holds the dictionaries `Intl.Segmenter` splits words by when the application's own plan does not split words, for a host whose mounted sites do.
  #[serde(default)]
  pub dictionaries: bool,
  /// Locales the application formats in and serves no pages in: the locale data holds them beside `supported`.
  #[serde(default)]
  pub formats: Vec<String>,
}

impl LocalesSection {
  /// `supported` with every other key at the value an absent one takes.
  pub fn new(supported: Vec<String>) -> Self {
    Self { supported, default: None, order: default_order(), remember: false, cookie: default_cookie(), unlisted: Unlisted::default(), dictionaries: false, formats: Vec::new() }
  }
}

/// `[locales] unlisted`.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Unlisted {
  #[default]
  Fallback,
  Error,
}

impl From<Unlisted> for snapfire_fsr_ir::locales::Unlisted {
  fn from(unlisted: Unlisted) -> Self {
    match unlisted {
      Unlisted::Fallback => Self::Fallback,
      Unlisted::Error => Self::Error,
    }
  }
}

/// Where a build writes the locale data and the host reads it, relative to the application.
pub const DATA_FILE: &str = "generated/locale.data";

/// What the locale data of an application is cut for: the locales `section` lists under `supported` and `formats` as BCP 47 tags, `en` alone without one. The word dictionaries are in when `plan` splits words or the section asks for them.
pub fn cut(section: Option<&LocalesSection>, plan: &str) -> snapfire_fsr_ir::intl_data::Cut {
  use snapfire_fsr_ir::intl_data::{splits_words, Cut};
  let bcp47 = |tag: &String| tag.replace('_', "-");
  match section {
    Some(section) => {
      let mut locales: Vec<String> = section.supported.iter().map(bcp47).collect();
      let default = section.default.as_ref().map(bcp47).or_else(|| locales.first().cloned()).unwrap_or_else(|| "en".to_owned());
      for tag in section.formats.iter().map(bcp47) {
        if !locales.contains(&tag) {
          locales.push(tag);
        }
      }
      Cut::new(locales, default, section.unlisted.into()).words(section.dictionaries || splits_words(plan))
    }
    None => Cut::new(vec!["en".to_owned()], "en".to_owned(), Unlisted::default().into()).words(splits_words(plan)),
  }
}

/// Makes the locale data `fsr build` wrote for the application what this program formats from. A host built with `intl_compiled` formats from every locale's data when the application has no file.
pub fn install_data(config: &crate::Config, plan: &str) -> Result<(), String> {
  use snapfire_fsr_ir::intl_data::IntlData;
  let path = config.resolve(DATA_FILE);
  let bytes = match std::fs::read(&path) {
    Ok(bytes) => bytes,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound && cfg!(feature = "intl_compiled") => return Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(format!("no locale data at {}, which `fsr build` writes and this host formats numbers, dates and names from", path.display())),
    Err(e) => return Err(format!("{}: {e}", path.display())),
  };
  let wanted = cut(config.locales.as_ref(), plan);
  let held = IntlData::cut_of(&bytes).map_err(|e| format!("{}: {e}; `fsr build` writes it again", path.display()))?;
  if (&held.locales, &held.default, held.unlisted, held.words) != (&wanted.locales, &wanted.default, wanted.unlisted, wanted.words) {
    let described = |cut: &snapfire_fsr_ir::intl_data::Cut| {
      let unlisted = match cut.unlisted {
        snapfire_fsr_ir::locales::Unlisted::Fallback => "fallback",
        snapfire_fsr_ir::locales::Unlisted::Error => "error",
      };
      format!("{} (default {}, unlisted = \"{unlisted}\"{})", cut.locales.join(", "), cut.default, if cut.words { ", with word dictionaries" } else { "" })
    };
    return Err(format!("{} was cut for the locales {} and the application now calls for {}; `fsr build` writes it again", path.display(), described(&held), described(&wanted)));
  }
  IntlData::from_static(Box::leak(bytes.into_boxed_slice())).map_err(|e| format!("{}: {e}", path.display()))?.install();
  Ok(())
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
/// is one locale, `en` and no source is consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Locales {
  pub supported: Vec<String>,
  pub default: String,
  pub order: Vec<Source>,
  pub remember: bool,
  pub cookie: String,
}

/// One request's locale and what it did to the path.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

impl Resolution {
  /// The path as the browser holds it: the prefix back on when the request
  /// carried one, spelled the way `locales.supported` spells it. What a link
  /// built inside a page has to be relative to, since a link is literal and
  /// dropping the prefix would move the reader to another locale.
  pub fn requested(&self) -> String {
    match self.prefixed {
      true => format!("/{}{}", self.locale.tag, self.path.trim_end_matches('/')),
      false => self.path.clone(),
    }
  }
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
    Self {
      supported: vec!["en".to_owned()],
      default: "en".to_owned(),
      order: Vec::new(),
      remember: false,
      cookie: default_cookie(),
    }
  }

  pub fn from_section(section: &LocalesSection) -> Result<Self, String> {
    if section.supported.is_empty() {
      return Err("locales.supported must name at least one locale".to_owned());
    }
    for tag in &section.supported {
      if !valid_tag(tag) {
        return Err(format!(
          "locales.supported: `{tag}` is not a locale tag; letters, digits, `_` and `-` only"
        ));
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
        other => {
          return Err(format!(
            "locales.order: `{other}` is not a source; the sources are prefix, cookie and header"
          ));
        }
      };
      if !order.contains(&source) {
        order.push(source);
      }
    }
    if section.cookie.is_empty() {
      return Err("locales.cookie must not be empty".to_owned());
    }
    Ok(Self {
      supported: section.supported.clone(),
      default,
      order,
      remember: section.remember,
      cookie: section.cookie.clone(),
    })
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
    self
      .supported
      .iter()
      .find(|t| normalise(t) == wanted)
      .map(String::as_str)
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
    self
      .supported
      .iter()
      .find(|t| language_of(t) == language)
      .map(String::as_str)
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
  /// weight or none the application supports.
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
      Some((chosen_by_prefix, _)) if self.remember && chosen_by_prefix == tag && held != Some(tag) => Some(format!(
        "{}={}; Path=/; Max-Age=31536000; SameSite=Lax",
        self.cookie, tag
      )),
      _ => None,
    };
    Resolution {
      locale: self.locale(tag),
      path: stripped,
      prefixed: prefix.is_some(),
      set_cookie,
    }
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
      unlisted: Unlisted::default(),
      dictionaries: false,
      formats: Vec::new(),
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
    assert_eq!(
      r.set_cookie.as_deref(),
      Some("sf_locale=fr_FR; Path=/; Max-Age=31536000; SameSite=Lax")
    );

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
    let base = LocalesSection {
      supported: vec!["en".to_owned()],
      default: None,
      order: default_order(),
      remember: false,
      cookie: default_cookie(),
      unlisted: Unlisted::default(),
      dictionaries: false,
      formats: Vec::new(),
    };
    assert!(
      bad(LocalesSection {
        supported: vec![],
        ..base.clone()
      })
      .contains("at least one")
    );
    assert!(
      bad(LocalesSection {
        default: Some("fr".to_owned()),
        ..base.clone()
      })
      .contains("not among")
    );
    assert!(
      bad(LocalesSection {
        order: vec!["path".to_owned()],
        ..base.clone()
      })
      .contains("not a source")
    );
    assert!(
      bad(LocalesSection {
        supported: vec!["fr FR".to_owned()],
        ..base.clone()
      })
      .contains("not a locale tag")
    );
    let ok = Locales::from_section(&LocalesSection {
      order: vec!["header".to_owned(), "header".to_owned()],
      ..base
    })
    .unwrap();
    assert_eq!(ok.order, vec![Source::Header]);
    assert_eq!(ok.default, "en");
  }
}

/// The message catalogs under `<app>/locales/*.toml`, one file per locale
/// named by its tag as the application spells it, `fr_FR.toml`. Tables
/// nest: `[cart]` then `empty = "…"` is the key `cart.empty`. A value that is
/// a string, a number or a boolean is kept as text; anything else is refused
/// naming the file and the key. No directory is no catalogs.
pub fn load_catalogs(app: &std::path::Path, default: &str) -> Result<snapfire_fsr_core::ext::Catalogs, String> {
  let dir = app.join("locales");
  let mut tables = std::collections::BTreeMap::new();
  let Ok(entries) = std::fs::read_dir(&dir) else {
    return Ok(snapfire_fsr_core::ext::Catalogs::from_tables(default, tables));
  };
  let mut files: Vec<std::path::PathBuf> = entries
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.extension().is_some_and(|e| e == "toml"))
    .collect();
  files.sort();
  for file in files {
    let tag = file.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let value = toml::Value::Table(text.parse::<toml::Table>().map_err(|e| format!("{}: {e}", file.display()))?);
    let mut table = std::collections::BTreeMap::new();
    flatten(&file, "", &value, &mut table)?;
    tables.insert(tag, table);
  }
  Ok(snapfire_fsr_core::ext::Catalogs::from_tables(default, tables))
}

fn flatten(
  file: &std::path::Path,
  prefix: &str,
  value: &toml::Value,
  out: &mut std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
  match value {
    toml::Value::Table(table) => {
      for (key, inner) in table {
        let full = if prefix.is_empty() {
          key.clone()
        } else {
          format!("{prefix}.{key}")
        };
        flatten(file, &full, inner, out)?;
      }
      Ok(())
    }
    toml::Value::String(s) => {
      out.insert(prefix.to_owned(), s.clone());
      Ok(())
    }
    toml::Value::Integer(n) => {
      out.insert(prefix.to_owned(), n.to_string());
      Ok(())
    }
    toml::Value::Float(f) => {
      out.insert(prefix.to_owned(), f.to_string());
      Ok(())
    }
    toml::Value::Boolean(b) => {
      out.insert(prefix.to_owned(), b.to_string());
      Ok(())
    }
    other => Err(format!(
      "{}: `{prefix}` is {}; a message is a string",
      file.display(),
      other.type_str()
    )),
  }
}
