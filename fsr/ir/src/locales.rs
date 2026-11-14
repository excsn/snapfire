//! The locales an application formats in and which of them a requested tag gets, as ECMA-402's lookup matcher picks one.

use icu_locale_core::{LanguageIdentifier, Locale};

/// What a tag gets when no listed locale covers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unlisted {
  /// The default locale, as a browser formats in its own default.
  Fallback,
  /// A refusal.
  Error,
}

/// Why a list of locales is not one an application can format in.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LocalesError {
  #[error("no locale is listed")]
  Empty,
  #[error("`{0}` is not a locale")]
  Malformed(String),
  #[error("`{0}` carries an extension, which a listed locale does not")]
  Extension(String),
  #[error("the default locale `{0}` is not one of the listed locales")]
  DefaultUnlisted(String),
}

/// Why a requested tag resolves to no locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refused {
  #[error("the tag is not a locale")]
  Malformed,
  #[error("no listed locale covers the tag")]
  Unlisted,
}

/// An application's listed locales, its default among them and what an unlisted tag gets.
#[derive(Debug, Clone)]
pub struct Locales {
  listed: Vec<(String, LanguageIdentifier)>,
  default: usize,
  unlisted: Unlisted,
}

impl Locales {
  pub fn new<T: AsRef<str>>(listed: &[T], default: &str, unlisted: Unlisted) -> Result<Self, LocalesError> {
    if listed.is_empty() {
      return Err(LocalesError::Empty);
    }
    let listed = listed.iter().map(|tag| plain(tag.as_ref()).map(|id| (id.to_string(), id))).collect::<Result<Vec<_>, _>>()?;
    let wanted = plain(default)?;
    let default = listed.iter().position(|(_, id)| *id == wanted).ok_or_else(|| LocalesError::DefaultUnlisted(default.to_owned()))?;
    Ok(Self { listed, default, unlisted })
  }

  /// The listed locale `tag` formats in: the longest listed tag that `tag` is or truncates to, subtag by subtag from the right, else the default or a refusal. The result carries `tag`'s Unicode extension.
  pub fn resolve(&self, tag: &str) -> Result<Locale, Refused> {
    let requested: Locale = tag.parse().map_err(|_| Refused::Malformed)?;
    let mut candidate = requested.id.to_string();
    let found = loop {
      if let Some((_, id)) = self.listed.iter().find(|(name, _)| *name == candidate) {
        break id;
      }
      match candidate.rfind('-') {
        Some(at) => candidate.truncate(at),
        None => match self.unlisted {
          Unlisted::Fallback => break &self.listed[self.default].1,
          Unlisted::Error => return Err(Refused::Unlisted),
        },
      }
    };
    let mut resolved = Locale::from(found.clone());
    resolved.extensions.unicode = requested.extensions.unicode;
    Ok(resolved)
  }
}

fn plain(tag: &str) -> Result<LanguageIdentifier, LocalesError> {
  let locale: Locale = tag.parse().map_err(|_| LocalesError::Malformed(tag.to_owned()))?;
  if !locale.extensions.is_empty() {
    return Err(LocalesError::Extension(tag.to_owned()));
  }
  Ok(locale.id)
}
