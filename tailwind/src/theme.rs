//! The theme: every `@theme` variable with its options, the keyframes declared beside them
//! and the lookups utilities resolve candidate values through.

use std::cell::RefCell;
use std::collections::HashSet;

use indexmap::IndexMap;

use crate::Error;
use crate::ast::Node;
use crate::utils::escape::{escape, unescape};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThemeOptions(pub u8);

impl ThemeOptions {
  pub const NONE: ThemeOptions = ThemeOptions(0);
  pub const INLINE: ThemeOptions = ThemeOptions(1);
  pub const REFERENCE: ThemeOptions = ThemeOptions(2);
  pub const DEFAULT: ThemeOptions = ThemeOptions(4);
  pub const STATIC: ThemeOptions = ThemeOptions(8);
  pub const USED: ThemeOptions = ThemeOptions(16);

  pub fn has(self, other: ThemeOptions) -> bool {
    self.0 & other.0 != 0
  }

  pub fn contains(self, other: ThemeOptions) -> bool {
    self.0 & other.0 == other.0
  }
}

impl std::ops::BitOr for ThemeOptions {
  type Output = ThemeOptions;
  fn bitor(self, rhs: ThemeOptions) -> ThemeOptions {
    ThemeOptions(self.0 | rhs.0)
  }
}

impl std::ops::BitOrAssign for ThemeOptions {
  fn bitor_assign(&mut self, rhs: ThemeOptions) {
    self.0 |= rhs.0;
  }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ThemeValue {
  pub value: String,
  pub options: ThemeOptions,
}

const IGNORED: &[(&str, &[&str])] = &[
  ("--font", &["--font-weight", "--font-size"]),
  ("--inset", &["--inset-shadow", "--inset-ring"]),
  (
    "--text",
    &[
      "--text-color",
      "--text-decoration-color",
      "--text-decoration-thickness",
      "--text-indent",
      "--text-shadow",
      "--text-underline-offset",
    ],
  ),
  ("--grid-column", &["--grid-column-start", "--grid-column-end"]),
  ("--grid-row", &["--grid-row-start", "--grid-row-end"]),
];

fn ignored_for(namespace: &str) -> &'static [&'static str] {
  IGNORED
    .iter()
    .find(|(ns, _)| *ns == namespace)
    .map(|(_, keys)| *keys)
    .unwrap_or(&[])
}

