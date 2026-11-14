//! Writes the tables the crate embeds from the text files under `data/`, which `datagen` writes. `src/data.rs` describes the keys.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[allow(dead_code)]
#[path = "src/table.rs"]
mod table;

type Records = BTreeMap<String, String>;

fn put(records: &mut Records, key: String, value: String) {
  if records.insert(key.clone(), value).is_some() {
    panic!("snapfire_i18n data: `{key}` is written twice");
  }
}

fn fields(line: &str) -> Vec<&str> {
  line.split('\t').collect()
}

fn formats(text: &str, records: &mut Records) {
  let symbols: HashMap<&str, (&str, String)> = text.lines().map(fields).filter(|f| f[0] == "s").map(|f| (f[1], (f[2], f[3..].join("\t")))).collect();
  let field_names: HashMap<&str, &str> = text.lines().map(fields).filter(|f| f[0] == "r").map(|f| (f[1], f[2])).collect();
  for line in text.lines() {
    let f = fields(line);
    match f[0] {
      "V" => put(records, "cV".to_owned(), f[1].to_owned()),
      "F" | "A" => put(records, format!("c{}\t{}", f[0], f[1]), f[2].to_owned()),
      "N" | "L" => put(records, format!("c{}\t{}", f[0], f[1]), f[2..].join("\t")),
      "s" => {}
      "r" => put(records, format!("cr\t{}", f[1]), f[3..].join("\t")),
      "S" => {
        for id in f[2].split(',').filter(|id| !id.is_empty()) {
          let (code, symbol) = symbols.get(id).unwrap_or_else(|| panic!("snapfire_i18n data: no symbol line {id}"));
          put(records, format!("cS\t{}\t{code}", f[1]), symbol.clone());
        }
      }
      "R" => {
        for id in f[2].split(',').filter(|id| !id.is_empty()) {
          let name = field_names.get(id).unwrap_or_else(|| panic!("snapfire_i18n data: no field line {id}"));
          put(records, format!("cR\t{}\t{name}", f[1]), id.to_owned());
        }
      }
      other => panic!("snapfire_i18n data: unknown record `{other}`"),
    }
  }
}

fn zones(text: &str, records: &mut Records) {
  let mut canonical: BTreeMap<&str, &str> = BTreeMap::new();
  let mut periods: BTreeMap<&str, Vec<String>> = BTreeMap::new();
  let mut locales: BTreeSet<&str> = BTreeSet::new();
  for line in text.lines() {
    let f = fields(line);
    match f[0] {
      "V" => put(records, "zV".to_owned(), f[1].to_owned()),
      "K" => {
        for name in &f[1..] {
          canonical.insert(name, f[1]);
        }
      }
      "M" => periods.entry(f[1]).or_default().push(format!("{}\u{1f}{}\u{1f}{}", f[2], f[3], f[4])),
      "G" | "R" | "Z" => put(records, format!("z{}\t{}", f[0], f[1]), f[2..].join("\t")),
      "P" | "D" | "F" | "Y" | "J" | "O" | "H" | "T" => {
        locales.insert(f[1]);
        put(records, format!("z{}\t{}", f[0], f[1]), f[2..].join("\t"));
      }
      "N" | "S" | "C" => {
        locales.insert(f[1]);
        put(records, format!("z{}\t{}\t{}", f[0], f[1], f[2]), f[3..].join("\t"));
      }
      "U" => {
        locales.insert(f[1]);
        put(records, format!("zU\t{}\t{}\t{}\t{}", f[1], f[2], f[3], f[4]), f[5].to_owned());
      }
      other => panic!("snapfire_i18n zone data: unknown record `{other}`"),
    }
  }
  for (name, zone) in canonical {
    put(records, format!("zK\t{name}"), zone.to_owned());
  }
  for (zone, periods) in periods {
    put(records, format!("zM\t{zone}"), periods.join("\u{1e}"));
  }
  for locale in locales {
    put(records, format!("zE\t{locale}"), locale.to_owned());
  }
}

fn names(text: &str, records: &mut Records) {
  for line in text.lines() {
    let f = fields(line);
    match f[0] {
      "V" => put(records, "nV".to_owned(), f[1].to_owned()),
      "Q" => put(records, format!("nQ\t{}", f[1]), f[2..].join("\t")),
      kind => {
        records.insert(format!("n{kind}\t{}\t{}", f[1], f[2]), f[3].to_owned());
      }
    }
  }
}

fn text(name: &str) -> String {
  println!("cargo::rerun-if-changed=data/{name}");
  std::fs::read_to_string(Path::new("data").join(name)).unwrap_or_else(|e| panic!("snapfire_i18n data: read data/{name}: {e}"))
}

fn written(out: &Path, name: &str, records: &Records) {
  let bytes = table::write(records.iter().map(|(key, value)| (key.as_str(), value.as_str()))).unwrap_or_else(|e| panic!("snapfire_i18n data: {e}"));
  std::fs::write(out.join(name), bytes).unwrap_or_else(|e| panic!("snapfire_i18n data: write {name}: {e}"));
}

fn main() {
  println!("cargo::rerun-if-changed=build.rs");
  println!("cargo::rerun-if-changed=src/table.rs");
  let out = std::env::var_os("OUT_DIR").expect("OUT_DIR");
  let out = Path::new(&out);
  if std::env::var_os("CARGO_FEATURE_EMBEDDED").is_none() {
    return;
  }
  let mut core = Records::new();
  formats(&text("cldr.txt"), &mut core);
  zones(&text("zones.txt"), &mut core);
  written(out, "core.table", &core);
  if std::env::var_os("CARGO_FEATURE_DISPLAY_NAMES").is_some() {
    let mut held = Records::new();
    names(&text("names.txt"), &mut held);
    written(out, "names.table", &held);
  }
}
