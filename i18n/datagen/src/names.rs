//! `snapfire_i18n_datagen --names <cldr-json> <out>`: the display names of the `cldr-localenames-full`, `cldr-numbers-full` and `cldr-dates-full` packages, in the line format `snapfire_i18n`'s `src/names.rs` describes.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use crate::{field, locales, read, str_at};

/// The names one locale gives, keyed by kind and code.
type Names = BTreeMap<(char, String), String>;

fn own_names(root: &Path, locale: &str) -> Result<(Names, Option<(String, String)>), String> {
  let mut names = Names::new();
  let main = root.join("cldr-localenames-full/main").join(locale);
  for (file, key, kind) in [("languages.json", "languages", 'L'), ("territories.json", "territories", 'R'), ("scripts.json", "scripts", 'S')] {
    let Ok(json) = read(&main.join(file)) else { continue };
    if let Some(map) = json.pointer(&format!("/main/{locale}/localeDisplayNames/{key}")).and_then(Value::as_object) {
      for (code, name) in map {
        if let Some(name) = name.as_str() {
          names.insert((kind, code.clone()), name.to_owned());
        }
      }
    }
  }
  let mut pattern = None;
  if let Ok(json) = read(&main.join("localeDisplayNames.json")) {
    let display = json.pointer(&format!("/main/{locale}/localeDisplayNames"));
    if let Some(calendars) = display.and_then(|d| d.pointer("/types/calendar")).and_then(Value::as_object) {
      for (code, name) in calendars {
        if let Some(name) = name.as_str() {
          names.insert(('K', code.clone()), name.to_owned());
        }
      }
    }
    if let Some(p) = display.and_then(|d| d.get("localeDisplayPattern")) {
      pattern = Some((str_at(p, &["localePattern"]).unwrap_or("").to_owned(), str_at(p, &["localeSeparator"]).unwrap_or("").to_owned()));
    }
  }
  if let Ok(json) = read(&root.join("cldr-numbers-full/main").join(locale).join("currencies.json")) {
    if let Some(map) = json.pointer(&format!("/main/{locale}/numbers/currencies")).and_then(Value::as_object) {
      for (code, info) in map {
        if let Some(name) = str_at(info, &["displayName"]) {
          names.insert(('C', code.clone()), name.to_owned());
        }
      }
    }
  }
  if let Ok(json) = read(&root.join("cldr-dates-full/main").join(locale).join("dateFields.json")) {
    if let Some(map) = json.pointer(&format!("/main/{locale}/dates/fields")).and_then(Value::as_object) {
      for (code, info) in map {
        if let Some(name) = str_at(info, &["displayName"]) {
          names.insert(('F', code.clone()), name.to_owned());
        }
      }
    }
  }
  Ok((names, pattern))
}

pub(crate) fn generate(root: &Path) -> Result<String, String> {
  let core = root.join("cldr-core");
  let version = str_at(&read(&core.join("package.json"))?, &["version"]).ok_or("no cldr-core version")?.to_owned();
  let parents = read(&core.join("supplemental/parentLocales.json"))?;
  let parents = parents.pointer("/supplemental/parentLocales/parentLocale").and_then(Value::as_object).ok_or("no parent locales")?.clone();
  let mut own: HashMap<String, (Names, Option<(String, String)>)> = HashMap::new();
  for locale in locales(&root.join("cldr-dates-full/main"))? {
    let made = own_names(root, &locale)?;
    own.insert(locale, made);
  }
  let parent_of = |locale: &str| -> Option<String> {
    if locale == "und" {
      return None;
    }
    let step = |l: &str| parents.get(l).and_then(Value::as_str).map(str::to_owned).or_else(|| l.rfind('-').map(|at| l[..at].to_owned())).unwrap_or_else(|| "und".to_owned());
    let mut parent = step(locale);
    while parent != "und" && parent != "root" && !own.contains_key(&parent) {
      parent = step(&parent);
    }
    Some(if parent == "root" { "und".to_owned() } else { parent })
  };
  let resolved = |locale: &str, key: &(char, String)| -> Option<String> {
    let mut at = Some(locale.to_owned());
    while let Some(l) = at {
      if let Some(name) = own.get(&l).and_then(|(names, _)| names.get(key)) {
        return Some(name.clone());
      }
      at = parent_of(&l);
    }
    None
  };
  let resolved_pattern = |locale: &str| -> Option<(String, String)> {
    let mut at = Some(locale.to_owned());
    while let Some(l) = at {
      if let Some(pattern) = own.get(&l).and_then(|(_, p)| p.clone()) {
        return Some(pattern);
      }
      at = parent_of(&l);
    }
    None
  };
  let mut out = String::new();
  writeln!(out, "V\t{version}").unwrap();
  let mut sorted: Vec<&String> = own.keys().collect();
  sorted.sort();
  for locale in sorted {
    let parent = parent_of(locale);
    let pattern = resolved_pattern(locale);
    if pattern.is_some() && parent.as_deref().is_none_or(|p| resolved_pattern(p) != pattern) {
      let (shape, separator) = pattern.expect("checked");
      writeln!(out, "Q\t{locale}\t{}\t{}", field(&shape)?, field(&separator)?).unwrap();
    }
    for key in own[locale].0.keys() {
      let here = resolved(locale, key);
      if parent.as_deref().is_none_or(|p| resolved(p, key) != here) {
        writeln!(out, "{}\t{locale}\t{}\t{}", key.0, field(&key.1)?, field(&here.unwrap_or_default())?).unwrap();
      }
    }
  }
  Ok(out)
}
