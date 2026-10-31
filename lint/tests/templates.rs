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

fn template_rules(source: &str) -> Vec<String> {
  let report = linter("").lint_source(Path::new("A.vue"), source);
  report.findings.iter().filter(|f| f.rule.starts_with("vue/")).map(|f| f.rule.clone()).collect()
}

#[test]
fn vues_essential_template_rules_each_find_their_case() {
  let script = "\n<script setup>const xs = []; const a = 1; const b = 2; const f = () => 0; const html = '';</script>\n";
  let cases = [
    ("<template><p v-if></p></template>", "vue/valid-v-if"),
    ("<template><p v-else-if=\"a\"></p></template>", "vue/valid-v-else-if"),
    ("<template><p v-if=\"a\"></p><p v-else=\"b\"></p></template>", "vue/valid-v-else"),
    ("<template><p v-for=\"xs\" :key=\"a\"></p></template>", "vue/valid-v-for"),
    ("<template><p v-show></p></template>", "vue/valid-v-show"),
    ("<template><p v-html></p></template>", "vue/valid-v-html"),
    ("<template><p v-text></p></template>", "vue/valid-v-text"),
    ("<template><p v-once=\"a\"></p></template>", "vue/valid-v-once"),
    ("<template><p v-cloak=\"a\"></p></template>", "vue/valid-v-cloak"),
    ("<template><p v-bind></p></template>", "vue/valid-v-bind"),
    ("<template><p :title.foo=\"a\"></p></template>", "vue/valid-v-bind"),
    ("<template><p @click></p></template>", "vue/valid-v-on"),
    ("<template><input v-model=\"f()\"></template>", "vue/valid-v-model"),
    ("<template><p v-model=\"a\"></p></template>", "vue/valid-v-model"),
    ("<template><p v-slot=\"a\"></p></template>", "vue/valid-v-slot"),
    ("<template><p v-memo></p></template>", "vue/valid-v-memo"),
    ("<template><p 0abc=\"a\"></p></template>", "vue/valid-attribute-name"),
    ("<template></template>", "vue/valid-template-root"),
    ("<template><p title=\"a\" :title=\"b\"></p></template>", "vue/no-duplicate-attributes"),
    ("<template><p v-if=\"a\"></p><p v-else-if=\"a\"></p></template>", "vue/no-dupe-v-else-if"),
    ("<template><textarea>{{ a }}</textarea></template>", "vue/no-textarea-mustache"),
    ("<template><MyThing v-html=\"html\" /></template>", "vue/no-v-text-v-html-on-component"),
    ("<template><p v-html=\"html\">child</p></template>", "vue/no-child-content"),
    ("<template><template v-if=\"a\" class=\"x\"><p /></template></template>", "vue/no-useless-template-attributes"),
    ("<template><template v-for=\"x in xs\"><p :key=\"x\" /></template></template>", "vue/no-v-for-template-key-on-child"),
    ("<template><transition><p /></transition></template>", "vue/require-toggle-inside-transition"),
    ("<template><button @click=\"f\" @click.ctrl=\"f\"></button></template>", "vue/use-v-on-exact"),
    ("<template><MyThing :title.sync=\"a\" /></template>", "vue/no-deprecated-v-bind-sync"),
    ("<template><MyThing @click.native=\"f\" /></template>", "vue/no-deprecated-v-on-native-modifier"),
    ("<template><input @keyup.13=\"f\"></template>", "vue/no-deprecated-v-on-number-modifiers"),
    ("<template><MyThing><p slot=\"head\" /></MyThing></template>", "vue/no-deprecated-slot-attribute"),
    ("<template><MyThing><template slot-scope=\"a\"><p /></template></MyThing></template>", "vue/no-deprecated-slot-scope-attribute"),
    ("<template><MyThing><template scope=\"a\"><p /></template></MyThing></template>", "vue/no-deprecated-scope-attribute"),
    ("<template><table is=\"my-row\"></table></template>", "vue/no-deprecated-html-element-is"),
    ("<template><MyThing inline-template><p /></MyThing></template>", "vue/no-deprecated-inline-template"),
    ("<template><router-link to=\"/\" tag=\"li\">x</router-link></template>", "vue/no-deprecated-router-link-tag-prop"),
    ("<template><p v-is=\"'x'\"></p></template>", "vue/no-deprecated-v-is"),
    ("<template><MyThing v-bind=\"$listeners\" /></template>", "vue/no-deprecated-dollar-listeners-api"),
  ];
  let missed: Vec<String> = cases.iter().filter_map(|(template, rule)| {
    let found = template_rules(&format!("{template}{script}"));
    (!found.iter().any(|r| r == rule)).then(|| format!("{rule} on {template}: {found:?}"))
  }).collect();
  assert!(missed.is_empty(), "{missed:#?}");
}

#[test]
fn vues_template_rules_pass_what_is_valid() {
  let source = "<template>\n  <p v-if=\"a\">1</p>\n  <p v-else-if=\"b\">2</p>\n  <p v-else>3</p>\n  <ul><li v-for=\"(x, i) in xs\" :key=\"i\" :title=\"x\" class=\"a\" :class=\"b\" @click.stop>{{ x }}</li></ul>\n  <input v-model.trim=\"a\" @keyup.enter=\"f\" @click.ctrl.exact=\"f\" @click.exact=\"f\">\n  <MyThing v-model:title=\"a\" :is=\"'vue:x'\"><template #head=\"{ item }\"><p v-show=\"item\" /></template></MyThing>\n  <transition><p v-if=\"a\" /></transition>\n  <template v-for=\"x in xs\" :key=\"x\"><p v-text=\"x\" /></template>\n  <p v-once v-pre v-cloak :id />\n  <table is=\"vue:my-row\"></table>\n</template>\n<script setup>const xs = []; const a = 1; const b = 2; const f = () => 0; const id = 'p';</script>\n";
  let found = template_rules(source);
  assert!(found.is_empty(), "{found:?}");
}
