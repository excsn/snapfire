//! `try`, `catch`, `finally` and `throw` in a loader and on a render path, run as JavaScript runs them.

use std::path::PathBuf;
use std::sync::Arc;

use futures::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_runtime::{FailureKind, RequestCtx, ServiceCaller, ServiceError, ServiceHandle};

struct Shop;

impl ServiceCaller for Shop {
  fn call(&self, service: &str, method: &str, _args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let (service, method) = (service.to_owned(), method.to_owned());
    Box::pin(async move {
      match method.as_str() {
        "get" => {
          let mut found = ValueMap::default();
          found.insert("name".to_owned(), Value::str("lamp"));
          Ok(Value::Map(found))
        }
        _ => Err(ServiceError::new(FailureKind::Unavailable, service, method, "the shop is closed")),
      }
    })
  }
}

fn run(loader: &str) -> Result<Value, (FailureKind, String)> {
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", loader).unwrap_or_else(|e| panic!("{e}"));
  let mut ctx = RequestCtx::anonymous(Default::default());
  ctx.services = ServiceHandle::new(Arc::new(Shop));
  futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value).map_err(|fail| (fail.kind, fail.message))
}

fn loaded(loader: &str) -> ValueMap {
  match run(loader) {
    Ok(Value::Map(fields)) => fields,
    other => panic!("{other:?}"),
  }
}

fn field(fields: &ValueMap, name: &str) -> Value {
  fields.get(name).cloned().unwrap_or_else(|| panic!("no `{name}` in {fields:?}"))
}

const LOAD: &str = "import { fail } from \"@snapfire/fsr\";\nimport type { Ctx } from \"@snapfire/fsr\";\n";

#[test]
fn a_catch_runs_when_an_awaited_service_call_fails_and_reads_its_kind_and_message() {
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  try {{\n    const found = await services.shop.list({{}});\n    return {{ found }};\n  }} catch (e) {{\n    return {{ kind: e.kind, message: e.message }};\n  }}\n}}\n"));
  assert_eq!(field(&fields, "kind"), Value::str("unavailable"));
  assert_eq!(field(&fields, "message"), Value::str("the shop is closed"));
}

