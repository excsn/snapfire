use assert_cmd::prelude::*;
use std::fs;

mod common;
use common::{get_snapfirec_cmd, Fixture};

fn with_plugin_on_path(cmd: &mut std::process::Command) {
  common::with_plugin_on_path(cmd, "snapfire_vue", "snapfirec-vue");
}

#[test]
fn a_component_becomes_a_module_and_a_stylesheet_beside_it() {
  let fixture = Fixture::new("vue-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  cmd.arg("--root").arg(fixture.root()).assert().success();

  let module = fs::read_to_string(fixture.root().join("dist/src/Card.vue")).expect("the module");
  assert!(module.contains("export default _sfc_main"), "{module}");
  assert!(module.contains("_sfc_main.render"), "the template is bound to the component");
  assert!(!module.contains(": string"), "the plugin handed back typescript and the build finished it");

  let css = fs::read_to_string(fixture.root().join("dist/src/Card.vue.css")).expect("the stylesheet");
  assert!(css.contains("[data-v-"), "the style is scoped: {css}");
  assert!(css.contains("padding: 12px"));
}

#[test]
fn an_import_of_a_component_names_the_module_it_became_by_its_own_name() {
  let fixture = Fixture::new("vue-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  cmd.arg("--root").arg(fixture.root()).assert().success();

  let main = fs::read_to_string(fixture.root().join("dist/src/main.js")).expect("the importer");
  assert!(main.contains("from \"./Card.vue\""), "{main}");
  assert!(!main.contains("./Card.js"), "{main}");
}

#[test]
fn a_component_and_a_module_whose_names_differ_only_by_case_both_reach_the_output() {
  let fixture = Fixture::new("vue-plugin");
  fs::write(fixture.root().join("src/card.ts"), "export const stock: number = 3;\n").expect("written");
  fs::write(fixture.root().join("src/main.ts"), "import Card from \"./Card.vue\";\nimport { stock } from \"./card\";\nexport { Card, stock };\n").expect("written");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  cmd.arg("--root").arg(fixture.root()).assert().success();

  let component = fs::read_to_string(fixture.root().join("dist/src/Card.vue")).expect("the component's module");
  assert!(component.contains("_sfc_main"), "{component}");
  let module = fs::read_to_string(fixture.root().join("dist/src/card.js")).expect("the module");
  assert!(module.contains("stock = 3"), "{module}");
  let main = fs::read_to_string(fixture.root().join("dist/src/main.js")).expect("the importer");
  assert!(main.contains("from \"./Card.vue\"") && main.contains("from \"./card.js\""), "{main}");
}

#[test]
fn what_the_component_imports_is_resolved_the_way_any_module_is() {
  let fixture = Fixture::new("vue-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  cmd.arg("--root").arg(fixture.root()).assert().success();

  let module = fs::read_to_string(fixture.root().join("dist/src/Card.vue")).expect("the module");
  assert!(module.contains("from \"./util.js\""), "a relative import inside the component resolved: {module}");
  assert!(module.contains("from \"vue\""), "and the framework stayed an external: {module}");
}

#[test]
fn the_build_says_which_plugin_answered_and_what_it_carries() {
  let fixture = Fixture::new("vue-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  let output = cmd.arg("--root").arg(fixture.root()).output().expect("it runs");
  let said = String::from_utf8_lossy(&output.stdout);
  assert!(said.contains("snapfirec-vue"), "the plugin is named: {said}");
  assert!(said.contains("@vue/compiler-sfc"), "with what it compiles through: {said}");
}

#[test]
fn a_missing_plugin_names_the_binary_the_command_and_the_file_that_wanted_it() {
  let fixture = Fixture::new("vue-plugin");
  let mut cmd = get_snapfirec_cmd();
  cmd.env("PATH", "/usr/bin:/bin");
  let output = cmd.arg("--root").arg(fixture.root()).output().expect("it runs");
  assert!(!output.status.success(), "a component nothing can compile fails the build");

  let complained = String::from_utf8_lossy(&output.stderr);
  assert!(complained.contains("snapfirec-vue"), "the binary it looked for: {complained}");
  assert!(complained.contains("cargo install snapfire_vue"), "how to get it: {complained}");
  assert!(complained.contains("Card.vue"), "and what needed it: {complained}");
}

#[test]
fn a_component_that_does_not_compile_is_reported_with_its_place_and_stops_the_build() {
  let fixture = Fixture::new("vue-plugin");
  fs::write(fixture.root().join("src/Card.vue"), "<template><div v-if=\"</div></template>\n").expect("written");

  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  let output = cmd.arg("--root").arg(fixture.root()).output().expect("it runs");
  assert!(!output.status.success());

  let complained = String::from_utf8_lossy(&output.stderr);
  assert!(complained.contains("src/Card.vue"), "the file is named: {complained}");
  assert!(complained.contains(':'), "with a location: {complained}");
}

#[test]
fn a_block_with_src_compiles_the_sibling_the_plugin_asked_for() {
  let fixture = Fixture::new("vue-plugin-src");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  cmd.arg("--root").arg(fixture.root()).assert().success();

  let css = fs::read_to_string(fixture.root().join("dist/src/Card.vue.css")).expect("the stylesheet");
  assert!(css.contains("padding: 12px") && css.contains("[data-v-"), "the linked sheet, scoped: {css}");
}

#[test]
fn a_second_build_answers_unchanged_components_from_the_cache() {
  let fixture = Fixture::new("vue-plugin");
  let mut cmd = get_snapfirec_cmd();
  with_plugin_on_path(&mut cmd);
  let first = cmd.arg("--root").arg(fixture.root()).output().expect("it runs");
  assert!(first.status.success());
  assert!(String::from_utf8_lossy(&first.stdout).contains("Plugin cache: 0 of 1"), "{}", String::from_utf8_lossy(&first.stdout));

  let mut again = get_snapfirec_cmd();
  with_plugin_on_path(&mut again);
  let second = again.arg("--root").arg(fixture.root()).output().expect("it runs");
  assert!(second.status.success());
  let said = String::from_utf8_lossy(&second.stdout);
  assert!(said.contains("Plugin cache: 1 of 1"), "nothing changed, so nothing was compiled again: {said}");
  assert!(fs::read_to_string(fixture.root().join("dist/src/Card.vue")).expect("the module").contains("_sfc_main"));
}
