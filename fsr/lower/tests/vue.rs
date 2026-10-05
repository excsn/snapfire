//! The Vue front end against Vue's own server renderer: each fixture is
//! lowered, rendered in Rust as an island with children and compared byte
//! for byte with what `@vue/server-renderer` writes for the same component,
//! props and children.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_compiler_wire::{Described, Lang, Options, Outcome};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Component, Entry, Expr, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::{Frameworks, Interpreter, Owner, VueMajor};
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::LowerError;
use snapfire_vue::Compiler;
use swc_core::common::{sync::Lrc, FileName, Globals, Mark, SourceMap, GLOBALS};
use swc_core::ecma::codegen::{text_writer::JsWriter, Emitter};
use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};
use swc_core::ecma::transforms::base::{fixer::fixer, hygiene::hygiene, resolver};
use swc_core::ecma::transforms::typescript::strip;

const TONIGHT: &str = include_str!("../../examples/recipes_vue_ts/app/src/ui/Tonight.vue");
const PLAN: &str = include_str!("../../examples/recipes_vue_ts/app/src/ui/PlanRecipe.vue");
const SCALER: &str = include_str!("../../examples/recipes_vue_ts/app/src/ui/Scaler.vue");
const STORE: &str = include_str!("../../examples/recipes_vue_ts/app/src/store.ts");

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_vue_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn describe(compiler: &Compiler, file: &str, source: &str) -> Described {
  match compiler.describe(file, source, &Options::default(), &Default::default()).expect("the driver answers") {
    Outcome::Described(described) => described,
    other => panic!("{file}: not described: {other:?}"),
  }
}

/// Lowers `file` as the set would in a build: described, then placed.
fn lower(compiler: &Compiler, dir: &Path, file: &str, source: &str) -> Result<ComponentSet, LowerError> {
  let mut set = ComponentSet::new(dir);
  let described = describe(compiler, file, source);
  if std::env::var("FSR_VUE_DUMP").is_ok() {
    eprintln!("{file}: {}", serde_json::to_string(&described.template).unwrap());
  }
  set.describe(file, described);
  set.lower(&format!("{file}#default"))?;
  Ok(set)
}

/// A page placing `module` as an island with `props` and `children`, the way a
/// template does, so the render writes the island's markup with its region.
fn host(module: &str, children: Vec<Tmpl>) -> Component {
  Component { body: Vec::new(), render: Tmpl::Island { module: module.to_owned(), props: vec![Entry::Spread(Expr::var("$props"))], children, when: None, mode: None, id: 0, define: false }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: Owner::Fsr, shadow: None }
}

fn render_rust(set: &ComponentSet, module: &str, props: &ValueMap, children: Vec<Tmpl>) -> String {
  let library: Components = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(snapfire_fsr_ir::render::prepare(c)))).collect();
  let interpreter = Interpreter::default().with_frameworks(Frameworks { react: None, vue: Some(VueMajor::V3) });
  let rendered = interpreter.render(&host(module, children), props, &library).expect("renders");
  assert_eq!(rendered.islands.len(), 1, "{rendered:?}");
  rendered.islands[0].body.html.clone()
}

fn runtime() -> BTreeMap<String, String> {
  let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compiler/vue/tests/runtime");
  BTreeMap::from([
    ("vue".to_owned(), std::fs::read_to_string(dir.join("vue.js")).expect("the runtime is vendored for tests")),
    ("vue/server-renderer".to_owned(), std::fs::read_to_string(dir.join("server-renderer.js")).expect("the server renderer is vendored for tests")),
    ("@snapfire/fsr-client/vue".to_owned(), "export function useStore(key, initial) { return { value: initial }; }\n".to_owned()),
    ("@snapfire/fsr-client".to_owned(), "export function get() { return 0; }\nexport async function optimistic() {}\n".to_owned()),
    ("@generated/client".to_owned(), "export const actions = {};\n".to_owned()),
    ("@src/store".to_owned(), "export const plannedCount = \"tonight/planned\";\n".to_owned()),
  ])
}

/// What Vue's own server renderer writes for `source` with `props` as JSON.
fn render_vue(compiler: &Compiler, file: &str, source: &str, props: &str, children: &str) -> String {
  let Outcome::Ok(module) = compiler.ssr_module(file, source, &Options::default(), &Default::default()).expect("the driver answers") else { panic!("{file} compiles for the server") };
  let js = match module.lang {
    Lang::Ts => strip_types(&module.js),
    Lang::Js => module.js,
  };
  compiler.render_module(&js, props, Some(children), &runtime()).expect("Vue renders")
}

fn strip_types(source: &str) -> String {
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Custom("component.ts".to_owned())), source.to_owned());
  let lexer = Lexer::new(Syntax::Typescript(TsSyntax::default()), swc_core::ecma::ast::EsVersion::latest(), StringInput::from(&*fm), None);
  let module = Parser::new_from(lexer).parse_module().expect("the server module parses");
  GLOBALS.set(&Globals::new(), || {
    let unresolved = Mark::new();
    let top = Mark::new();
    let program = swc_core::ecma::ast::Program::Module(module);
    let program = program.apply(&mut resolver(unresolved, top, true));
    let program = program.apply(&mut strip(unresolved, top));
    let program = program.apply(&mut (hygiene(), fixer(None)));
    let mut buf = Vec::new();
    {
      let writer = JsWriter::new(cm.clone(), "\n", &mut buf, None);
      let mut emitter = Emitter { cfg: Default::default(), cm: cm.clone(), comments: None, wr: writer };
      emitter.emit_program(&program).expect("emits");
    }
    String::from_utf8(buf).expect("utf8")
  })
}

fn props(pairs: &[(&str, Value)]) -> ValueMap {
  pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

fn json(props: &ValueMap) -> String {
  fn value(v: &Value) -> String {
    match v {
      Value::Null => "null".to_owned(),
      Value::Bool(b) => b.to_string(),
      Value::Int(n) => n.to_string(),
      Value::F64(f) => f.to_string(),
      Value::Str(s) => format!("{:?}", s.as_str()),
      Value::Seq(items) => format!("[{}]", items.iter().map(value).collect::<Vec<_>>().join(",")),
      Value::Map(map) => format!("{{{}}}", map.iter().map(|(k, v)| format!("{k:?}:{}", value(v))).collect::<Vec<_>>().join(",")),
      other => panic!("{other:?}"),
    }
  }
  value(&Value::Map(props.clone()))
}

/// The island's children as a template and as the markup it writes.
fn child(children: &str) -> (Vec<Tmpl>, String) {
  let tmpl = match children {
    "" => Vec::new(),
    "<i>child</i>" => vec![Tmpl::Element { tag: "i".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("child".to_owned())] }],
    text => vec![Tmpl::Text(text.to_owned())],
  };
  (tmpl, children.replace('<', "&lt;").replace('>', "&gt;").replace("&lt;i&gt;child&lt;/i&gt;", "<i>child</i>"))
}

