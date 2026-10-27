//! An application's ESLint configuration as `fsr.lint.toml`, which `fsr lint` reads.

use crate::{Report, Tree};

/// The flat configuration's names, which ESLint 9 reads in place of any `.eslintrc`.
const FLAT: [&str; 5] = [
  "eslint.config.js",
  "eslint.config.mjs",
  "eslint.config.cjs",
  "eslint.config.ts",
  "eslint.config.mts",
];
const RC: [&str; 2] = [".eslintrc.json", ".eslintrc"];
const IGNORE: &str = ".eslintignore";

/// The ESLint configuration of `input` written into the migrated `tree` as `fsr.lint.toml`. It reads the
/// input, since the passes move the application under `app/` and leave its root files behind.
pub(crate) fn translate(input: &Tree, tree: &mut Tree, report: &mut Report) {
  let flat = FLAT.iter().find(|p| input.get(p).is_some()).copied();
  let rc = RC.iter().find(|p| input.get(p).is_some()).copied();
  let package = input
    .text("package.json")
    .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
    .and_then(|p| p.get("eslintConfig").cloned());
  let (from, translation) = match (flat, rc) {
    (Some(flat), _) => {
      if let Some(rc) = rc {
        report.note(rc, 0, format!("is not read: {flat} is, as ESLint 9 reads only it"));
      }
      (
        flat,
        snapfire_lint::eslint::translate_flat(input.text(flat).unwrap_or_default(), std::path::Path::new(flat)),
      )
    }
    (None, Some(rc)) => match serde_json::from_str::<serde_json::Value>(input.text(rc).unwrap_or_default()) {
      Ok(config) => (rc, snapfire_lint::eslint::translate_eslintrc(&config)),
      Err(_) => {
        report.note(
          rc,
          0,
          "is not JSON, so its rules are not carried over into fsr.lint.toml",
        );
        return;
      }
    },
    (None, None) => match package {
      Some(config) => ("package.json", snapfire_lint::eslint::translate_eslintrc(&config)),
      None => return,
    },
  };
  for note in &translation.notes {
    report.note(from, 0, note.clone());
  }
  let mut table: toml::Table = translation.toml.parse().unwrap_or_default();
  if let Some(ignore) = input.text(IGNORE) {
    let patterns = ignore
      .lines()
      .map(str::trim)
      .filter(|l| !l.is_empty() && !l.starts_with('#'));
    let globs = patterns.map(|p| {
      toml::Value::String(
        p.strip_suffix('/')
          .map_or_else(|| p.to_owned(), |dir| format!("{dir}/**")),
      )
    });
    let ignores = table.entry("ignores").or_insert_with(|| toml::Value::Array(Vec::new()));
    if let toml::Value::Array(list) = ignores {
      list.extend(globs);
    }
  }
  for path in FLAT.iter().chain(&RC).chain([&IGNORE]) {
    tree.remove(path);
  }
  if !table.is_empty() {
    tree.insert("fsr.lint.toml", toml::to_string(&table).unwrap_or_default());
  }
}
