//! The front end for Vue and Svelte templates: each framework's own compiler reads the component,
//! through [`Compilers`]. Vue's template tree is checked by the rules here; Svelte's compiler checks
//! its own and each warning it gives is a finding under `svelte/` and its code.

use std::path::Path;

use serde_json::Value;
use snapfire_compiler_wire::{Diagnostic, Options, Outcome, Severity, Unit};

use crate::{Compilers, Level, RuleInfo};

/// A template finding by line and column, 1-based.
pub(crate) struct Found {
  pub rule: String,
  pub line: usize,
  pub column: usize,
  pub message: String,
  pub help: Option<String>,
}

/// The rules over a Vue template, from eslint-plugin-vue's essential set.
pub(crate) const VUE_RULES: [RuleInfo; 4] = [
  RuleInfo {
    id: "vue/no-template-key",
    default: Level::Error,
    recommended: true,
    category: "correctness",
    fixable: false,
    docs: "### What it does\n\nDisallows `key` on a `<template>` that has no `v-for`.\n\n### Why is this bad?\n\n`<template>` renders no element, so a key on it keys nothing. Put it on the element inside.\n",
  },
  RuleInfo {
    id: "vue/no-use-v-if-with-v-for",
    default: Level::Error,
    recommended: true,
    category: "correctness",
    fixable: false,
    docs: "### What it does\n\nDisallows `v-if` on the element that has `v-for`.\n\n### Why is this bad?\n\nIn Vue 3 `v-if` runs first, so it cannot read the `v-for` variable. Filter the list in a computed value, or move `v-if` to a wrapping `<template>`.\n",
  },
  RuleInfo {
    id: "vue/require-component-is",
    default: Level::Error,
    recommended: true,
    category: "correctness",
    fixable: false,
    docs: "### What it does\n\nRequires `is` or `:is` on `<component>`.\n\n### Why is this bad?\n\nWithout it `<component>` has nothing to render.\n",
  },
  RuleInfo {
    id: "vue/require-v-for-key",
    default: Level::Error,
    recommended: true,
    category: "correctness",
    fixable: false,
    docs: "### What it does\n\nRequires `:key` on an element that has `v-for`.\n\n### Why is this bad?\n\nWithout a key Vue patches the list in place, so an element's state follows its position rather than its item when the list changes.\n",
  },
];

/// Svelte's warnings, one rule per code.
pub(crate) const SVELTE_FAMILY: RuleInfo = RuleInfo {
  id: "svelte/*",
  default: Level::Warn,
  recommended: false,
  category: "correctness",
  fixable: false,
  docs: "### What it does\n\nEach warning Svelte's compiler gives is a finding under `svelte/` and its code with dashes: `a11y_missing_attribute` is `svelte/a11y-missing-attribute`. They default to `warn`, as Svelte gives them. A warning from a `snapfirec-svelte` that gives no code is `svelte/warning`. Svelte's own `<!-- svelte-ignore code -->` silences one as well.\n",
};

/// The compilers as `snapfirec-vue` and `snapfirec-svelte`, each started on first use and kept for the run.
#[derive(Default)]
pub struct Plugins {
  workers: parking_lot::Mutex<std::collections::HashMap<String, snapfire_compiler_wire::host::Worker>>,
}

impl Plugins {
  fn ask(&self, ext: &str, unit: Unit, describe: bool) -> Result<Outcome, String> {
    let mut workers = self.workers.lock();
    if !workers.contains_key(ext) {
      let worker = snapfire_compiler_wire::host::Worker::start(ext).map_err(|e| e.to_string())?;
      workers.insert(ext.to_owned(), worker);
    }
    let worker = workers.get_mut(ext).expect("started above");
    let answer = match describe {
      true => worker.describe(vec![unit]),
      false => worker.compile(vec![unit]),
    };
    match answer {
      Ok(mut outcomes) if outcomes.len() == 1 => Ok(outcomes.remove(0)),
      Ok(outcomes) => Err(format!("snapfirec-{ext} answered {} outcomes for one component", outcomes.len())),
      Err(e) => {
        workers.remove(ext);
        Err(e.to_string())
      }
    }
  }
}

impl Compilers for Plugins {
  fn describe(&self, ext: &str, unit: Unit) -> Result<Outcome, String> {
    self.ask(ext, unit, true)
  }

  fn compile(&self, ext: &str, unit: Unit) -> Result<Outcome, String> {
    self.ask(ext, unit, false)
  }
}

/// The rule of a warning from a plugin that gives no code.
const UNCODED: &str = "svelte/warning";

