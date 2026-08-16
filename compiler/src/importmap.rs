use anyhow::{Context, Result};
use jsonc_parser::ParseOptions;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// An import map as the page will serve it.
///
/// Resolution follows the spec rather than exact key matching: a key ending in `/` maps a whole
/// prefix, and the longest matching key wins. Treating the map as a dictionary would report
/// `lodash/debounce` as missing from a map that resolves it perfectly well through `lodash/`.
#[derive(Debug, Deserialize, Default)]
pub struct ImportMap {
  #[serde(default)]
  pub imports: BTreeMap<String, String>,
  #[serde(default)]
  pub scopes: BTreeMap<String, BTreeMap<String, String>>,
}

impl ImportMap {
  pub fn load(path: &Path) -> Result<Self> {
    let content = fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;

    jsonc_parser::parse_to_serde_value::<ImportMap>(&content, &ParseOptions::default())
      .with_context(|| format!("Failed to parse {:?}", path))
  }

  /// Whether `specifier` resolves for a module served at `importer`.
  ///
  /// `importer` is `None` when the build has no public path, in which case scopes cannot be
  /// evaluated because a scope is keyed by the importing module's URL.
  pub fn resolves(&self, specifier: &str, importer: Option<&str>) -> bool {
    if let Some(importer) = importer {
      for (prefix, mappings) in self.scopes.iter().rev() {
        if importer.starts_with(prefix.as_str()) && lookup(mappings, specifier) {
          return true;
        }
      }
    }

    lookup(&self.imports, specifier)
  }

  pub fn uses_scopes(&self) -> bool {
    !self.scopes.is_empty()
  }
}

fn lookup(mappings: &BTreeMap<String, String>, specifier: &str) -> bool {
  if mappings.contains_key(specifier) {
    return true;
  }

  mappings
    .keys()
    .any(|key| key.ends_with('/') && specifier.starts_with(key.as_str()))
}
