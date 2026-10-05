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
  assert_eq!(refused("Model", "<script setup>\nimport { ref } from \"vue\";\nconst text = ref(\"\");\n</script>\n<template><input v-model=\"text\" /></template>\n"), "5:18: `v-model`", "the directive's own position, not its expression's");
  assert_eq!(refused("Named", "<template><div><slot name=\"aside\" /></div></template>\n"), "1:22: `<slot name=\"aside\">`, a named slot; an island's children fill the default slot alone");
  assert_eq!(refused("Nested", "<script setup>\nimport Row from \"./Row.vue\";\n</script>\n<template><Row /></template>\n"), "4:11: `<Row>`, a component placed inside a Vue template; the build lowers a file's own template and the browser mounts what it places");
  assert!(refused("Options", "<script>\nexport default { data() { return {}; } };\n</script>\n<template><p /></template>\n").ends_with("a `<script>` without `setup`"));
  assert_eq!(refused("Inject", "<script setup>\nimport { inject } from \"vue\";\nconst theme = inject(\"theme\");\n</script>\n<template><p>{{ theme }}</p></template>\n"), "3:15: `inject`, which reads what a parent component provides; a lowered component has its props alone");
  let bare = refused("Bare", "<template><p>{{ nowhere }}</p></template>\n");
  assert!(bare.starts_with("1:17: `nowhere` is not bound here"), "{bare}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_page_placing_a_described_component_that_does_not_lower_keeps_it_foreign_and_says_why() {
  let compiler = Compiler::new().expect("the compiler boots");
  let page = "import { Island } from \"@snapfire/fsr-authoring/template\";\nimport Box from \"@src/ui/Box.vue\";\nexport default function Page() {\n  return <main><Island><Box /></Island></main>;\n}\n";
  let dir = app("page", &[("routes/page.tsx", page), ("src/ui/Box.vue", "<script setup>\nimport { ref } from \"vue\";\nconst text = ref(\"\");\n</script>\n<template><input v-model=\"text\" /></template>\n")]);
  let mut set = ComponentSet::new(&dir);
  set.describe("src/ui/Box.vue", describe(&compiler, "src/ui/Box.vue", &std::fs::read_to_string(dir.join("src/ui/Box.vue")).unwrap()));
  set.lower("routes/page.tsx#default").expect("the page lowers with the box foreign");
  assert_eq!(set.foreign, vec!["src/ui/Box.vue#default".to_owned()]);
  let (module, residue) = &set.foreign_residue[0];
  assert_eq!(module, "src/ui/Box.vue#default");
  assert_eq!((residue.line, residue.message.as_str()), (5, "`v-model`"));
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