#[test]
fn a_catch_that_falls_through_continues_after_the_try() {
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  let names: string[] = [];\n  try {{\n    const found = await services.shop.list({{}});\n    names = found.names;\n  }} catch {{\n    names = [\"none\"];\n  }}\n  const one = await services.shop.get({{}});\n  return {{ names, one: one.name }};\n}}\n"));
  assert_eq!(field(&fields, "names"), Value::seq(vec![Value::str("none")]));
  assert_eq!(field(&fields, "one"), Value::str("lamp"));
}

#[test]
fn a_failure_a_body_raises_with_fail_is_caught() {
  let fields = loaded(&format!("{LOAD}export async function load({{ params }}: Ctx) {{\n  try {{\n    if (!params.id) fail(\"not_found\", \"no such page\");\n    return {{ kind: \"none\" }};\n  }} catch (e) {{\n    return {{ kind: e.kind, message: e.message }};\n  }}\n}}\n"));
  assert_eq!(field(&fields, "kind"), Value::str("not_found"));
  assert_eq!(field(&fields, "message"), Value::str("no such page"));
}

#[test]
fn a_thrown_value_that_is_not_an_error_is_what_the_catch_binds() {
  let fields = loaded(&format!("{LOAD}export async function load({{ params }}: Ctx) {{\n  try {{\n    throw {{ code: 7 }};\n  }} catch (e) {{\n    return {{ code: e.code }};\n  }}\n}}\n"));
  assert_eq!(field(&fields, "code"), Value::F64(7.0));
}

#[test]
fn a_catch_parameter_may_be_a_pattern() {
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  try {{\n    await services.shop.list({{}});\n    return {{ message: \"\" }};\n  }} catch ({{ message }}) {{\n    return {{ message }};\n  }}\n}}\n"));
  assert_eq!(field(&fields, "message"), Value::str("the shop is closed"));
}

#[test]
fn finally_runs_after_the_try_and_after_the_catch() {
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  let trace = \"\";\n  try {{\n    trace += \"t\";\n  }} finally {{\n    trace += \"f\";\n  }}\n  try {{\n    await services.shop.list({{}});\n    trace += \"x\";\n  }} catch {{\n    trace += \"c\";\n  }} finally {{\n    trace += \"F\";\n  }}\n  return {{ trace }};\n}}\n"));
  assert_eq!(field(&fields, "trace"), Value::str("tfcF"));
}

#[test]
fn a_return_in_finally_replaces_what_the_try_returned_or_threw() {
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  try {{\n    return {{ from: \"try\" }};\n  }} finally {{\n    return {{ from: \"finally\" }};\n  }}\n}}\n"));
  assert_eq!(field(&fields, "from"), Value::str("finally"));
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  try {{\n    await services.shop.list({{}});\n    return {{ from: \"try\" }};\n  }} finally {{\n    return {{ from: \"finally\" }};\n  }}\n}}\n"));
  assert_eq!(field(&fields, "from"), Value::str("finally"));
}

#[test]
fn a_try_without_a_catch_fails_with_what_it_threw_once_finally_has_run() {
  let failed = run(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  try {{\n    const found = await services.shop.list({{}});\n    return {{ found }};\n  }} finally {{\n    const one = await services.shop.get({{}});\n  }}\n}}\n"));
  assert_eq!(failed, Err((FailureKind::Unavailable, "the shop is closed".to_owned())));
}

#[test]
fn a_rethrow_keeps_the_kind_and_an_uncaught_error_is_internal() {
  let rethrown = run(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  try {{\n    return await services.shop.list({{}});\n  }} catch (e) {{\n    throw e;\n  }}\n}}\n"));
  assert_eq!(rethrown, Err((FailureKind::Unavailable, "the shop is closed".to_owned())));
  let thrown = run(&format!("{LOAD}export async function load({{ params }}: Ctx) {{\n  throw new Error(\"broken\");\n}}\n"));
  assert_eq!(thrown, Err((FailureKind::Internal, "broken".to_owned())));
}

#[test]
fn a_try_inside_a_try_catches_what_its_own_await_throws() {
  let fields = loaded(&format!("{LOAD}export async function load({{ services }}: Ctx) {{\n  let trace = \"\";\n  try {{\n    try {{\n      await services.shop.list({{}});\n    }} catch (inner) {{\n      trace += \"i\";\n      await services.shop.list({{}});\n    }}\n  }} catch (outer) {{\n    trace += \"o\";\n  }}\n  return {{ trace }};\n}}\n"));
  assert_eq!(field(&fields, "trace"), Value::str("io"));
}

fn html(tag: &str, page: &str) -> Result<String, String> {
  let dir: PathBuf = std::env::temp_dir().join(format!("fsr_exceptions_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

#[test]
fn a_render_try_that_falls_through_renders_what_follows_it() {
  let page = "export default function Page() {\n  let host = \"\";\n  try {\n    host = new URL(\"nope\").hostname;\n  } catch {\n    host = \"none\";\n  }\n  return <p>{host}</p>;\n}\n";
  assert_eq!(html("fall_through", page).as_deref(), Ok("<p>none</p>"));
}

#[test]
fn a_render_finally_runs_on_either_path() {
  let page = "const label = (s: string) => {\n  let out = \"\";\n  try {\n    out = new URL(s).hostname;\n  } catch (e) {\n    out = e.name;\n  } finally {\n    out = `[${out}]`;\n  }\n  return out;\n};\nexport default function Page() {\n  return <p>{label(\"https://a.example\")} {label(\"nope\")}</p>;\n}\n";
  assert_eq!(html("finally", page).as_deref(), Ok("<p>[a.example] [Error]</p>"));
}

#[test]
fn a_render_try_inside_a_branch_runs_only_when_the_branch_is_taken() {
  let page = |raw: &str| format!("export default function Page() {{\n  const raw = \"{raw}\";\n  let host = \"unset\";\n  if (raw.length > 0) {{\n    try {{\n      host = new URL(raw).hostname;\n    }} finally {{\n      host = host + \"!\";\n    }}\n  }}\n  return <p>{{host}}</p>;\n}}\n");
  assert_eq!(html("branch_taken", &page("https://a.example")).as_deref(), Ok("<p>a.example!</p>"));
  assert_eq!(html("branch_not_taken", &page("")).as_deref(), Ok("<p>unset</p>"));
}

#[test]
fn a_component_try_that_returns_markup_renders_the_return_it_reached() {
  let page = |raw: &str| format!("export default function Page() {{\n  try {{\n    const host = new URL(\"{raw}\").hostname;\n    return <p>{{host}}</p>;\n  }} catch (e) {{\n    const why = e.name;\n    return <p>bad {{why}}</p>;\n  }}\n}}\n");
  assert_eq!(html("markup_ok", &page("https://a.example/x")).as_deref(), Ok("<p>a.example</p>"));
  assert_eq!(html("markup_caught", &page("nope")).as_deref(), Ok("<p>bad Error</p>"));
}

fn stepped(tag: &str, page: &str, handler: usize) -> Result<String, String> {
  let dir: PathBuf = std::env::temp_dir().join(format!("fsr_exceptions_step_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let interpreter = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone())));
  interpreter.island_step(module, &library[module], &ValueMap::default(), &ValueMap::default(), Some(snapfire_fsr_ir::render::HandlerRef::own(handler)), &Value::Null, &library).map(|step| step.rendered.html).map_err(|e| format!("{e:?}"))
}

fn checker(url: &str) -> String {
  format!("import {{ useState }} from \"react\";\nexport default function Page() {{\n  const [host, setHost] = useState(\"none\");\n  const [tries, setTries] = useState(0);\n  return <div><button onClick={{() => {{\n    try {{\n      setHost(new URL(\"{url}\").hostname);\n    }} catch (e) {{\n      setHost(\"bad \" + e.name);\n    }} finally {{\n      setTries(tries + 1);\n    }}\n  }}}}>{{tries}}</button><p>{{host}}</p></div>;\n}}\n")
}

#[test]
fn a_handler_runs_its_catch_when_its_try_fails_and_its_finally_either_way() {
  let ok = stepped("handler_ok", &checker("https://a.example/x"), 0);
  assert!(ok.as_ref().is_ok_and(|html| html.contains(">1</button><p>a.example</p>")), "{ok:?}");
  let caught = stepped("handler_caught", &checker("nope"), 0);
  assert!(caught.as_ref().is_ok_and(|html| html.contains(">1</button><p>bad Error</p>")), "{caught:?}");
}

#[test]
fn a_handler_try_without_a_catch_runs_its_finally_and_fails() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [tries, setTries] = useState(0);\n  return <button onClick={() => {\n    try {\n      setTries(new URL(\"nope\").port);\n    } finally {\n      setTries(tries + 1);\n    }\n  }}>{tries}</button>;\n}\n";
  let failed = stepped("handler_uncaught", page, 0);
  assert!(failed.as_ref().is_err_and(|e| e.contains("Invalid URL")), "{failed:?}");
}