fn agree(compiler: &Compiler, dir: &Path, file: &str, source: &str, props: &ValueMap, children: &str) -> String {
  let set = lower(compiler, dir, file, source).unwrap_or_else(|e| panic!("{file} lowers: {e}"));
  let module = format!("{file}#default");
  let (tmpl, markup) = child(children);
  let rust = render_rust(&set, &module, props, tmpl);
  let vue = render_vue(compiler, file, source, &json(props), &markup);
  let held = format!("<template data-sf-children>{markup}</template>");
  let rust = match rust.strip_suffix(&held) {
    Some(shown) => {
      assert!(!shown.contains("data-sf-children"), "the children are held apart only when the template did not place them: {rust}");
      shown.to_owned()
    }
    None => rust,
  };
  assert_eq!(rust, vue, "\n rust: {rust}\n  vue: {vue}\n");
  rust
}

#[test]
fn the_recipes_components_render_what_vue_renders() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("recipes", &[("src/store.ts", STORE)]);
  let ingredients = Value::seq(vec![
    Value::Map(props(&[("name", Value::str("flour")), ("quantity", Value::F64(1.5)), ("unit", Value::str("cups"))])),
    Value::Map(props(&[("name", Value::str("eggs")), ("quantity", Value::F64(2.0)), ("unit", Value::str(""))])),
  ]);
  let scaler = agree(&compiler, &dir, "src/ui/Scaler.vue", SCALER, &props(&[("serves", Value::F64(4.0)), ("ingredients", ingredients)]), "");
  assert!(scaler.contains("<li data-v-"), "the scope stamp is on every element: {scaler}");
  assert!(scaler.contains("<span class=\"quantity\" data-v-"), "{scaler}");
  assert!(scaler.contains("1.5 cups"), "{scaler}");
  assert!(scaler.contains("serves 4"), "{scaler}");
  assert!(scaler.contains("<!----></p>"), "the note is a `v-if` that rendered nothing: {scaler}");

  let plan = agree(&compiler, &dir, "src/ui/PlanRecipe.vue", PLAN, &props(&[("id", Value::str("r1")), ("planned", Value::Bool(true))]), "");
  assert!(plan.contains("class=\"btn-on btn\""), "the dynamic class comes first, then the static one: {plan}");
  assert!(plan.contains("Planned for tonight"), "{plan}");
  let unplanned = agree(&compiler, &dir, "src/ui/PlanRecipe.vue", PLAN, &props(&[("id", Value::str("r1")), ("planned", Value::Bool(false))]), "");
  assert!(unplanned.contains("class=\"btn\""), "{unplanned}");

  let tonight = agree(&compiler, &dir, "src/ui/Tonight.vue", TONIGHT, &props(&[("count", Value::F64(3.0))]), "Kept in the session cookie. <a href=\"/tonight\">See them</a>.");
  assert!(tonight.contains("3 for tonight"), "{tonight}");
  assert!(tonight.contains("<!---->"), "the panel is closed, so its `v-if` wrote the empty anchor: {tonight}");
  assert!(!tonight.contains("Kept in the session cookie"), "the closed panel shows no children: {tonight}");
  let set = lower(&compiler, &dir, "src/ui/Tonight.vue", TONIGHT).unwrap();
  let whole = render_rust(&set, "src/ui/Tonight.vue#default", &props(&[("count", Value::F64(3.0))]), vec![Tmpl::Text("Kept in the session cookie.".to_owned())]);
  assert!(whole.ends_with("</div><template data-sf-children>Kept in the session cookie.</template>"), "the children are held after the markup for the browser to place when the panel opens: {whole}");
  std::fs::remove_dir_all(&dir).unwrap();
}

const KITCHEN: &str = r#"<script setup lang="ts">
import { computed, ref } from "vue";

interface Row {
  name: string;
  done: boolean;
}

const props = withDefaults(defineProps<{ rows: Row[]; title?: string; note?: string }>(), { title: "Kitchen" });
const open = ref(true);
const remaining = computed(() => props.rows.filter((r) => !r.done).length);
const style = { fontSize: 14, "--gap": "4px" };
</script>

<template>
  <h2 :title="note" :aria-expanded="open" :tabIndex="0">{{ title }}: {{ remaining }} left, {{ open }}</h2>
  <section :style="style" style="color: red" v-show="open">
    <template v-if="rows.length === 0">
      <p>nothing</p>
      <p>at all</p>
    </template>
    <template v-else-if="remaining === 0">done</template>
    <ul v-else>
      <template v-for="(row, i) in rows" :key="row.name">
        <li :class="[row.done ? 'done' : '', { first: i === 0 }]" :hidden="row.done" :data-i="i">{{ row.name }}</li>
      </template>
    </ul>
    <input type="checkbox" :checked="open" disabled />
    <p v-html="'<b>raw</b>'"></p>
    <p v-text="`text ${rows.length}`"></p>
    <slot />
  </section>
</template>
"#;

