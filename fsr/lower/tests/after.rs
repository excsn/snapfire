//! `after(fn)` in a body: the function runs once the response is written, with what the body bound when it was called.

use std::sync::Arc;

use futures::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_runtime::{RequestCtx, ServiceCaller, ServiceError, ServiceHandle};

#[derive(Default)]
struct Audit(Mutex<Vec<(String, ValueMap)>>);

impl ServiceCaller for Audit {
  fn call(&self, _service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    self.0.lock().push((method.to_owned(), args));
    Box::pin(async { Ok(Value::Null) })
  }
}

fn ctx(audit: &Arc<Audit>) -> RequestCtx {
  let mut ctx = RequestCtx::anonymous(Default::default());
  ctx.services = ServiceHandle::new(audit.clone());
  ctx
}

fn written(audit: &Audit) -> Vec<(String, ValueMap)> {
  audit.0.lock().clone()
}

fn field(args: &ValueMap, name: &str) -> Value {
  args.get(name).cloned().unwrap_or_else(|| panic!("no `{name}` in {args:?}"))
}

const HEAD: &str = "import { after } from \"@snapfire/fsr/host\";\nimport type { Ctx } from \"@snapfire/fsr/host\";\n";

#[test]
fn a_handlers_after_runs_only_when_the_host_runs_what_it_left() {
  let source = format!("{HEAD}export async function POST({{ services, input }}: Ctx) {{\n  after(async () => {{\n    await services.audit.log({{ what: input.name }});\n  }});\n  return {{ ok: true }};\n}}\n");
  let handlers = snapfire_fsr_lower::lower_handlers("routes/api/route.ts", &source).unwrap_or_else(|e| panic!("{e}"));
  let audit = Arc::new(Audit::default());
  let ctx = ctx(&audit);
  let mut input = ValueMap::default();
  input.insert("name".to_owned(), Value::str("lamp"));
  let outcome = futures::executor::block_on(Interpreter::default().run(&handlers[0].body, &ctx, Some(Value::Map(input)))).unwrap_or_else(|f| panic!("{}", f.message));
  assert!(matches!(&outcome.value, Value::Map(m) if m.get("ok") == Some(&Value::Bool(true))), "{:?}", outcome.value);
  assert!(written(&audit).is_empty(), "nothing ran before the response");
  let left = ctx.after.take();
  assert_eq!(left.len(), 1);
  for work in left {
    futures::executor::block_on(work);
  }
  let calls = written(&audit);
  assert_eq!(calls.len(), 1);
  assert_eq!((calls[0].0.as_str(), field(&calls[0].1, "what")), ("log", Value::str("lamp")));
}

#[test]
fn a_loaders_after_reads_what_was_bound_when_it_was_called_and_runs_when_the_body_fails() {
  let source = format!("import {{ after, fail }} from \"@snapfire/fsr/host\";\nimport type {{ Ctx }} from \"@snapfire/fsr/host\";\nexport async function load({{ services, params }}: Ctx) {{\n  const page = \"home\";\n  after(() => services.audit.log({{ page }}));\n  fail(\"not_found\", \"nothing here\");\n}}\n");
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", &source).unwrap_or_else(|e| panic!("{e}"));
  let audit = Arc::new(Audit::default());
  let ctx = ctx(&audit);
  let failed = futures::executor::block_on(Interpreter::default().run(&body, &ctx, None));
  assert!(failed.is_err());
  for work in ctx.after.take() {
    futures::executor::block_on(work);
  }
  let calls = written(&audit);
  assert_eq!(calls.len(), 1, "work queued before the failure still runs");
  assert_eq!(field(&calls[0].1, "page"), Value::str("home"));
}

#[test]
fn after_takes_a_function() {
  let source = format!("{HEAD}export async function load({{ services }}: Ctx) {{\n  after(services.audit.log({{}}));\n  return {{}};\n}}\n");
  let refused = snapfire_fsr_lower::lower_loader("page.loader.ts", &source).err().map(|e| e.to_string()).unwrap_or_default();
  assert!(refused.contains("`after` takes a function written in place"), "{refused}");
}

#[test]
fn after_cannot_write_the_session() {
  let source = format!("{HEAD}export async function POST({{ session }}: Ctx) {{\n  after(() => {{\n    session.seen = true;\n  }});\n  return {{}};\n}}\n");
  let refused = snapfire_fsr_lower::lower_handlers("routes/api/route.ts", &source).err().map(|e| e.to_string()).unwrap_or_default();
  assert!(refused.contains("a session write inside `after`"), "{refused}");
}
