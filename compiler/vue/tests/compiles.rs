use snapfire_plugin::{Lang, Options, Outcome, Severity};
use snapfire_vue::Compiler;

const CARD: &str = r#"<script setup lang="ts">
import { ref } from "vue";
const props = defineProps<{ title: string; count: number }>();
const n = ref(props.count);
</script>

<template>
  <div class="card">
    <h2>{{ title }}</h2>
    <button @click="n++">{{ n }}</button>
  </div>
</template>

<style scoped>
.card { padding: 12px; }
</style>
"#;

fn compile(source: &str) -> Outcome {
  let compiler = Compiler::new().expect("the compiler boots");
  compiler.compile("src/Card.vue", source, &Options::default(), &Default::default()).expect("the driver answers")
}

fn ok(outcome: Outcome) -> snapfire_plugin::Compiled {
  match outcome {
    Outcome::Ok(compiled) => compiled,
    Outcome::Failed { diagnostics } => panic!("refused: {diagnostics:?}"),
    Outcome::Needs { files } => panic!("asked for {files:?}"),
  }
}

#[test]
fn the_carried_compiler_names_its_version() {
  let compiler = Compiler::new().expect("the compiler boots");
  assert_eq!(compiler.version().expect("a version"), "3.5.13");
}

#[test]
fn a_component_becomes_one_module_and_its_styles() {
  let compiled = ok(compile(CARD));

  assert!(compiled.js.contains("export default _sfc_main"), "one default export");
  assert!(compiled.js.contains("_sfc_main.render = render"), "the template is bound to the component");
  assert!(compiled.js.contains("_sfc_main.__scopeId"), "a scoped style means the component carries the id");
  assert!(compiled.js.contains("defineProps") || compiled.js.contains("props:"), "the props survived");

  let css = compiled.css.expect("a scoped style block produces css");
  assert!(css.contains("[data-v-"), "the scope attribute is in the selector: {css}");
  assert!(css.contains("padding: 12px"), "the declaration survived");

  assert!(compiled.diagnostics.is_empty(), "nothing to report: {:?}", compiled.diagnostics);
}

#[test]
fn the_scope_id_follows_the_content_so_two_components_never_share_one() {
  let one = ok(compile(CARD));
  let two = ok(compile(&CARD.replace("padding: 12px", "padding: 13px")));
  let id = |css: Option<String>| css.expect("css").split("[data-v-").nth(1).expect("an id").split(']').next().expect("closed").to_owned();
  assert_ne!(id(one.css), id(two.css));
}

#[test]
fn a_component_with_no_script_is_still_a_component() {
  let compiled = ok(compile("<template><p>just markup</p></template>\n"));
  assert!(compiled.js.contains("const _sfc_main = {}"), "an empty component object");
  assert!(compiled.js.contains("_sfc_main.render = render"));
  assert!(compiled.css.is_none(), "no style block means no stylesheet");
}

#[test]
fn a_template_that_does_not_parse_is_refused_with_its_line() {
  let outcome = compile("<template><div v-if=\"</div></template>\n");
  let Outcome::Failed { diagnostics } = outcome else { panic!("a broken template should be refused") };
  assert!(!diagnostics.is_empty());
  assert_eq!(diagnostics[0].severity, Severity::Error);
  assert_eq!(diagnostics[0].file.as_deref(), Some("src/Card.vue"));
  assert!(diagnostics[0].line.is_some(), "a location came back: {:?}", diagnostics[0]);
}

#[test]
fn a_language_the_plugin_does_not_carry_is_named_rather_than_emitted_wrong() {
  let outcome = compile("<template><p>x</p></template>\n<style lang=\"scss\">.a { .b { color: red } }</style>\n");
  let Outcome::Failed { diagnostics } = outcome else { panic!("scss should be refused") };
  assert!(diagnostics.iter().any(|d| d.message.contains("scss")), "the language is named: {diagnostics:?}");
}

#[test]
fn one_context_compiles_many_components() {
  let compiler = Compiler::new().expect("the compiler boots");
  for i in 0..25 {
    let source = CARD.replace("padding: 12px", &format!("padding: {i}px"));
    let outcome = compiler.compile(&format!("src/Card{i}.vue"), &source, &Options::default(), &Default::default()).expect("answers");
    assert!(matches!(outcome, Outcome::Ok(_)), "component {i} compiled");
  }
}

#[test]
fn a_typed_script_block_is_handed_on_as_typescript_rather_than_stripped_here() {
  assert_eq!(ok(compile(CARD)).lang, Lang::Ts, "lang=\"ts\" means the build's own front end finishes it");
  let plain = "<script>\nexport default { name: \"Plain\" };\n</script>\n<template><p>x</p></template>\n";
  assert_eq!(ok(compile(plain)).lang, Lang::Js);
}

const LINKED: &str = r#"<script setup lang="ts">
const props = defineProps<{ title: string }>();
</script>

<template>
  <div class="card">{{ title }}</div>
</template>

<style src="./card.css" scoped></style>
"#;

#[test]
fn a_block_with_src_names_the_file_it_needs_and_compiles_once_it_is_handed_over() {
  let compiler = Compiler::new().expect("the compiler boots");
  let first = compiler.compile("src/Card.vue", LINKED, &Options::default(), &Default::default()).expect("answers");
  assert!(matches!(&first, Outcome::Needs { files } if files == &["./card.css".to_owned()]), "{first:?}");

  let mut files = std::collections::BTreeMap::new();
  files.insert("./card.css".to_owned(), ".card { padding: 12px; }\n".to_owned());
  let compiled = ok(compiler.compile("src/Card.vue", LINKED, &Options::default(), &files).expect("answers"));
  let css = compiled.css.expect("the linked sheet is the component's style");
  assert!(css.contains("padding: 12px") && css.contains("[data-v-"), "scoped like an inline block: {css}");
  assert_eq!(compiled.deps, ["./card.css"], "and the file is a dependency the build watches");
}