#[test]
fn the_directives_of_the_first_cut_render_what_vue_renders() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("kitchen", &[]);
  let rows = |done: &[bool]| Value::seq(done.iter().enumerate().map(|(i, d)| Value::Map(props(&[("name", Value::str(format!("row {i} & <co>"))), ("done", Value::Bool(*d))]))).collect::<Vec<Value>>());
  let html = agree(&compiler, &dir, "src/ui/Kitchen.vue", KITCHEN, &props(&[("rows", rows(&[false, true]))]), "<i>child</i>");
  assert!(html.starts_with("<!--[--><h2 aria-expanded=\"true\" tabIndex=\"0\">Kitchen: 1 left, true</h2>"), "a multi-root component is a fragment, a null attribute is absent and a boolean prints its word: {html}");
  assert!(html.contains("style=\"font-size:14;--gap:4px;color:red;\""), "{html}");
  assert!(html.contains("<li class=\"first\" data-i=\"0\">row 0 &amp; &lt;co&gt;</li><li class=\"done\" hidden data-i=\"1\">"), "{html}");
  assert!(html.contains("<input type=\"checkbox\" checked disabled>"), "a void element closes without a slash: {html}");
  assert!(html.contains("tabIndex=\"0\""), "a bound name that is no boolean is written as given: {html}");
  assert!(html.contains("<p><b>raw</b></p><p>text 2</p><!--[--><sf-s data-sf-children><i>child</i></sf-s><!--]-->"), "{html}");
  agree(&compiler, &dir, "src/ui/Kitchen.vue", KITCHEN, &props(&[("rows", rows(&[])), ("title", Value::str("Empty")), ("note", Value::str("a \"note\""))]), "");
  agree(&compiler, &dir, "src/ui/Kitchen.vue", KITCHEN, &props(&[("rows", rows(&[true, true]))]), "");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_lowered_component_is_marked_for_the_vue_adapter_with_its_state() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("shape", &[("src/store.ts", STORE)]);
  let set = lower(&compiler, &dir, "src/ui/Tonight.vue", TONIGHT).unwrap();
  let (_, component) = set.components.iter().find(|(m, _)| m == "src/ui/Tonight.vue#default").unwrap();
  assert_eq!(component.owner, Owner::Vue);
  assert_eq!(component.state, vec!["held".to_owned(), "open".to_owned()]);
  assert!(set.foreign.is_empty(), "a lowered component is not foreign: {:?}", set.foreign);
  let plan = serde_json::to_string(component).unwrap();
  assert!(plan.contains("\"owner\":\"vue\""), "{plan}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn what_the_first_cut_does_not_read_is_residue_that_names_its_line() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("residue", &[]);
  let refused = |tag: &str, source: &str| -> String {
    let file = format!("src/ui/{tag}.vue");
    match lower(&compiler, &dir, &file, source) {
      Err(LowerError::Residue(residue)) => format!("{}:{}: {}", residue.line, residue.column, residue.message),
      Err(other) => panic!("{tag} should be residue: {other}"),
      Ok(set) => panic!("{tag} should be residue: it lowered to {:?}", set.components.iter().map(|(m, _)| m).collect::<Vec<_>>()),
    }
  };
  assert_eq!(refused("Model", "<script setup>\nimport { ref } from \"vue\";\nconst text = ref(\"\");\n</script>\n<template><div v-model=\"text\" /></template>\n"), "5:16: `v-model` on `<div>`", "the directive's own position, not its expression's");
  assert_eq!(refused("Named", "<template><div><slot :[which]=\"1\" /></div></template>\n"), "1:22: a slot prop whose name is an expression");
  assert_eq!(refused("Nested", "<script setup>\nimport Row from \"./Row.vue\";\n</script>\n<template><Row /></template>\n"), "4:11: `Row` comes from `./Row.vue`, which the build cannot follow", "a child that is not there");
  assert!(refused("Options", "<script>\nexport default { data() { return {}; } };\n</script>\n<template><p /></template>\n").ends_with("a `<script>` without `setup`"));
  assert_eq!(refused("Inject", "<script setup>\nimport { inject } from \"vue\";\nconst theme = inject(\"theme\", () => \"x\", true);\n</script>\n<template><p>{{ theme }}</p></template>\n"), "3:31: `inject` with a default factory");
  let bare = refused("Bare", "<template><p>{{ nowhere }}</p></template>\n");
  assert!(bare.starts_with("1:17: `nowhere` is not bound here"), "{bare}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_page_placing_a_described_component_that_does_not_lower_keeps_it_foreign_and_says_why() {
  let compiler = Compiler::new().expect("the compiler boots");
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Box from \"@src/ui/Box.vue\";\nexport default function Page() {\n  return <main><Island><Box /></Island></main>;\n}\n";
  let dir = app("page", &[("routes/page.tsx", page), ("src/ui/Box.vue", "<script setup>\nimport { ref } from \"vue\";\nconst text = ref(\"\");\n</script>\n<template><div v-model=\"text\" /></template>\n")]);
  let mut set = ComponentSet::new(&dir);
  set.describe("src/ui/Box.vue", describe(&compiler, "src/ui/Box.vue", &std::fs::read_to_string(dir.join("src/ui/Box.vue")).unwrap()));
  set.lower("routes/page.tsx#default").expect("the page lowers with the box foreign");
  assert_eq!(set.foreign, vec!["src/ui/Box.vue#default".to_owned()]);
  let (module, residue) = &set.foreign_residue[0];
  assert_eq!(module, "src/ui/Box.vue#default");
  assert_eq!((residue.line, residue.message.as_str()), (5, "`v-model` on `<div>`"));
  assert!(!set.components.iter().any(|(m, _)| m == "src/ui/Box.vue#default"));
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_client_adapters_placements_lower_inside_a_vue_template_as_they_do_in_jsx() {
  let compiler = Compiler::new().expect("the compiler boots");
  let source = "<script setup lang=\"ts\">\nimport { Link, Mount, Picture, useLocale } from \"@snapfire/fsr-client/vue\";\nconst props = defineProps<{ nest: string[] }>();\nconst locale = useLocale();\n</script>\n<template>\n  <section>\n    <p class=\"locale\">{{ locale }}</p>\n    <Link href=\"/next\" class=\"next\">next</Link>\n    <Picture src=\"/pictures/probe.png\" alt=\"probe\" :width=\"4\" :height=\"4\" />\n    <Mount v-for=\"module in props.nest\" :key=\"module\" :module=\"module\" :props=\"{ label: 'nested' }\" />\n  </section>\n</template>\n";
  let dir = app("placements", &[]);
  let set = lower(&compiler, &dir, "src/ui/Probe.vue", source).unwrap_or_else(|e| panic!("the placements lower: {e}"));
  let (_, component) = set.components.iter().find(|(m, _)| m == "src/ui/Probe.vue#default").expect("lowered, not foreign");
  assert_eq!(component.owner, Owner::Vue);
  let html = render_rust(&set, "src/ui/Probe.vue#default", &props(&[("nest", Value::seq(vec![Value::str("a.tsx#default"), Value::str("b.vue#default")]))]), Vec::new());
  assert!(html.contains("<a href=\"/next\" class=\"next\" data-sf-link=\"exact\">next</a>"), "a link carries the navigator's marks: {html}");
  assert!(html.contains("<img src=\"/pictures/probe.png\" alt=\"probe\" width=\"4\" height=\"4\" loading=\"lazy\" decoding=\"async\">"), "a picture of a string source is an image: {html}");
  assert_eq!(html.matches("<sf-s data-sf-island=\"\"></sf-s>").count(), 2, "one empty region per mount, which the adapter fills: {html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_vue_components_handlers_lower_to_steps_that_write_its_refs_and_the_store() {
  use snapfire_fsr_ir::render::{HandlerRef, HANDLER_ATTR, UNLOWERED_ATTR};
  let source = r#"<script setup lang="ts">
import { ref } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";

const n = ref(0);
const count = useStore("probe/count", 0);
const emit = defineEmits<{ (e: "x"): void }>();
function bump(by: number) {
  n.value += by;
  count.value = count.value + n.value;
}
const cap = () => {
  if (n.value > 1) return;
  count.value = 10;
};
</script>

<template>
  <div>
    <p>{{ n }} {{ count.value }}</p>
    <button class="inc" @click="n++">inc</button>
    <button class="bump" @click="bump(2)">bump</button>
    <button class="cap" @click.prevent="cap">cap</button>
    <button class="arrow" @click="(e) => { n = 7; count.value = n * 2 }">arrow</button>
    <input class="key" @keyup.enter="n = 0" />
    <button class="emit" @click="emit('x')">emit</button>
  </div>
</template>
"#;
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("handlers", &[]);
  let set = lower(&compiler, &dir, "src/ui/Steps.vue", source).unwrap();
  let module = "src/ui/Steps.vue#default";
  let (_, component) = set.components.iter().find(|(m, _)| m == module).unwrap();
  assert_eq!(component.handlers.len(), 4, "{:?}", component.handlers);
  assert!(component.handlers.iter().all(|h| h.event == "click"));
  let Tmpl::Element { children, .. } = &component.render else { panic!("{:?}", component.render) };
  let unlowered: Vec<String> = children
    .iter()
    .filter_map(|c| match c {
      Tmpl::Element { attrs, .. } => attrs.iter().find_map(|a| match a {
        Entry::Field(name, Expr::Lit(snapfire_fsr_ir::ast::Lit::Str(why))) if name == UNLOWERED_ATTR => Some(why.clone()),
        _ => None,
      }),
      _ => None,
    })
    .collect();
  assert_eq!(unlowered.len(), 2, "{unlowered:?}");
  assert!(unlowered[0].contains("`.enter` on `@keyup`"), "{unlowered:?}");
  assert!(unlowered[1].contains("a call to `emit`"), "{unlowered:?}");
  let bound = children.iter().filter(|c| matches!(c, Tmpl::Element { attrs, .. } if attrs.iter().any(|a| matches!(a, Entry::Field(n, _) if n.starts_with(HANDLER_ATTR))))).count();
  assert_eq!(bound, 4);

  let library: Components = set.components.iter().map(|(m, c)| (m.clone(), Arc::new(snapfire_fsr_ir::render::prepare(c)))).collect();
  let interpreter = Interpreter::default().with_frameworks(Frameworks { react: None, vue: Some(VueMajor::V3) });
  let step = |n: f64, count: f64, index: usize| {
    let mut props = ValueMap::default();
    props.insert("$store".to_owned(), Value::Map([("probe/count".to_owned(), Value::F64(count))].into_iter().collect()));
    let state: ValueMap = [("n".to_owned(), Value::F64(n))].into_iter().collect();
    interpreter.island_step(module, component, &props, &state, Some(HandlerRef::own(index)), &Value::Null, &library).unwrap()
  };
  let number = |v: Option<&Value>| match v {
    Some(Value::Int(i)) => *i as f64,
    Some(Value::F64(f)) => *f,
    other => panic!("{other:?}"),
  };
  let inc = step(1.0, 5.0, 0);
  assert_eq!(number(inc.state.get("n")), 2.0);
  assert!(inc.store.is_empty(), "{:?}", inc.store);
  let bump = step(1.0, 5.0, 1);
  assert_eq!(number(bump.state.get("n")), 3.0);
  assert_eq!(number(bump.store.get("probe/count")), 8.0, "the store write read `n` after the write before it");
  assert!(bump.rendered.html.contains("<p>3 8</p>"), "{}", bump.rendered.html);
  let capped = step(0.0, 5.0, 2);
  assert_eq!(number(capped.store.get("probe/count")), 10.0);
  let passed = step(2.0, 5.0, 2);
  assert!(passed.store.get("probe/count").is_none_or(|v| number(Some(v)) == 5.0), "an early return leaves the store: {:?}", passed.store);
  let arrow = step(0.0, 0.0, 3);
  assert_eq!(number(arrow.state.get("n")), 7.0);
  assert_eq!(number(arrow.store.get("probe/count")), 14.0);
  std::fs::remove_dir_all(&dir).unwrap();
}

const METHODS: &str = r#"<script setup lang="ts">
const props = defineProps<{ items: { name: string; price: number }[]; nums: number[]; word: string; missing?: string[] }>();
const byPrice = props.items.toSorted((a, b) => a.price - b.price).map((i) => i.name).join();
const big = props.nums.filter((n) => {
  const limit = 2;
  return n > limit;
});
</script>

<template>
  <p>{{ nums.slice(1, 3).join() }} {{ nums.slice(-2).join() }} {{ word.slice(1, -1) }} {{ word.slice(-3) }} {{ word.slice(4, 2) }}</p>
  <p>{{ nums.at(-1) }} {{ word.at(0) }} {{ nums.at(10) ?? "none" }} {{ word.at(-1) }}</p>
  <p>{{ nums.indexOf(3) }} {{ nums.indexOf(99) }} {{ word.indexOf("l") }} {{ word.indexOf("l", 3) }} {{ word.indexOf("") }}</p>
  <p>{{ nums.concat([7, 8], 9).join() }} {{ word.concat("!", 1) }}</p>
  <p>{{ nums.toReversed().join() }} {{ [...nums].reverse().join() }} {{ nums.join() }}</p>
  <p>{{ word.padStart(8, "*") }}|{{ word.padEnd(8) }}|{{ "5".padStart(3, "0") }}|{{ word.padStart(9, "ab") }}|{{ word.padStart(2) }}</p>
  <p>{{ word.substring(3, 1) }} {{ word.substring(2) }} {{ word.substring(-4, 99) }}</p>
  <p>{{ nums.toSorted().join() }} {{ byPrice }} {{ [...nums].sort((a, b) => b - a).join() }} {{ nums.slice().sort().join() }}</p>
  <p>{{ items.flatMap((i) => [i.name, i.price]).join() }} {{ nums.flatMap((n) => (n > 3 ? [] : n)).join() }}</p>
  <p>{{ 2 ** 10 }} {{ Math.pow(2, 0.5).toFixed(3) }} {{ Math.sqrt(16) }} {{ Math.trunc(-4.7) }} {{ Math.sign(-3) }} {{ Math.sign(0) }}</p>
  <p>{{ Math.max(...nums) }} {{ Math.min(...nums, 0) }}</p>
  <p>{{ JSON.stringify(items) }}</p>
  <pre>{{ JSON.stringify(items[0], null, 2) }}</pre>
  <p>{{ JSON.stringify({ a: 'q"\n', b: null, c: [1.5, true], d: [], e: {} }) }} {{ JSON.stringify(word) }} {{ JSON.stringify(nums, null, "--") }}</p>
  <p>{{ missing?.join("-") ?? "none" }} {{ items?.map((i) => i.name).join() }}</p>
  <p>{{ big.join() }}</p>
  <p class="callbacks">{{ nums.concat(nums).filter((n, i, all) => all.indexOf(n) === i).join() }} {{ nums.reduce((acc, n, i) => acc + n * i, 0) }} {{ nums.some((n, i) => i === 3 && n === 3) }} {{ nums.every((n, i, all) => all.length === 4 && i < 4) }} {{ nums.map((n, i, all) => all[all.length - 1 - i]).join() }}</p>
</template>
"#;

#[test]
fn the_array_and_string_methods_render_what_javascript_computes() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("methods", &[]);
  let item = |name: &str, price: f64| Value::Map(props(&[("name", Value::str(name)), ("price", Value::F64(price))]));
  let given = props(&[
    ("items", Value::seq(vec![item("pear", 3.0), item("fig", 1.5), item("kiwi", 3.0), item("plum", 0.5)])),
    ("nums", Value::seq(vec![Value::F64(10.0), Value::F64(9.0), Value::F64(1.0), Value::F64(3.0)])),
    ("word", Value::str("hello")),
  ]);
  let html = agree(&compiler, &dir, "src/ui/Methods.vue", METHODS, &given, "");
  assert!(html.contains("<p>1,10,3,9 plum,fig,pear,kiwi 10,9,3,1 1,10,3,9</p>"), "the default order is by text and a comparator's sort is stable: {html}");
  assert!(html.contains("<p class=\"callbacks\">10,9,1,3 20 true true 3,1,9,10</p>"), "a callback gets the index and the array: {html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

const PATTERNS: &str = r#"<script setup lang="ts">
const props = defineProps<{ items: { name: string; price: number; extra?: { note?: string } }[]; nums: number[] }>();
const { name, price: cost, extra: { note = "none" } = {} } = props.items[0];
const [first, , third = 0, ...others] = props.nums;
const { items: [, second], ...restOf } = props;
const totals = props.items.map(({ name: label, price = 0, extra: { note: inner = "-" } = {} }) => `${label}:${price}:${inner}`);
const pairs = props.nums.map((n, i) => [n, i]).map(([n, i]) => n * i);
const a = 1, b = a + 1;
</script>

<template>
  <p class="object">{{ name }} {{ cost }} {{ note }}</p>
  <p class="array">{{ first }} {{ third }} {{ others.join() }}</p>
  <p class="nested">{{ second.name }} {{ Object.keys(restOf).join() }}</p>
  <p class="params">{{ totals.join(" ") }} {{ pairs.join() }}</p>
  <p class="many">{{ a }} {{ b }}</p>
</template>
"#;

#[test]
fn destructuring_renders_what_javascript_binds() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("patterns", &[]);
  let item = |name: &str, price: Option<f64>, note: Option<&str>| {
    let mut fields = vec![("name", Value::str(name))];
    if let Some(price) = price {
      fields.push(("price", Value::F64(price)));
    }
    if let Some(note) = note {
      fields.push(("extra", Value::Map(props(&[("note", Value::str(note))]))));
    }
    Value::Map(props(&fields))
  };
  let given = props(&[
    ("items", Value::seq(vec![item("pear", Some(3.0), None), item("fig", None, Some("dried")), item("kiwi", Some(2.0), Some("ripe"))])),
    ("nums", Value::seq(vec![Value::F64(4.0), Value::F64(5.0), Value::F64(6.0), Value::F64(7.0), Value::F64(8.0)])),
  ]);
  let html = agree(&compiler, &dir, "src/ui/Patterns.vue", PATTERNS, &given, "");
  assert!(html.contains("<p class=\"object\">pear 3 none</p>"), "{html}");
  assert!(html.contains("<p class=\"array\">4 6 7,8</p>"), "{html}");
  assert!(html.contains("<p class=\"params\">pear:3:- fig:0:dried kiwi:2:ripe 0,5,12,21,32</p>"), "{html}");
  let short = props(&[("items", Value::seq(vec![item("pear", Some(3.0), None), item("fig", None, None)])), ("nums", Value::seq(vec![Value::F64(4.0)]))]);
  let html = agree(&compiler, &dir, "src/ui/Patterns.vue", PATTERNS, &short, "");
  assert!(html.contains("<p class=\"array\">4 0 </p>"), "a missing item takes its default and the rest is empty: {html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

const STATEMENTS: &str = r#"<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ stock: number; kind: string; scores: number[] }>();
const label = computed(() => {
  let out = "Buy";
  if (props.stock === 0) out = "Sold out";
  else if (props.stock < 3) {
    const left = props.stock;
    out = "Last " + left;
  }
  switch (props.kind) {
    case "a":
      out += "!";
      break;
    case "b":
    case "c":
      out = out.toUpperCase();
      break;
    default:
      out = out + "?";
  }
  let n = 0;
  n++;
  n += 2;
  const tags: string[] = [];
  tags.push("x");
  tags.push(...["y", "z"]);
  tags.unshift("w");
  return `${out} ${n} ${tags.join()}`;
});
const grades = props.scores.map((score) => {
  if (score > 90) return "A";
  else if (score > 80) {
    const b = "B";
    return b;
  }
  switch (Math.floor(score / 10)) {
    case 7:
      return "C";
    case 6:
    case 5:
      return "D";
  }
  return "F";
});
</script>

<template>
  <p class="label">{{ label }}</p>
  <p class="grades">{{ grades.join() }}</p>
</template>
"#;

#[test]
fn statements_render_what_javascript_runs() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("statements", &[]);
  let scores = Value::seq(vec![Value::F64(95.0), Value::F64(85.0), Value::F64(75.0), Value::F64(55.0), Value::F64(10.0)]);
  for (stock, kind, want) in [(0.0, "a", "Sold out! 3 w,x,y,z"), (2.0, "b", "LAST 2 3 w,x,y,z"), (9.0, "z", "Buy? 3 w,x,y,z"), (1.0, "c", "LAST 1 3 w,x,y,z")] {
    let given = props(&[("stock", Value::F64(stock)), ("kind", Value::str(kind)), ("scores", scores.clone())]);
    let html = agree(&compiler, &dir, "src/ui/Statements.vue", STATEMENTS, &given, "");
    assert!(html.contains(&format!("<p class=\"label\">{want}</p>")), "{html}");
    assert!(html.contains("<p class=\"grades\">A,B,C,D,F</p>"), "{html}");
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

const COERCED: &str = r#"<script setup lang="ts">
const props = defineProps<{ s: string; e: string; junk: string; hex: string; inf: string; sci: string; t: boolean; f: boolean; n: null; x: number; k: number }>();
const product = props.s * 2;
const parsed = [Number(props.e), Number(props.junk), Number(props.hex), Number(props.inf), Number(props.sci), Number(props.n), Number(props.t)];
</script>

<template>
  <p class="mixed">{{ product }} {{ x - s }} {{ s - 1 }} {{ s / 2 }} {{ t + 1 }} {{ f * 5 }} {{ n + 1 }} {{ t + t }} {{ junk * 2 }} {{ e - 1 }} {{ hex % 7 }} {{ s ** 2 }}</p>
  <p class="text">{{ s + 1 }} {{ 1 + s }} {{ t + s }} {{ n + s }}</p>
  <p class="number">{{ parsed.join() }} {{ inf * 1 }} {{ 1 / 0 }}</p>
  <p class="compare">{{ x > s }} {{ s < 10 }} {{ x === s }} {{ x !== s }} {{ n < 1 }} {{ junk < 1 }} {{ junk >= 1 }} {{ t > f }} {{ k > 2.5 }} {{ k === 3 }} {{ k !== 3 }} {{ n === 0 }}</p>
</template>
"#;

#[test]
fn mixed_operands_coerce_and_compare_as_javascript_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("coerced", &[]);
  let given = props(&[
    ("s", Value::str(" 3 ")),
    ("e", Value::str("")),
    ("junk", Value::str("abc")),
    ("hex", Value::str("0x1f")),
    ("inf", Value::str("-Infinity")),
    ("sci", Value::str("1.5e3")),
    ("t", Value::Bool(true)),
    ("f", Value::Bool(false)),
    ("n", Value::Null),
    ("x", Value::F64(10.0)),
    ("k", Value::Int(3)),
  ]);
  let html = agree(&compiler, &dir, "src/ui/Coerced.vue", COERCED, &given, "");
  assert!(html.contains("<p class=\"mixed\">6 7 2 1.5 2 0 1 2 NaN -1 3 9</p>"), "{html}");
  assert!(html.contains("<p class=\"text\"> 3 1 1 3  true 3  null 3 </p>"), "{html}");
  assert!(html.contains("<p class=\"number\">0,NaN,31,-Infinity,1500,0,1 -Infinity Infinity</p>"), "{html}");
  assert!(html.contains("<p class=\"compare\">true true false true true false false true true true false false</p>"), "{html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

const MADE: &str = r#"<script setup lang="ts">
const props = defineProps<{ at: number; iso: string; rows: { id: string; n: number }[] }>();
const when = new Date(props.at);
const parsed = new Date(props.iso);
const parts = [when.getUTCFullYear(), when.getUTCMonth(), when.getUTCDate(), when.getUTCDay(), when.getUTCHours(), when.getUTCMinutes(), when.getUTCSeconds(), when.getUTCMilliseconds()];
const byId = new Map(props.rows.map((r) => [r.id, r.n]));
const seen = new Set(props.rows.map((r) => r.n));
const counts = Object.fromEntries(props.rows.map((r) => [r.id, r.n * 2]));
</script>

<template>
  <p class="date">{{ when.toISOString() }} {{ when.getTime() }} {{ parts.join() }} {{ parsed.getTime() }} {{ Date.parse(iso) }}</p>
  <p class="map">{{ byId.get("b") }} {{ byId.has("a") }} {{ byId.has("z") }} {{ byId.size }} {{ [...byId.keys()].join() }} {{ [...byId.values()].join() }} {{ [...byId].map(([k, v]) => k + v).join() }}</p>
  <p class="set">{{ seen.has(2) }} {{ seen.has(9) }} {{ seen.size }} {{ [...seen].join() }} {{ Array.from(seen).join() }}</p>
  <p class="rest">{{ counts.a }} {{ (42).toString() }} {{ Array.from(byId).length }}</p>
</template>
"#;

#[test]
fn dates_maps_and_sets_render_what_javascript_makes() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("made", &[]);
  let row = |id: &str, n: f64| Value::Map(props(&[("id", Value::str(id)), ("n", Value::F64(n))]));
  let given = props(&[
    ("at", Value::F64(1_791_199_808_333.0)),
    ("iso", Value::str("2026-02-28T23:59:59.5Z")),
    ("rows", Value::seq(vec![row("a", 1.0), row("b", 2.0), row("c", 2.0), row("a", 3.0)])),
  ]);
  let html = agree(&compiler, &dir, "src/ui/Made.vue", MADE, &given, "");
  assert!(html.contains("2026-10-05T"), "{html}");
  assert!(html.contains("<p class=\"map\">2 true false 3 a,b,c 3,2,2 a3,b2,c2</p>"), "a later key replaces an earlier one in place: {html}");
  assert!(html.contains("<p class=\"set\">true false 3 1,2,3 1,2,3</p>"), "{html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

const PATTERNS_RE: &str = r#"<script setup lang="ts">
const props = defineProps<{ title: string; csv: string; code: string }>();
const SLUG = /[^a-z0-9]+/g;
const slug = props.title.toLowerCase().replace(SLUG, "-").replace(/^-|-$/g, "");
const caps = props.title.replace(/\b\w/g, (c) => c.toUpperCase());
const swapped = props.code.replace(/(\w+)-(\d+)/, "$2:$1 [$&] <$`|$'> $$");
const named = props.code.replace(/(?<word>[a-z]+)/, "<$<word>>");
const parts = props.csv.split(/\s*,\s*/);
const kept = props.csv.split(/(,)/);
const found = props.code.match(/(\w+)-(\d+)/);
const all = props.csv.match(/\d+/g);
const every = [...props.code.matchAll(/([a-z])(\d)?/g)].map((m) => m[1] + (m[2] ?? "_"));
const offsets = props.title.replace(/o/g, (m, at) => `${at}`);
</script>

<template>
  <p class="slug">{{ slug }}</p>
  <p class="caps">{{ caps }}</p>
  <p class="swapped">{{ swapped }}</p>
  <p class="named">{{ named }}</p>
  <p class="split">{{ parts.join("|") }} {{ kept.join("|") }} {{ "a1b22c".split(/\d+/).join("|") }} {{ "abc".split(/(?:)/).join("|") }}</p>
  <p class="match">{{ found ? found.join("/") : "none" }} {{ all ? all.join("/") : "none" }} {{ every.join() }} {{ "xyz".match(/q/) ?? "none" }}</p>
  <p class="test">{{ /^\d{3}$/.test(code) }} {{ /zz/i.test("aZz") }} {{ code.search(/\d/) }} {{ "café latte".search(/latte/) }}</p>
  <p class="all">{{ csv.replaceAll(",", ";") }} {{ csv.replaceAll(/\s/g, "") }} {{ "a.b.c".replaceAll(".", (m) => "[" + m + "]") }}</p>
  <p class="offsets">{{ offsets }}</p>
  <p class="dots">{{ "a\nb".replace(/a.b/, "x") }} {{ "a\nb".replace(/a.b/s, "x") }} {{ "Ünï x9".replace(/\w/g, "*") }}</p>
</template>
"#;

#[test]
fn regular_expressions_render_what_javascript_matches() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("regex", &[]);
  let given = props(&[("title", Value::str("Hello, World of FSR!")), ("csv", Value::str("1, 22 ,333,4")), ("code", Value::str("ab-12 cd"))]);
  let html = agree(&compiler, &dir, "src/ui/Patterns.vue", PATTERNS_RE, &given, "");
  assert!(html.contains("<p class=\"slug\">hello-world-of-fsr</p>"), "{html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

/// Lowers `root` with every file in `files` described, renders it in Rust and with Vue's server renderer, each child's server module given under the specifier the parent imports it by, and compares the two.
fn agree_tree(compiler: &Compiler, dir: &Path, files: &[(&str, &str, &str)], root: &str, props: &ValueMap) -> String {
  let mut set = ComponentSet::new(dir);
  for (file, _, source) in files {
    set.describe(*file, describe(compiler, file, source));
  }
  set.lower(&format!("{root}#default")).unwrap_or_else(|e| panic!("{root} lowers: {e}"));
  let rust = render_rust(&set, &format!("{root}#default"), props, Vec::new());
  let mut modules = runtime();
  for (file, specifier, source) in files.iter().filter(|(f, _, _)| *f != root) {
    let Outcome::Ok(module) = compiler.ssr_module(file, source, &Options::default(), &Default::default()).expect("the driver answers") else { panic!("{file} compiles for the server") };
    let js = match module.lang {
      Lang::Ts => strip_types(&module.js),
      Lang::Js => module.js,
    };
    modules.insert((*specifier).to_owned(), js);
  }
  let (_, _, source) = files.iter().find(|(f, _, _)| *f == root).expect("the root is among the files");
  let Outcome::Ok(module) = compiler.ssr_module(root, source, &Options::default(), &Default::default()).expect("the driver answers") else { panic!("{root} compiles for the server") };
  let js = match module.lang {
    Lang::Ts => strip_types(&module.js),
    Lang::Js => module.js,
  };
  let vue = compiler.render_module(&js, &json(props), Some(""), &modules).expect("Vue renders");
  let held = "<template data-sf-children></template>";
  let rust = rust.strip_suffix(held).map(str::to_owned).unwrap_or(rust);
  assert_eq!(rust, vue, "\n rust: {rust}\n  vue: {vue}\n");
  rust
}

const CARD: &str = r#"<script setup lang="ts">
import { ref } from "vue";
const props = defineProps<{ title: string; itemCount: number; tags?: string[] }>();
const open = ref(false);
</script>

<template>
  <article class="card">
    <h3>{{ title }} ({{ itemCount }})</h3>
    <ul v-if="tags"><li v-for="tag in tags" :key="tag">{{ tag }}</li></ul>
    <slot />
    <p v-if="open">open</p>
  </article>
</template>
"#;

const PAIR: &str = r#"<script setup lang="ts">
defineProps<{ left: string; right: string }>();
</script>

<template>
  <b>{{ left }}</b>
  <i>{{ right }}</i>
</template>
"#;

const BOARD: &str = r#"<script setup lang="ts">
import Card from "./Card.vue";
import Pair from "./Pair.vue";
const props = defineProps<{ rows: { name: string; n: number; tags: string[] }[] }>();
</script>

<template>
  <section class="board">
    <Card v-for="row in rows" :key="row.name" :title="row.name" :item-count="row.n" :tags="row.tags" @select="() => {}">
      <em>{{ row.name }} inside</em>
    </Card>
    <card title="plain" :item-count="0" />
    <Pair left="a" right="b" />
  </section>
</template>
"#;

#[test]
fn a_vue_child_renders_inline_as_vue_renders_it() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("children", &[("src/ui/Board.vue", BOARD), ("src/ui/Card.vue", CARD), ("src/ui/Pair.vue", PAIR)]);
  let row = |name: &str, n: f64, tags: &[&str]| Value::Map(props(&[("name", Value::str(name)), ("n", Value::F64(n)), ("tags", Value::seq(tags.iter().map(|t| Value::str(*t)).collect::<Vec<_>>()))]));
  let given = props(&[("rows", Value::seq(vec![row("pear", 2.0, &["green"]), row("fig", 1.0, &[])]))]);
  let files = [("src/ui/Board.vue", "", BOARD), ("src/ui/Card.vue", "Card.vue", CARD), ("src/ui/Pair.vue", "Pair.vue", PAIR)];
  let html = agree_tree(&compiler, &dir, &files, "src/ui/Board.vue", &given);
  assert!(html.contains("<h3>pear (2)</h3>"), "{html}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_scoped_parent_stamps_its_id_on_a_child_root_as_vue_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let parent = "<script setup lang=\"ts\">\nimport Pill from \"./Pill.vue\";\n</script>\n\n<template>\n  <div class=\"row\"><Pill label=\"x\" /></div>\n</template>\n\n<style scoped>\n.row { color: red; }\n</style>\n";
  let pill = "<script setup lang=\"ts\">\ndefineProps<{ label: string }>();\n</script>\n\n<template>\n  <span class=\"pill\"><b>{{ label }}</b></span>\n</template>\n\n<style scoped>\n.pill { color: blue; }\n</style>\n";
  let dir = app("scoped_children", &[("src/ui/Row.vue", parent), ("src/ui/Pill.vue", pill)]);
  let files = [("src/ui/Row.vue", "", parent), ("src/ui/Pill.vue", "Pill.vue", pill)];
  agree_tree(&compiler, &dir, &files, "src/ui/Row.vue", &ValueMap::default());
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_scoped_parent_placing_a_child_of_several_roots_renders_as_vue_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let parent = "<script setup lang=\"ts\">\nimport Pair from \"./Pair.vue\";\n</script>\n\n<template>\n  <div class=\"row\"><Pair left=\"a\" right=\"b\" /></div>\n</template>\n\n<style scoped>\n.row { color: red; }\n</style>\n";
  let dir = app("scoped_fragment", &[("src/ui/Row.vue", parent), ("src/ui/Pair.vue", PAIR)]);
  let files = [("src/ui/Row.vue", "", parent), ("src/ui/Pair.vue", "Pair.vue", PAIR)];
  agree_tree(&compiler, &dir, &files, "src/ui/Row.vue", &ValueMap::default());
  std::fs::remove_dir_all(&dir).unwrap();
}

const FORM: &str = r#"<script setup lang="ts">
import { ref } from "vue";
const props = defineProps<{ name: string; agree: boolean; picks: string[]; size: string; count: number; note: string }>();
const name = ref(props.name);
const agree = ref(props.agree);
const picks = ref(props.picks);
const size = ref(props.size);
const count = ref(props.count);
const note = ref(props.note);
</script>

<template>
  <form>
    <input class="name" v-model="name" />
    <input type="number" v-model.number="count" />
    <input type="checkbox" class="agree" v-model="agree" />
    <input type="checkbox" value="pear" v-model="picks" />
    <input type="checkbox" value="fig" v-model="picks" />
    <input type="radio" value="s" v-model="size" />
    <input type="radio" :value="'l'" v-model="size" />
    <textarea v-model="note"></textarea>
    <select v-model="size"><option value="s">Small</option><option>l</option></select>
    <select v-model="picks" multiple><option value="pear">Pear</option><option value="fig">Fig</option></select>
  </form>
</template>
"#;

#[test]
fn v_model_renders_the_value_vue_writes() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("form", &[]);
  for (agreed, picks, size) in [(true, vec!["fig"], "l"), (false, vec![], "s")] {
    let given = props(&[
      ("name", Value::str("Ada \"A\"")),
      ("agree", Value::Bool(agreed)),
      ("picks", Value::seq(picks.iter().map(|p| Value::str(*p)).collect::<Vec<_>>())),
      ("size", Value::str(size)),
      ("count", Value::F64(3.0)),
      ("note", Value::str("a <note>")),
    ]);
    agree(&compiler, &dir, "src/ui/Form.vue", FORM, &given, "");
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

const FIELD: &str = r#"<script setup lang="ts">
const model = defineModel<string>();
const title = defineModel<string>("title", { default: "Untitled" });
</script>

<template>
  <label><span>{{ title }}</span><input v-model="model" /></label>
</template>
"#;

const EDITOR: &str = r#"<script setup lang="ts">
import { ref } from "vue";
import Field from "./Field.vue";
const props = defineProps<{ draft: string; heading?: string }>();
const text = ref(props.draft);
const title = ref(props.heading);
</script>

<template>
  <div><Field v-model="text" /><Field v-model="text" v-model:title="title" /></div>
</template>
"#;

#[test]
fn v_model_on_a_child_and_define_model_in_it_render_as_vue_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("models", &[("src/ui/Editor.vue", EDITOR), ("src/ui/Field.vue", FIELD)]);
  let files = [("src/ui/Editor.vue", "", EDITOR), ("src/ui/Field.vue", "Field.vue", FIELD)];
  for heading in [None, Some("Notes")] {
    let mut given = props(&[("draft", Value::str("hello"))]);
    if let Some(heading) = heading {
      given.insert("heading".to_owned(), Value::str(heading));
    }
    let html = agree_tree(&compiler, &dir, &files, "src/ui/Editor.vue", &given);
    assert!(html.contains("value=\"hello\""), "{html}");
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

const SPREAD: &str = r#"<script setup lang="ts">
const props = defineProps<{ extra: Record<string, unknown> }>();
</script>

<template>
  <a href="/x" v-bind="extra" title="kept">link</a>
  <input v-bind="{ type: 'checkbox', checked: true, disabled: false }" />
</template>
"#;

#[test]
fn v_bind_of_an_object_renders_its_attributes_as_vue_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("spread", &[]);
  let extra = Value::Map(props(&[("id", Value::str("go")), ("data-n", Value::F64(2.0)), ("title", Value::str("over")), ("hidden", Value::Bool(false)), ("aria-label", Value::str("a \"b\""))]));
  agree(&compiler, &dir, "src/ui/Spread.vue", SPREAD, &props(&[("extra", extra)]), "");
  std::fs::remove_dir_all(&dir).unwrap();
}

const PANEL: &str = r#"<script setup lang="ts">
const props = defineProps<{ rows: { name: string; n: number }[] }>();
</script>

<template>
  <section class="panel">
    <header><slot name="title">Untitled</slot></header>
    <ul>
      <li v-for="(row, i) in rows" :key="row.name"><slot name="row" :row="row" :index="i" label="r">{{ row.name }}</slot></li>
    </ul>
    <slot />
    <footer><slot name="foot" :count="rows.length" /></footer>
  </section>
</template>
"#;

const USES_PANEL: &str = r#"<script setup lang="ts">
import Panel from "./Panel.vue";
const props = defineProps<{ rows: { name: string; n: number }[]; titled: boolean }>();
</script>

<template>
  <div>
    <Panel :rows="rows">
      <template v-if="titled" #title><b>Shelf</b></template>
      <template #row="{ row, index, label }"><i>{{ label }}{{ index }}:{{ row.name }}={{ row.n }}</i></template>
      <p>body</p>
      <template #foot="foot">{{ foot.count }} rows</template>
    </Panel>
    <Panel :rows="rows" />
  </div>
</template>
"#;

#[test]
fn named_and_scoped_slots_render_as_vue_renders_them() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("slots", &[("src/ui/Uses.vue", USES_PANEL), ("src/ui/Panel.vue", PANEL)]);
  let files = [("src/ui/Uses.vue", "", USES_PANEL), ("src/ui/Panel.vue", "Panel.vue", PANEL)];
  let row = |name: &str, n: f64| Value::Map(props(&[("name", Value::str(name)), ("n", Value::F64(n))]));
  for titled in [true, false] {
    let given = props(&[("rows", Value::seq(vec![row("pear", 2.0), row("fig", 5.0)])), ("titled", Value::Bool(titled))]);
    agree_tree(&compiler, &dir, &files, "src/ui/Uses.vue", &given);
  }
  std::fs::remove_dir_all(&dir).unwrap();
}

const BADGE: &str = r#"<script setup lang="ts">
import { inject } from "vue";
const theme = inject("theme", "light");
const unit = inject("unit");
const missing = inject("nobody", "fallback");
</script>

<template>
  <span class="badge">{{ theme }} {{ unit }} {{ missing }}</span>
</template>
"#;

const THEMED: &str = r#"<script setup lang="ts">
import { provide, ref } from "vue";
import Badge from "./Badge.vue";
const props = defineProps<{ mode: string }>();
const mode = ref(props.mode);
provide("theme", mode);
provide("unit", "kg");
</script>

<template>
  <div><Badge /></div>
  <p>second root</p>
</template>
"#;

#[test]
fn provide_and_inject_render_as_vue_does() {
  let compiler = Compiler::new().expect("the compiler boots");
  let dir = app("inject", &[("src/ui/Themed.vue", THEMED), ("src/ui/Badge.vue", BADGE)]);
  let files = [("src/ui/Themed.vue", "", THEMED), ("src/ui/Badge.vue", "Badge.vue", BADGE)];
  let html = agree_tree(&compiler, &dir, &files, "src/ui/Themed.vue", &props(&[("mode", Value::str("dark"))]));
  assert!(html.contains("dark kg fallback"), "{html}");
  std::fs::remove_dir_all(&dir).unwrap();
}
