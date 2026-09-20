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
use snapfire_fsr_ir::{Frameworks, HydratedBy, Interpreter, VueMajor};
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
  Component { body: Vec::new(), render: Tmpl::Island { module: module.to_owned(), props: vec![Entry::Spread(Expr::var("$props"))], children, when: None, mode: None, id: 0, define: false }, state: Vec::new(), handlers: Vec::new(), hydrated_by: None, shadow: None }
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
  assert_eq!(component.hydrated_by, Some(HydratedBy::Vue));
  assert_eq!(component.state, vec!["held".to_owned(), "open".to_owned()]);
  assert!(set.foreign.is_empty(), "a lowered component is not foreign: {:?}", set.foreign);
  let plan = serde_json::to_string(component).unwrap();
  assert!(plan.contains("\"hydrate\":\"vue\""), "{plan}");
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
