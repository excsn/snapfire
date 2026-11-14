//! Which CLDR locale a BCP 47 tag reads its data from.

use crate::data::{fields, Data};

/// The language, script and region subtags of `tag`, cased as CLDR names its locales. Anything after the region, a variant or an extension, is ignored. `_` separates as `-` does.
pub(crate) fn subtags(tag: &str) -> (String, Option<String>, Option<String>) {
  let mut parts = tag.split(['-', '_']).filter(|part| !part.is_empty());
  let language = parts.next().map(str::to_ascii_lowercase).unwrap_or_default();
  let (mut script, mut region) = (None, None);
  for part in parts {
    let alpha = part.chars().all(|c| c.is_ascii_alphabetic());
    match part.len() {
      4 if alpha && script.is_none() && region.is_none() => {
        let mut chars = part.chars();
        script = chars.next().map(|first| first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase());
      }
      2 if alpha && region.is_none() => region = Some(part.to_ascii_uppercase()),
      3 if part.chars().all(|c| c.is_ascii_digit()) && region.is_none() => region = Some(part.to_owned()),
      _ => break,
    }
  }
  (language, script, region)
}

/// The ids of a locale's numbers, currency symbols and relative-time fields.
pub(crate) struct Locale {
  pub numbers: &'static str,
  pub symbols: &'static str,
  pub relative: &'static str,
}

impl Data {
  /// The data of the most specific CLDR locale `tag` falls back to: language, script and region, then language and region, language and script, the language alone and last the root locale. A tag CLDR names as another locale's child, such as `zh-TW` for `zh-Hant`, reads its parent's data.
  pub(crate) fn resolve(&self, tag: &str) -> Locale {
    let [numbers, symbols, relative] = fields(self.formats_of(tag).unwrap_or(""));
    Locale { numbers, symbols, relative }
  }

  /// The ids of the CLDR locale `tag` reads its formats from, the root locale's when it has none nearer.
  pub(crate) fn formats_of(&self, tag: &str) -> Option<&'static str> {
    let found = |name: &str| self.get(&["cL", name]).or_else(|| self.get(&["cA", name]).and_then(|to| self.get(&["cL", to])));
    candidates(tag).iter().find_map(|name| found(name)).or_else(|| self.get(&["cL", "und"]))
  }

  /// The most specific locale holding zone records that `tag` reads, following the same fallback as [`Data::resolve`].
  pub(crate) fn named(&self, tag: &str) -> Option<&'static str> {
    let held = |name: &str| self.get(&["zE", name]);
    candidates(tag).iter().find_map(|name| held(name).or_else(|| self.get(&["cA", name]).and_then(held)))
  }
}

/// The CLDR locale names `tag` may read, most specific first: language, script and region, then language and region, language and script and the language alone.
pub(crate) fn candidates(tag: &str) -> Vec<String> {
  let (language, script, region) = subtags(tag);
  let mut candidates = Vec::with_capacity(4);
  if let (Some(script), Some(region)) = (&script, &region) {
    candidates.push(format!("{language}-{script}-{region}"));
  }
  if let Some(region) = &region {
    candidates.push(format!("{language}-{region}"));
  }
  if let Some(script) = &script {
    candidates.push(format!("{language}-{script}"));
  }
  candidates.push(language);
  candidates
}

#[cfg(test)]
mod tests {
  use super::subtags;

  #[test]
  fn a_tag_is_cased_as_cldr_names_locales() {
    assert_eq!(subtags("zh-hant-tw"), ("zh".to_owned(), Some("Hant".to_owned()), Some("TW".to_owned())));
    assert_eq!(subtags("en_US"), ("en".to_owned(), None, Some("US".to_owned())));
    assert_eq!(subtags("es-419"), ("es".to_owned(), None, Some("419".to_owned())));
    assert_eq!(subtags("de-DE-u-co-phonebk"), ("de".to_owned(), None, Some("DE".to_owned())));
  }
}
