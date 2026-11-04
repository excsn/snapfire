//! How deep a render nests is the only limit on it. The tests run on a small stack, so what they assert holds whatever a machine's frames take and whatever stack its thread was given.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::interp::MAX_APPLIES;
use snapfire_fsr_ir::render::{prepare, Components, MAX_CALLS};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

const SMALL_STACK: usize = 256 << 10;

const TREE: &str = "export function Node({ node }: { node: { text: string; children: { text: string }[] } }) {\n  const kids = node.children.filter((child) => child.text !== \"\");\n  return <section className=\"a\"><div><ul>{kids.length > 0 ? kids.map((child, i) => <li key={i}><span><Node node={child} /></span></li>) : <li>end</li>}</ul></div></section>;\n}\n";

static NEXT: AtomicU32 = AtomicU32::new(0);

fn library(page: &str, module: &str) -> Components {
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_stack_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  let mut set = ComponentSet::new(&dir);
  set.lower(module).unwrap();
  let _ = std::fs::remove_dir_all(&dir);
  set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect()
}

fn on_a_small_stack(library: Components, module: &'static str, props: ValueMap) -> Result<String, String> {
  let (html, _props) = std::thread::Builder::new()
    .stack_size(SMALL_STACK)
    .spawn(move || {
      let component = library.get(module).cloned().unwrap();
      (Interpreter::default().render_module(module, &component, &props, &library).map(|rendered| rendered.html).map_err(|e| e.message), props)
    })
    .unwrap()
    .join()
    .unwrap();
  html
}

fn tree(levels: usize) -> Value {
  let mut node = Value::seq(Vec::new());
  for _ in 0..levels {
    let map: ValueMap = [("text".to_owned(), Value::str("x")), ("children".to_owned(), node)].into_iter().collect();
    node = Value::seq(vec![Value::Map(map)]);
  }
  let Value::Seq(mut top) = node else { unreachable!() };
  top.pop().unwrap()
}

fn prop(name: &str, value: Value) -> ValueMap {
  [(name.to_owned(), value)].into_iter().collect()
}

#[test]
fn a_large_component_nested_to_the_limit_renders() {
  let html = on_a_small_stack(library(TREE, "routes/page.tsx#Node"), "routes/page.tsx#Node", prop("node", tree(MAX_CALLS + 1))).unwrap();
  assert_eq!(html.matches("<section").count(), MAX_CALLS + 1);
}

#[test]
fn a_large_component_nested_past_the_limit_fails_the_render() {
  let message = on_a_small_stack(library(TREE, "routes/page.tsx#Node"), "routes/page.tsx#Node", prop("node", tree(MAX_CALLS + 2))).unwrap_err();
  assert!(message.contains(&format!("nested {MAX_CALLS} components deep")), "{message}");
}

#[test]
fn a_component_that_never_stops_fails_the_render() {
  let page = "export function Forever() {\n  return <div><Forever /></div>;\n}\n";
  let message = on_a_small_stack(library(page, "routes/page.tsx#Forever"), "routes/page.tsx#Forever", ValueMap::default()).unwrap_err();
  assert!(message.contains(&format!("nested {MAX_CALLS} components deep")), "{message}");
}

#[test]
fn a_function_that_recurses_to_its_limit_answers() {
  let page = "function depth(n: number): number {\n  return n === 0 ? 0 : [n].map((m) => depth(m - 1) + 1)[0];\n}\nexport function Page({ n }: { n: number }) {\n  return <p>{depth(n)}</p>;\n}\n";
  let levels = MAX_APPLIES - 1;
  let html = on_a_small_stack(library(page, "routes/page.tsx#Page"), "routes/page.tsx#Page", prop("n", Value::F64(levels as f64))).unwrap();
  assert_eq!(html, format!("<p>{levels}</p>"));
}

#[test]
fn a_function_that_never_stops_fails_the_render() {
  let page = "function forever(n: number): number {\n  return [n].map((m) => forever(m + 1))[0];\n}\nexport function Page({ n }: { n: number }) {\n  return <p>{forever(n)}</p>;\n}\n";
  let message = on_a_small_stack(library(page, "routes/page.tsx#Page"), "routes/page.tsx#Page", prop("n", Value::F64(0.0))).unwrap_err();
  assert!(message.contains(&format!("called itself {MAX_APPLIES} levels deep")), "{message}");
}

fn body_on_a_small_stack(source: &str) -> Result<Value, String> {
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_stack_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
  std::fs::create_dir_all(dir.join("routes/a")).unwrap();
  std::fs::write(dir.join("routes/a/page.loader.ts"), source).unwrap();
  let body = ComponentSet::new(&dir).lower_loader("routes/a/page.loader.ts").unwrap();
  let _ = std::fs::remove_dir_all(&dir);
  std::thread::Builder::new()
    .stack_size(SMALL_STACK)
    .spawn(move || {
      let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
      futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).map_err(|e| e.message)
    })
    .unwrap()
    .join()
    .unwrap()
}

const DEEP_OBJECT: &str = "let n = null;\n  for (const i of \"x\".repeat(200000).split(\"\")) {\n    n = { next: n };\n  }\n";
const DEEP_ARRAY: &str = "let n = [];\n  for (const i of \"x\".repeat(200000).split(\"\")) {\n    n = [n];\n  }\n";

#[test]
fn a_very_deep_object_a_body_built_is_dropped() {
  assert_eq!(body_on_a_small_stack(&format!("export async function load() {{\n  {DEEP_OBJECT}  return n === null;\n}}\n")), Ok(Value::Bool(false)));
}

#[test]
fn a_very_deep_array_a_body_built_is_dropped() {
  assert_eq!(body_on_a_small_stack(&format!("export async function load() {{\n  {DEEP_ARRAY}  return n.length;\n}}\n")), Ok(Value::F64(1.0)));
}

#[test]
fn a_very_deep_object_stringifies() {
  let length = "{\"next\":".len() * 200000 + "null".len() + 200000;
  assert_eq!(body_on_a_small_stack(&format!("export async function load() {{\n  {DEEP_OBJECT}  return JSON.stringify(n).length;\n}}\n")), Ok(Value::F64(length as f64)));
}

#[test]
fn a_very_deep_array_converts_to_text() {
  assert_eq!(body_on_a_small_stack(&format!("export async function load() {{\n  {DEEP_ARRAY}  return String(n);\n}}\n")), Ok(Value::str("")));
}

#[test]
fn a_very_deep_value_a_body_returns_fails_the_body() {
  let message = body_on_a_small_stack(&format!("export async function load() {{\n  {DEEP_OBJECT}  return n;\n}}\n")).unwrap_err();
  assert!(message.contains("nested"), "{message}");
}

#[test]
fn a_very_deep_value_passed_to_a_service_fails_the_body() {
  let message = body_on_a_small_stack(&format!("export async function load({{ services }}) {{\n  {DEEP_OBJECT}  return await services.store.keep({{ n }});\n}}\n")).unwrap_err();
  assert!(message.contains("nested"), "{message}");
}
