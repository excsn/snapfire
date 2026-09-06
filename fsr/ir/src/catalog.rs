//! Message catalogs: one table of dotted keys to strings per locale, which
//! `i18n.t` reads under the ambient locale. Every locale's table is held
//! merged over the default locale's, so a key the locale lacks reads as the
//! default's and the table the browser receives answers exactly as the
//! server does.

use std::collections::BTreeMap;
use std::sync::Arc;

pub type Table = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalogs {
  default: String,
  /// Per locale, the default's table with the locale's own laid over it.
  merged: BTreeMap<String, Arc<Table>>,
  /// `merged` as JSON, the text a document or a payload carries.
  json: BTreeMap<String, Arc<str>>,
  /// Per locale, how many keys its own file held.
  own: BTreeMap<String, usize>,
}

impl Catalogs {
  /// `tables` by locale tag as the application spells it; `default` is the
  /// locale whose table fills in for every other.
  pub fn from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Self {
    let default = default.into();
    let base = tables.get(&default).cloned().unwrap_or_default();
    let mut merged = BTreeMap::new();
    let mut json = BTreeMap::new();
    let mut own = BTreeMap::new();
    for (tag, table) in tables {
      own.insert(tag.clone(), table.len());
      let mut whole = base.clone();
      whole.extend(table);
      json.insert(tag.clone(), Arc::from(serde_json::to_string(&whole).expect("a string table serialises").as_str()));
      merged.insert(tag, Arc::new(whole));
    }
    Self { default, merged, json, own }
  }

  pub fn is_empty(&self) -> bool {
    self.merged.is_empty()
  }

  pub fn default_tag(&self) -> &str {
    &self.default
  }

  /// Every locale with a table and how many keys its own file held, by tag.
  pub fn rows(&self) -> Vec<(String, usize)> {
    self.own.iter().map(|(tag, n)| (tag.clone(), *n)).collect()
  }

  /// The merged table for `tag`, or the default locale's when `tag` has none.
  pub fn table(&self, tag: &str) -> Option<&Arc<Table>> {
    self.merged.get(tag).or_else(|| self.merged.get(&self.default))
  }

  /// `table` as JSON.
  pub fn json(&self, tag: &str) -> Option<Arc<str>> {
    self.json.get(tag).or_else(|| self.json.get(&self.default)).cloned()
  }

  pub fn lookup(&self, tag: &str, key: &str) -> Option<&str> {
    self.table(tag)?.get(key).map(String::as_str)
  }
}
