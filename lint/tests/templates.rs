use std::path::Path;

use snapfire_compiler_wire::{Outcome, Unit};
use snapfire_lint::{Compilers, Config, Level, Linter, Report};

/// The compilers in this process, as the plugin workers would answer.
struct InProcess {
  vue: snapfire_vue::Compiler,
  svelte: snapfire_svelte::Compiler,
}

impl Compilers for InProcess {
  fn describe(&self, ext: &str, unit: Unit) -> Result<Outcome, String> {
    match ext {
      "vue" => self.vue.describe(&unit.filename, &unit.source, &unit.options, &unit.files).map_err(|e| e.to_string()),
      "svelte" => self.svelte.describe(&unit.filename, &unit.source, &unit.options).map_err(|e| e.to_string()),
      other => Err(format!("no compiler for .{other}")),
    }
  }

  fn compile(&self, ext: &str, unit: Unit) -> Result<Outcome, String> {
    match ext {
      "vue" => self.vue.compile(&unit.filename, &unit.source, &unit.options, &unit.files).map_err(|e| e.to_string()),
      "svelte" => self.svelte.compile(&unit.filename, &unit.source, &unit.options, &unit.files).map_err(|e| e.to_string()),
      other => Err(format!("no compiler for .{other}")),
    }
  }
}

fn linter(config: &str) -> Linter {
  let config = Config::parse(config, Path::new("fsr.lint.toml")).unwrap();
  let compilers = InProcess { vue: snapfire_vue::Compiler::new().unwrap(), svelte: snapfire_svelte::Compiler::new().unwrap() };
  Linter::new(config).unwrap().with_compilers(Box::new(compilers))
}

fn found(report: &Report) -> Vec<(&str, usize, usize, Level)> {
  report.findings.iter().map(|f| (f.rule.as_str(), f.line, f.column, f.level)).collect()
}

const LIST: &str = "<script setup lang=\"ts\">\nconst items = [1, 2];\n</script>\n\n<template>\n  <ul>\n    <li v-for=\"item in items\">{{ item }}</li>\n    <li v-for=\"item in items\" :key=\"item\">{{ item }}</li>\n  </ul>\n</template>\n";

#[test]
fn a_vue_template_is_linted_where_it_sits() {
  let report = linter("").lint_source(Path::new("List.vue"), LIST);
  assert_eq!(found(&report), [("vue/require-v-for-key", 7, 5, Level::Error)], "{}", report.render());
}

#[test]
fn vue_template_rules_find_what_eslint_plugin_vue_does() {
  let cases = [
    ("<template><div v-for=\"x in xs\" v-if=\"x\" :key=\"x\"></div></template>\n<script setup>const xs = [];</script>\n", "vue/no-use-v-if-with-v-for"),
    ("<template><component /></template>\n", "vue/require-component-is"),
    ("<template><template key=\"a\"><p /></template></template>\n", "vue/no-template-key"),
  ];
  for (source, rule) in cases {
    let report = linter("").lint_source(Path::new("A.vue"), source);
    let rules: Vec<_> = report.findings.iter().map(|f| f.rule.as_str()).collect();
    assert_eq!(rules, [rule], "{source}\n{}", report.render());
  }
  let clean = "<template><component :is=\"x\" /><template v-for=\"x in xs\" :key=\"x\"><p /></template></template>\n<script setup>const xs = []; const x = 1;</script>\n";
  assert!(linter("").lint_source(Path::new("A.vue"), clean).findings.is_empty());
}

#[test]
fn a_template_comment_silences_a_template_finding() {
  let source = LIST.replace("    <li v-for=\"item in items\">", "    <!-- eslint-disable-next-line vue/require-v-for-key -->\n    <li v-for=\"item in items\">");
  let report = linter("").lint_source(Path::new("List.vue"), &source);
  assert!(report.findings.is_empty(), "{}", report.render());
  assert_eq!(report.suppressed.len(), 1);
}

const IMAGE: &str = "<script lang=\"ts\">\n  let { src }: { src: string } = $props();\n</script>\n\n<img {src}>\n";

#[test]
fn svelte_s_warnings_are_findings_by_their_code() {
  let report = linter("").lint_source(Path::new("Image.svelte"), IMAGE);
  assert_eq!(found(&report), [("svelte/a11y-missing-attribute", 5, 1, Level::Warn)], "{}", report.render());
  let report = linter("[rules]\n\"svelte/a11y-missing-attribute\" = \"error\"\n").lint_source(Path::new("Image.svelte"), IMAGE);
  assert_eq!(found(&report), [("svelte/a11y-missing-attribute", 5, 1, Level::Error)]);
  let report = linter("[rules]\n\"svelte/a11y-missing-attribute\" = \"off\"\n").lint_source(Path::new("Image.svelte"), IMAGE);
  assert!(report.findings.is_empty());
}

#[test]
fn a_component_that_does_not_compile_is_a_parse_finding() {
  let report = linter("").lint_source(Path::new("Broken.svelte"), "<p>\n{#if x}\n</p>\n");
  assert!(report.findings.iter().any(|f| f.rule == "lint/parse" && f.level == Level::Error), "{}", report.render());
}

#[test]
fn without_compilers_only_the_scripts_are_linted() {
  let linter = Linter::new(Config::default()).unwrap();
  let report = linter.lint_source(Path::new("List.vue"), &LIST.replace("const items", "debugger;\nconst items"));
  let rules: Vec<_> = report.findings.iter().map(|f| f.rule.as_str()).collect();
  assert_eq!(rules, ["eslint/no-debugger"]);
}

/// A plugin from before warnings carried their code.
struct Uncoded;

impl Compilers for Uncoded {
  fn describe(&self, _ext: &str, _unit: Unit) -> Result<Outcome, String> {
    Err("no describe".to_owned())
  }

  fn compile(&self, _ext: &str, _unit: Unit) -> Result<Outcome, String> {
    let warning = snapfire_compiler_wire::Diagnostic { line: Some(1), column: Some(1), ..snapfire_compiler_wire::Diagnostic::warning("`<img>` element should have an alt attribute") };
    Ok(Outcome::Ok(snapfire_compiler_wire::Compiled { js: String::new(), lang: Default::default(), css: None, source_map: None, deps: Vec::new(), diagnostics: vec![warning] }))
  }
}

#[test]
fn a_warning_without_a_code_is_still_a_finding() {
  let linter = Linter::new(Config::default()).unwrap().with_compilers(Box::new(Uncoded));
  let report = linter.lint_source(Path::new("Pic.svelte"), "<img src=\"a.png\">\n");
  assert_eq!(found(&report), [("svelte/warning", 1, 1, Level::Warn)], "{}", report.render());
}
