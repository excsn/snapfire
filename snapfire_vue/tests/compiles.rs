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

fn probe(source: &str, body: &str) -> String {
  let compiler = Compiler::new().expect("the compiler boots");
  let script = format!("const source = {};\n{body}", serde_json::to_string(source).unwrap());
  compiler.eval(&script).expect("the probe runs")
}

#[test]
fn a_single_file_component_goes_through_all_four_stages() {
  let out = probe(
    CARD,
    r#"
    const { descriptor, errors } = sfc.parse(source, { filename: "Card.vue" });
    const id = "abc123";
    const script = sfc.compileScript(descriptor, { id, inlineTemplate: false });
    const template = sfc.compileTemplate({
      source: descriptor.template.content,
      filename: "Card.vue",
      id,
      scoped: true,
      compilerOptions: { bindingMetadata: script.bindings },
    });
    const style = sfc.compileStyle({
      source: descriptor.styles[0].content,
      filename: "Card.vue",
      id: "data-v-" + id,
      scoped: true,
    });
    return {
      errors: errors.length,
      blocks: { script: !!descriptor.scriptSetup, template: !!descriptor.template, styles: descriptor.styles.length },
      scriptHasProps: script.content.includes("props"),
      templateErrors: template.errors.length,
      hasRender: template.code.includes("export function render"),
      hasScopeId: script.content.length > 0,
      css: style.code.trim(),
      styleErrors: style.errors.length,
    };
    "#,
  );
  println!("{out}");
  let v: serde_json::Value = serde_json::from_str(&out).expect("json");
  assert_eq!(v["errors"], 0, "the SFC parses");
  assert_eq!(v["templateErrors"], 0, "the template compiles");
  assert_eq!(v["styleErrors"], 0, "the style compiles");
  assert_eq!(v["blocks"]["styles"], 1);
  assert_eq!(v["hasRender"], true, "a render function came out");
  assert!(v["css"].as_str().unwrap().contains("[data-v-abc123]"), "the scoped attribute is in the css: {}", v["css"]);
}
