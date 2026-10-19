use assert_cmd::prelude::*;
use std::fs;

mod common;
use common::{get_snapfirec_cmd, Fixture};

fn with_plugin_on_path(cmd: &mut std::process::Command) {
  common::with_plugin_on_path(cmd, "snapfire_svelte", "snapfirec-svelte");
}

fn build() -> Fixture {
  let fixture = Fixture::new("svelte-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  cmd.arg("--root").arg(fixture.root()).assert().success();
  fixture
}

#[test]
fn a_svelte_component_becomes_a_module_and_a_stylesheet_beside_it() {
  let fixture = build();
  let module = fs::read_to_string(fixture.root().join("dist/src/Card.js")).expect("the module");
  assert!(module.contains("export default function"), "{module}");
  assert!(!module.contains(": string"), "the types are gone: {module}");

  let css = fs::read_to_string(fixture.root().join("dist/src/Card.svelte.css")).expect("the stylesheet");
  assert!(css.contains(".card.svelte-"), "the style is scoped: {css}");
  assert!(css.contains("padding: 12px"));
}

#[test]
fn an_import_of_a_svelte_component_names_the_module_it_became() {
  let fixture = build();
  let main = fs::read_to_string(fixture.root().join("dist/src/main.js")).expect("the importer");
  assert!(main.contains("from \"./Card.js\""), "a `.svelte` specifier becomes the `.js` beside it: {main}");
  assert!(!main.contains("./Card.svelte"), "{main}");
}

#[test]
fn what_a_svelte_component_imports_is_resolved_the_way_any_module_is() {
  let fixture = build();
  let module = fs::read_to_string(fixture.root().join("dist/src/Card.js")).expect("the module");
  assert!(module.contains("from \"./util.js\""), "a relative import inside the component resolved: {module}");
  assert!(module.contains("from 'svelte/internal/client'"), "and the runtime stayed an external, as Svelte wrote it: {module}");
}

#[test]
fn the_build_says_snapfirec_svelte_answered_and_what_it_carries() {
  let fixture = Fixture::new("svelte-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  let output = cmd.arg("--root").arg(fixture.root()).output().expect("it runs");
  let said = String::from_utf8_lossy(&output.stdout);
  assert!(said.contains("snapfirec-svelte"), "the plugin is named: {said}");
  assert!(said.contains("svelte 5.57.2"), "with what it compiles through: {said}");
}

#[test]
fn a_runes_module_goes_to_the_plugin_and_keeps_its_name() {
  let fixture = build();
  let module = fs::read_to_string(fixture.root().join("dist/src/counter.svelte.js")).expect("the module, named for its source");
  assert!(!module.contains("$state("), "the plugin compiled the rune rather than the TypeScript front end: {module}");
  assert!(module.contains("from 'svelte/internal/client'"), "{module}");
  assert!(!module.contains(": number"), "snapfirec stripped the types before the plugin saw it: {module}");
  let main = fs::read_to_string(fixture.root().join("dist/src/main.js")).expect("the importer");
  assert!(main.contains("from \"./counter.svelte.js\""), "{main}");
}

#[test]
fn the_build_labels_a_component_with_the_language_it_was_written_in() {
  let fixture = Fixture::new("svelte-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  let output = cmd.arg("--root").arg(fixture.root()).output().expect("it runs");
  let said = String::from_utf8_lossy(&output.stdout);
  assert!(said.contains("Compiling SVELTE: \"src/Card.svelte\""), "{said}");
  assert!(!said.contains("VUE"), "no other plugin's name: {said}");
}

fn stand_in_compiler(version: &str) -> String {
  format!(
    r#"(function () {{
  globalThis.svelte = {{
    VERSION: "{version}",
    parse: function () {{ return {{}}; }},
    compile: function () {{ return {{ js: {{ code: "export const compiledBy = \"{version}\";\nexport default function Card() {{}}\n", map: null }}, css: null, warnings: [] }}; }},
    compileModule: function () {{ return {{ js: {{ code: "// module by {version}\nexport const counter = {{}};\nexport function bump() {{}}\n", map: null }}, css: null, warnings: [] }}; }}
  }};
}})();
"#
  )
}

#[test]
fn the_applications_own_compiler_compiles_and_a_new_one_under_the_same_path_compiles_again() {
  let fixture = Fixture::new("svelte-plugin");
  let compiler = fixture.root().join("vendor-compiler.js");
  fs::write(&compiler, stand_in_compiler("9.9.9")).unwrap();
  let run = || {
    let mut cmd = get_snapfirec_cmd();
    with_plugin_on_path(&mut cmd);
    cmd.arg("--root").arg(fixture.root()).arg("--plugin-compiler").arg(format!("svelte={}", compiler.display())).assert().success();
    fs::read_to_string(fixture.root().join("dist/src/Card.js")).expect("the module")
  };
  assert!(run().contains("\"9.9.9\""), "the named compiler answered");
  fs::write(&compiler, stand_in_compiler("9.9.10")).unwrap();
  assert!(run().contains("\"9.9.10\""), "a new compiler under the same path is not answered from the cache");
}
