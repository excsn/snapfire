use std::collections::BTreeMap;

use snapfire_compiler_wire::{Described, Lang, Options, Outcome};
use snapfire_vue::Compiler;

const SCALER: &str = r#"<script setup lang="ts">
import { computed, ref } from "vue";

const props = defineProps<{ serves: number; names: string[] }>();
const wanted = ref(props.serves);
const doubled = computed(() => wanted.value * 2);
function more(): void {
  wanted.value += 1;
}
</script>

<template>
  <div class="scaler" :class="{ big: wanted > 4 }">
    <!-- a note the browser never sees -->
    <button class="step" @click="more">+</button>
    <span v-if="wanted !== serves">scaled from {{ serves }}</span>
    <ul>
      <li v-for="n in names" :key="n">{{ n }}</li>
    </ul>
  </div>
</template>

<style scoped>
.scaler { border-top: 1px solid #eee; }
</style>
"#;

fn describe(source: &str) -> Described {
  let compiler = Compiler::new().expect("the compiler boots");
  match compiler.describe("src/Scaler.vue", source, &Options::default(), &Default::default()).expect("the driver answers") {
    Outcome::Described(described) => described,
    other => panic!("not a description: {other:?}"),
  }
}

#[test]
fn a_description_carries_the_script_block_the_bindings_and_the_scope() {
  let described = describe(SCALER);
  let script = described.script.expect("a script block");
  assert!(script.setup, "the setup form");
  assert!(!script.plain, "no plain script beside it");
  assert_eq!(script.lang, Lang::Ts);
  assert_eq!(script.line, 1, "the content starts on the tag's line, since it opens with a newline");
  assert!(script.content.starts_with("\nimport { computed, ref } from \"vue\";"), "{}", script.content);
  assert_eq!(described.bindings.get("props").map(String::as_str), Some("setup-reactive-const"));
  assert_eq!(described.bindings.get("serves").map(String::as_str), Some("props"));
  assert_eq!(described.bindings.get("names").map(String::as_str), Some("props"));
  assert_eq!(described.bindings.get("wanted").map(String::as_str), Some("setup-ref"));
  assert_eq!(described.bindings.get("doubled").map(String::as_str), Some("setup-ref"));
  assert_eq!(described.bindings.get("more").map(String::as_str), Some("setup-const"));
  assert!(!described.bindings.keys().any(|k| k.starts_with("__")), "the metadata's own flags are left out: {:?}", described.bindings);
  assert!(described.scope.as_deref().is_some_and(|s| s.starts_with("data-v-")), "{:?}", described.scope);
  assert!(described.diagnostics.is_empty(), "{:?}", described.diagnostics);
}

#[test]
fn a_description_carries_the_template_as_a_tree_with_directives_still_on_their_elements() {
  let described = describe(SCALER);
  let template = described.template.expect("a template");
  let root = &template["children"][0];
  assert_eq!(root["node"], "element");
  assert_eq!(root["tag"], "div");
  assert_eq!(root["kind"], 0);
  let props = root["props"].as_array().expect("props");
  assert_eq!(props[0]["prop"], "attribute");
  assert_eq!(props[0]["name"], "class");
  assert_eq!(props[0]["value"], "scaler");
  assert_eq!(props[1]["prop"], "directive");
  assert_eq!(props[1]["name"], "bind");
  assert_eq!(props[1]["arg"], "class");
  assert_eq!(props[1]["argStatic"], true);
  assert_eq!(props[1]["exp"], "{ big: wanted > 4 }");
  let children = root["children"].as_array().expect("children");
  let tags: Vec<&str> = children.iter().filter_map(|c| c["tag"].as_str()).collect();
  assert_eq!(tags, ["button", "span", "ul"], "the comment is not among them: {children:?}");
  let button = &children[0];
  let on = &button["props"][1];
  assert_eq!(on["name"], "on");
  assert_eq!(on["arg"], "click");
  assert_eq!(on["exp"], "more");
  let span = &children[1];
  assert_eq!(span["props"][0]["name"], "if");
  assert_eq!(span["props"][0]["exp"], "wanted !== serves");
  assert_eq!(span["children"][0]["node"], "text");
  assert_eq!(span["children"][0]["content"], "scaled from ");
  assert_eq!(span["children"][1]["node"], "interpolation");
  assert_eq!(span["children"][1]["content"], "serves");
  assert_eq!(span["children"][1]["line"], 16, "positions are the file's, not the block's");
  let li = &children[2]["children"][0];
  assert_eq!(li["props"][0]["name"], "for");
  assert_eq!(li["props"][0]["exp"], "n in names");
  assert_eq!(li["props"][1]["name"], "bind");
  assert_eq!(li["props"][1]["arg"], "key");
}

#[test]
fn a_component_with_no_script_describes_its_template_alone() {
  let described = describe("<template><p>just markup</p></template>\n");
  assert!(described.script.is_none());
  assert!(described.bindings.is_empty());
  assert!(described.scope.is_none());
  assert_eq!(described.template.expect("a template")["children"][0]["tag"], "p");
}

#[test]
fn a_template_that_does_not_parse_is_refused_the_same_way_a_compile_is() {
  let compiler = Compiler::new().expect("the compiler boots");
  let outcome = compiler.describe("src/Bad.vue", "<template><div v-if=\"</div></template>\n", &Options::default(), &Default::default()).expect("the driver answers");
  let Outcome::Failed { diagnostics } = outcome else { panic!("a broken template should be refused: {outcome:?}") };
  assert_eq!(diagnostics[0].file.as_deref(), Some("src/Bad.vue"));
}

#[test]
fn the_compiled_module_leaves_template_comments_out_like_the_description_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let Outcome::Ok(compiled) = compiler.compile("src/Scaler.vue", SCALER, &Options::default(), &Default::default()).expect("the driver answers") else { panic!("compiles") };
  assert!(!compiled.js.contains("a note the browser never sees"), "{}", compiled.js);
}