/// Whether this front end reads `path`.
pub(crate) fn reads(path: &Path) -> Option<&'static str> {
  match path.extension().and_then(|e| e.to_str()) {
    Some("vue") => Some("vue"),
    Some("svelte") => Some("svelte"),
    _ => None,
  }
}

pub(crate) fn lint(ext: &str, path: &Path, source: &str, compilers: &dyn Compilers) -> Vec<Found> {
  let unit = Unit { filename: path.display().to_string(), path: path.display().to_string(), source: source.to_owned(), options: Options::default(), files: Default::default() };
  let asked = match ext {
    "vue" => compilers.describe(ext, unit),
    _ => compilers.compile(ext, unit),
  };
  match asked {
    Err(message) => vec![Found { rule: crate::COMPILER.to_owned(), line: 1, column: 1, message, help: Some(format!("install snapfirec-{ext} to lint its templates")) }],
    Ok(Outcome::Failed { diagnostics }) => diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| at(crate::PARSE.to_owned(), d)).collect(),
    Ok(Outcome::Described(described)) => described.template.as_ref().map(vue).unwrap_or_default(),
    Ok(Outcome::Ok(compiled)) => compiled.diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| at(d.code.as_ref().map_or_else(|| UNCODED.to_owned(), |code| format!("svelte/{}", code.replace('_', "-"))), d)).collect(),
    Ok(Outcome::Needs { .. }) => Vec::new(),
  }
}

fn at(rule: String, d: &Diagnostic) -> Found {
  let message = d.message.lines().next().unwrap_or_default().to_owned();
  Found { rule, line: d.line.unwrap_or(1) as usize, column: d.column.unwrap_or(1) as usize, message, help: None }
}

/// The findings of the Vue rules over the tree `snapfirec-vue` describes.
fn vue(template: &Value) -> Vec<Found> {
  let mut out = Vec::new();
  walk(template, &mut out);
  out.sort_by_key(|f| (f.line, f.column));
  out
}

fn walk(node: &Value, out: &mut Vec<Found>) {
  if node["node"] == "element" {
    element(node, out);
  }
  for child in node["children"].as_array().into_iter().flatten() {
    walk(child, out);
  }
}

fn directive<'v>(element: &'v Value, name: &str) -> Option<&'v Value> {
  element["props"].as_array()?.iter().find(|p| p["prop"] == "directive" && p["name"] == name)
}

fn bound_or_set(element: &Value, name: &str) -> bool {
  element["props"].as_array().is_some_and(|props| props.iter().any(|p| (p["prop"] == "attribute" && p["name"] == name) || (p["prop"] == "directive" && p["name"] == "bind" && p["arg"] == name)))
}

fn element(node: &Value, out: &mut Vec<Found>) {
  let tag = node["tag"].as_str().unwrap_or_default();
  let mut find = |rule: &str, message: &str, help: &str| {
    out.push(Found { rule: rule.to_owned(), line: node["line"].as_u64().unwrap_or(1) as usize, column: node["column"].as_u64().unwrap_or(1) as usize, message: message.to_owned(), help: Some(help.to_owned()) });
  };
  let for_ = directive(node, "for").is_some();
  if for_ && tag != "slot" && !bound_or_set(node, "key") {
    find("vue/require-v-for-key", &format!("`<{tag}>` with `v-for` has no `:key`"), "bind a key that is unique per item: `:key=\"item.id\"`");
  }
  if for_ && directive(node, "if").is_some() {
    find("vue/no-use-v-if-with-v-for", &format!("`<{tag}>` has both `v-if` and `v-for`"), "filter the list in a computed value, or put `v-if` on a wrapping `<template>`");
  }
  if tag == "component" && !bound_or_set(node, "is") {
    find("vue/require-component-is", "`<component>` has no `is`", "name what it renders with `:is`");
  }
  if tag == "template" && !for_ && bound_or_set(node, "key") {
    find("vue/no-template-key", "`<template>` has a `key` but no `v-for`", "put the key on the element inside");
  }
}

/// The `<!-- -->` comments in a component's source, with their byte offsets.
pub(crate) fn comments(source: &str) -> Vec<(String, usize, usize)> {
  let mut out = Vec::new();
  let mut from = 0;
  while let Some(start) = source[from..].find("<!--").map(|i| from + i) {
    let Some(end) = source[start..].find("-->").map(|i| start + i + 3) else { break };
    out.push((source[start..end].to_owned(), start, end));
    from = end;
  }
  out
}