fn is_ignored_theme_key(key: &str, namespace: &str) -> bool {
  ignored_for(namespace)
    .iter()
    .any(|ignored| key == *ignored || (key.starts_with(ignored) && key.as_bytes().get(ignored.len()) == Some(&b'-')))
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Theme {
  pub prefix: Option<String>,
  values: IndexMap<String, ThemeValue>,
  keyframes: Vec<Node>,
  used: RefCell<HashSet<String>>,
}

impl Theme {
  pub fn is_empty(&self) -> bool {
    self.values.is_empty()
  }

  pub fn add(&mut self, key: &str, value: &str, options: ThemeOptions) -> Result<(), Error> {
    if let Some(namespace) = key.strip_suffix("-*") {
      if value != "initial" {
        return Err(Error::Message(format!(
          "Invalid theme value `{value}` for namespace `{key}`"
        )));
      }
      if key == "--*" {
        self.values.clear();
      } else {
        self.clear_namespace(namespace, ThemeOptions::NONE);
      }
    }

    if options.has(ThemeOptions::DEFAULT)
      && let Some(existing) = self.values.get(key)
      && !existing.options.has(ThemeOptions::DEFAULT)
    {
      return Ok(());
    }

    if value == "initial" {
      self.values.shift_remove(key);
    } else {
      match self.values.get_mut(key) {
        Some(existing) => {
          existing.value = value.to_owned();
          existing.options = options;
        }
        None => {
          self.values.insert(
            key.to_owned(),
            ThemeValue {
              value: value.to_owned(),
              options,
            },
          );
        }
      }
    }
    Ok(())
  }

  pub fn get(&self, keys: &[&str]) -> Option<String> {
    keys
      .iter()
      .find_map(|key| self.values.get(*key).map(|v| v.value.clone()))
  }

  pub fn get_options(&self, key: &str) -> ThemeOptions {
    let key = unescape(&self.unprefix_key(key));
    let Some(value) = self.values.get(&key) else {
      return ThemeOptions::NONE;
    };
    let mut options = value.options;
    if self.used.borrow().contains(&key) {
      options |= ThemeOptions::USED;
    }
    options
  }

  /// Every variable with its printed name, prefix included.
  pub fn entries(&self) -> Vec<(String, ThemeValue)> {
    self
      .values
      .iter()
      .map(|(k, v)| {
        let mut value = v.clone();
        if self.used.borrow().contains(k) {
          value.options |= ThemeOptions::USED;
        }
        (self.prefix_key(k), value)
      })
      .collect()
  }

  pub fn prefix_key(&self, key: &str) -> String {
    match &self.prefix {
      Some(prefix) => format!("--{prefix}-{}", &key[2.min(key.len())..]),
      None => key.to_owned(),
    }
  }

  fn unprefix_key(&self, key: &str) -> String {
    match &self.prefix {
      Some(prefix) => format!("--{}", key.get(3 + prefix.len()..).unwrap_or("")),
      None => key.to_owned(),
    }
  }

  pub fn clear_namespace(&mut self, namespace: &str, clear: ThemeOptions) {
    let ignored = ignored_for(namespace);
    let keys: Vec<String> = self.values.keys().cloned().collect();
    'outer: for key in keys {
      if !key.starts_with(namespace) {
        continue;
      }
      if clear != ThemeOptions::NONE {
        let options = self.get_options(&key);
        if !options.contains(clear) {
          continue;
        }
      }
      for ignored in ignored {
        if key.starts_with(ignored) {
          continue 'outer;
        }
      }
      self.values.shift_remove(&key);
    }
  }

  fn resolve_key(&self, candidate: Option<&str>, keys: &[&str]) -> Option<String> {
    for namespace in keys {
      let mut key = match candidate {
        Some(value) => format!("{namespace}-{value}"),
        None => (*namespace).to_owned(),
      };

      if !self.values.contains_key(&key) {
        match candidate {
          Some(value) if value.contains('.') => {
            key = format!("{namespace}-{}", value.replace('.', "_"));
            if !self.values.contains_key(&key) {
              continue;
            }
          }
          _ => continue,
        }
      }

      if is_ignored_theme_key(&key, namespace) {
        continue;
      }
      return Some(key);
    }
    None
  }

  fn var(&self, key: &str) -> Option<String> {
    let value = self.values.get(key)?;
    let fallback = if value.options.has(ThemeOptions::REFERENCE) {
      Some(value.value.as_str())
    } else {
      None
    };
    let name = escape(&self.prefix_key(key));
    Some(match fallback {
      Some(fallback) if !fallback.is_empty() => format!("var({name}, {fallback})"),
      _ => format!("var({name})"),
    })
  }

  pub fn mark_used_variable(&self, key: &str) -> bool {
    let key = unescape(&self.unprefix_key(key));
    if !self.values.contains_key(&key) {
      return false;
    }
    self.used.borrow_mut().insert(key)
  }

  pub fn resolve(&self, candidate: Option<&str>, keys: &[&str], options: ThemeOptions) -> Option<String> {
    let key = self.resolve_key(candidate, keys)?;
    let value = self.values.get(&key)?;
    if (options | value.options).has(ThemeOptions::INLINE) {
      return Some(value.value.clone());
    }
    self.var(&key)
  }

  pub fn resolve_value(&self, candidate: Option<&str>, keys: &[&str]) -> Option<String> {
    let key = self.resolve_key(candidate, keys)?;
    self.values.get(&key).map(|v| v.value.clone())
  }

  pub fn resolve_with(
    &self,
    candidate: &str,
    keys: &[&str],
    nested: &[&str],
  ) -> Option<(String, Vec<(String, String)>)> {
    let key = self.resolve_key(Some(candidate), keys)?;
    let mut extra = Vec::new();
    for name in nested {
      let nested_key = format!("{key}{name}");
      let Some(nested_value) = self.values.get(&nested_key) else {
        continue;
      };
      if nested_value.options.has(ThemeOptions::INLINE) {
        extra.push(((*name).to_owned(), nested_value.value.clone()));
      } else if let Some(var) = self.var(&nested_key) {
        extra.push(((*name).to_owned(), var));
      }
    }
    let value = self.values.get(&key)?;
    if value.options.has(ThemeOptions::INLINE) {
      return Some((value.value.clone(), extra));
    }
    Some((self.var(&key)?, extra))
  }

  /// The variables under `namespace`, keyed by what follows it; `None` is the namespace itself.
  pub fn namespace(&self, namespace: &str) -> IndexMap<Option<String>, String> {
    let mut values = IndexMap::new();
    let prefix = format!("{namespace}-");
    let nested = format!("{prefix}-");
    for (key, value) in &self.values {
      if key == namespace {
        values.insert(None, value.value.clone());
      } else if key.starts_with(&nested) {
        values.insert(Some(key[namespace.len()..].to_owned()), value.value.clone());
      } else if key.starts_with(&prefix) {
        values.insert(Some(key[prefix.len()..].to_owned()), value.value.clone());
      }
    }
    values
  }

  pub fn add_keyframes(&mut self, keyframes: Node) {
    self.keyframes.push(keyframes);
  }

  pub fn keyframes(&self) -> &[Node] {
    &self.keyframes
  }
}