fn runtime() -> BTreeMap<String, String> {
  let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/runtime");
  BTreeMap::from([
    ("vue".to_owned(), std::fs::read_to_string(dir.join("vue.js")).expect("the runtime is vendored for tests")),
    ("vue/server-renderer".to_owned(), std::fs::read_to_string(dir.join("server-renderer.js")).expect("the server renderer is vendored for tests")),
  ])
}

const CARD: &str = r#"<script setup>
import { ref } from "vue";
const props = defineProps({ title: String, items: Array });
const open = ref(false);
</script>

<template>
  <div class="card" :class="{ open }">
    <h2 :title="title">{{ title }}</h2>
    <p v-if="open">shown</p>
    <ul>
      <li v-for="item in items" :key="item">{{ item }}</li>
    </ul>
    <slot />
  </div>
</template>
"#;

#[test]
fn vue_renders_a_component_on_the_server_from_inside_the_plugin() {
  let compiler = Compiler::new().expect("the compiler boots");
  let Outcome::Ok(module) = compiler.ssr_module("src/Card.vue", CARD, &Options::default(), &Default::default()).expect("the driver answers") else { panic!("compiles for the server") };
  assert!(module.js.contains("_sfc_main.ssrRender = ssrRender;"), "{}", module.js);
  assert_eq!(module.lang, Lang::Js);
  let html = compiler.render_module(&module.js, r#"{"title":"Tea & <cake>","items":["a","b"]}"#, Some("<b>child</b>"), &runtime()).expect("renders");
  assert_eq!(html, "<div class=\"card\"><h2 title=\"Tea &amp; &lt;cake&gt;\">Tea &amp; &lt;cake&gt;</h2><!----><ul><!--[--><li>a</li><li>b</li><!--]--></ul><!--[--><sf-s data-sf-children><b>child</b></sf-s><!--]--></div>", "an empty string on a dynamic attribute is written bare");
  let again = compiler.render_module(&module.js, r#"{"title":"Two","items":[]}"#, None, &runtime()).expect("renders again");
  assert_eq!(again, "<div class=\"card\"><h2 title=\"Two\">Two</h2><!----><ul><!--[--><!--]--></ul><!--[--><!--]--></div>");
}
