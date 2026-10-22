use std::collections::BTreeMap;

use crate::config::lenient_json;
use crate::edit::Edits;
use crate::syntax::{is_script, parse, specifiers};
use crate::{Report, Tree};

const RESOLVED_EXTENSIONS: &[&str] = &[".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".mts", ".cts", ".json", ".css", ".scss", ".sass", ".svelte", ".vue"];

/// The import aliases `tsconfig.json` or `jsconfig.json` declares:
/// `compilerOptions.paths` and a `baseUrl` that bare specifiers resolve from.
#[derive(Debug, Clone, Default)]
pub(crate) struct Aliases {
  base: Option<String>,
  paths: Vec<(String, Vec<String>)>,
}

impl Aliases {
  pub(crate) fn read(tree: &Tree) -> Self {
    let Some(config) = ["tsconfig.json", "jsconfig.json"].iter().find_map(|f| tree.text(f).and_then(lenient_json)) else {
      return Self::default();
    };
    let options = config.get("compilerOptions");
    let base = options.and_then(|o| o.get("baseUrl")).and_then(|b| b.as_str()).and_then(|b| normalize("", b));
    let mut paths = Vec::new();
    if let Some(map) = options.and_then(|o| o.get("paths")).and_then(|p| p.as_object()) {
      for (pattern, targets) in map {
        let targets = targets.as_array().into_iter().flatten().filter_map(|t| t.as_str()).filter_map(|t| normalize(base.as_deref().unwrap_or(""), t)).collect();
        paths.push((pattern.clone(), targets));
      }
    }
    Self { base, paths }
  }

  /// The paths, without extension, that `spec` may name.
  fn candidates(&self, spec: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (pattern, targets) in &self.paths {
      let matched = match pattern.split_once('*') {
        Some((head, tail)) => spec.strip_prefix(head).and_then(|r| r.strip_suffix(tail)).map(Some),
        None => (spec == pattern).then_some(None),
      };
      if let Some(star) = matched {
        for target in targets {
          out.push(match star {
            Some(star) => target.replacen('*', star, 1),
            None => target.clone(),
          });
        }
      }
    }
    if let Some(base) = &self.base {
      out.extend(normalize(base, spec));
    }
    out
  }
}

/// How a specifier reached its file, so the rewritten one reads the same way.
pub(crate) enum Reached {
  Exact,
  Appended(String),
  /// `./x.js` naming `x.ts`: the specifier's extension, put back.
  Swapped(String),
}

pub(crate) fn find(tree: &Tree, path: &str) -> Option<(String, Reached)> {
  if tree.contains(path) {
    return Some((path.to_owned(), Reached::Exact));
  }
  for ext in RESOLVED_EXTENSIONS {
    let with = format!("{path}{ext}");
    if tree.contains(&with) {
      return Some((with, Reached::Appended((*ext).to_owned())));
    }
  }
  for ext in RESOLVED_EXTENSIONS {
    let with = format!("{path}/index{ext}");
    if tree.contains(&with) {
      return Some((with, Reached::Appended(format!("/index{ext}"))));
    }
  }
  for (js, ts) in [(".js", ".ts"), (".js", ".tsx"), (".jsx", ".tsx"), (".mjs", ".mts")] {
    if let Some(stem) = path.strip_suffix(js) {
      let with = format!("{stem}{ts}");
      if tree.contains(&with) {
        return Some((with, Reached::Swapped(js.to_owned())));
      }
    }
  }
  None
}

/// `rel` joined to the directory `dir`, `.` and `..` resolved; `None` when it
/// climbs out of the root.
pub(crate) fn normalize(dir: &str, rel: &str) -> Option<String> {
  let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
  for part in rel.split('/') {
    match part {
      "" | "." => {}
      ".." => {
        parts.pop()?;
      }
      p => parts.push(p),
    }
  }
  Some(parts.join("/"))
}

pub(crate) fn dirname(path: &str) -> &str {
  path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// The specifier that reaches `to` from a module in the directory `from`.
pub(crate) fn relative(from: &str, to: &str) -> String {
  let from: Vec<&str> = from.split('/').filter(|p| !p.is_empty()).collect();
  let to_parts: Vec<&str> = to.split('/').filter(|p| !p.is_empty()).collect();
  let common = from.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();
  let up = from.len() - common;
  let rest = to_parts[common..].join("/");
  if up == 0 {
    format!("./{rest}")
  } else {
    format!("{}{rest}", "../".repeat(up))
  }
}

fn restyle(spec: String, reached: &Reached) -> String {
  match reached {
    Reached::Exact => spec,
    Reached::Appended(suffix) => spec.strip_suffix(suffix.as_str()).map(str::to_owned).unwrap_or(spec),
    Reached::Swapped(ext) => match spec.rsplit_once('.') {
      Some((stem, _)) => format!("{stem}{ext}"),
      None => spec,
    },
  }
}

/// `old` with each file moved as `moves` says (old path to new) and every
/// relative or aliased specifier in a moved script rewritten to reach the
/// same file at its new path. A file `moves` leaves out is not in the result.
/// A specifier reaching under `app/src/` through an alias is written as
/// `@src/...`, FSR's alias for that directory.
pub(crate) fn relocate(old: &Tree, moves: &BTreeMap<String, String>, aliases: &Aliases, report: &mut Report) -> Tree {
  let mut out = Tree::new();
  for (from, to) in moves {
    let Some(bytes) = old.get(from) else { continue };
    let rewritten = match old.text(from) {
      Some(text) if is_script(from) => rewrite(old, moves, aliases, from, to, text, report).map(String::into_bytes),
      _ => None,
    };
    out.insert(to.clone(), rewritten.unwrap_or_else(|| bytes.to_vec()));
  }
  out
}

fn rewrite(old: &Tree, moves: &BTreeMap<String, String>, aliases: &Aliases, from: &str, to: &str, text: &str, report: &mut Report) -> Option<String> {
  let parsed = match parse(from, text) {
    Ok(p) => p,
    Err(e) => {
      report.note(from, 0, format!("does not parse, so its imports were not rewritten: {e}"));
      return None;
    }
  };
  let mut edits = Edits::default();
  for spec in specifiers(&parsed) {
    let relative_spec = spec.value.starts_with("./") || spec.value.starts_with("../") || spec.value == "." || spec.value == "..";
    let candidates = if relative_spec { normalize(dirname(from), &spec.value).into_iter().collect() } else { aliases.candidates(&spec.value) };
    let Some((target, reached)) = candidates.iter().find_map(|c| find(old, c)) else { continue };
    let Some(new_target) = moves.get(&target) else {
      report.note(from, spec.line, format!("imports `{}`, which is not part of the migrated application", spec.value));
      continue;
    };
    let new_spec = match new_target.strip_prefix("app/src/") {
      Some(rest) if !relative_spec => format!("@src/{rest}"),
      _ => relative(dirname(to), new_target),
    };
    let new_spec = restyle(new_spec, &reached);
    if new_spec != spec.value {
      edits.replace(spec.range, new_spec);
    }
  }
  (!edits.is_empty()).then(|| edits.apply(text))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  #[test]
  fn a_relative_import_follows_both_files_to_their_new_directories() {
    let old = tree(&[("app/page.tsx", "import Nav from \"../components/Nav\";\nimport \"./globals.css\";\n"), ("components/Nav.tsx", ""), ("app/globals.css", "")]);
    let moves: BTreeMap<String, String> = [("app/page.tsx", "app/routes/page.tsx"), ("components/Nav.tsx", "app/src/components/Nav.tsx"), ("app/globals.css", "app/routes/globals.css")]
      .into_iter()
      .map(|(a, b)| (a.to_owned(), b.to_owned()))
      .collect();
    let out = relocate(&old, &moves, &Aliases::default(), &mut Report::default());
    assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import Nav from \"../src/components/Nav\";\nimport \"./globals.css\";\n");
  }

  #[test]
  fn an_alias_from_tsconfig_paths_becomes_the_src_alias() {
    let old = tree(&[
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./*\"] } } }"),
      ("app/page.tsx", "import { db } from '@/lib/db';\nimport x from 'react';\n"),
      ("lib/db/index.ts", ""),
    ]);
    let moves: BTreeMap<String, String> = [("app/page.tsx", "app/routes/page.tsx"), ("lib/db/index.ts", "app/src/lib/db/index.ts")].into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect();
    let out = relocate(&old, &moves, &Aliases::read(&old), &mut Report::default());
    assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import { db } from '@src/lib/db';\nimport x from 'react';\n");
  }

  #[test]
  fn a_js_specifier_naming_a_ts_file_keeps_its_extension() {
    let old = tree(&[("src/a/x.ts", "export * from \"../b/y.js\";\n"), ("src/b/y.ts", "")]);
    let moves: BTreeMap<String, String> = [("src/a/x.ts", "app/src/a/deeper/x.ts"), ("src/b/y.ts", "app/src/b/y.ts")].into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect();
    let out = relocate(&old, &moves, &Aliases::default(), &mut Report::default());
    assert_eq!(out.text("app/src/a/deeper/x.ts").unwrap(), "export * from \"../../b/y.js\";\n");
  }

  #[test]
  fn an_import_of_a_file_left_behind_is_a_note() {
    let old = tree(&[("app/page.tsx", "import cfg from \"../next.config.js\";\n"), ("next.config.js", "")]);
    let moves: BTreeMap<String, String> = [("app/page.tsx".to_owned(), "app/routes/page.tsx".to_owned())].into_iter().collect();
    let mut report = Report::default();
    relocate(&old, &moves, &Aliases::default(), &mut report);
    assert_eq!(report.notes[0].to_string(), "app/page.tsx:1: imports `../next.config.js`, which is not part of the migrated application");
  }
}
