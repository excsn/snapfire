//! An object a body makes part of a cycle is freed once the request lets go of it, as one outside any cycle is.

use std::sync::Arc;

use futures::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_runtime::{RequestCtx, ServiceCaller, ServiceError, ServiceHandle};

#[derive(Default)]
struct Keep(Mutex<Vec<ValueMap>>);

impl ServiceCaller for Keep {
  fn call(&self, _service: &str, _method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    self.0.lock().push(args);
    Box::pin(async { Ok(Value::Null) })
  }
}

/// How many holders the object the body handed to `keep.hold` as `c` has once the request is over.
fn holders_after(body: &str) -> usize {
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_cycles_{}_{}", std::process::id(), body.len()));
  std::fs::create_dir_all(dir.join("routes/a")).unwrap();
  std::fs::write(dir.join("routes/a/page.loader.ts"), format!("export async function load({{ services }}) {{\n  {body}\n  return 1;\n}}\n")).unwrap();
  let mut set = ComponentSet::new(&dir);
  let plan = set.lower_loader("routes/a/page.loader.ts").unwrap_or_else(|e| panic!("{e}"));
  let _ = std::fs::remove_dir_all(&dir);
  let keep = Arc::new(Keep::default());
  let mut ctx = RequestCtx::anonymous(Default::default());
  ctx.services = ServiceHandle::new(keep.clone());
  let interpreter = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone())));
  futures::executor::block_on(interpreter.run(&plan, &ctx, None)).unwrap_or_else(|f| panic!("{}", f.message));
  let kept = keep.0.lock();
  match kept.first().and_then(|args| args.get("c")) {
    Some(Value::Map(c)) => c.holders(),
    other => panic!("kept {other:?}"),
  }
}

#[test]
fn an_object_outside_any_cycle_is_let_go_of() {
  assert_eq!(holders_after("const c = { n: 1 }; const o = { c }; o.n = 2; await services.keep.hold({ c });"), 1);
}

#[test]
fn an_object_pointing_at_itself_is_freed_when_the_request_ends() {
  assert_eq!(holders_after("const c = { n: 1 }; const o = { c }; o.self = o; await services.keep.hold({ c });"), 1);
}

#[test]
fn two_objects_pointing_at_each_other_are_freed_when_the_request_ends() {
  assert_eq!(holders_after("const c = { n: 1 }; const a = { c }; const b = { a }; a.b = b; await services.keep.hold({ c });"), 1);
}

#[test]
fn an_array_holding_itself_is_freed_when_the_request_ends() {
  assert_eq!(holders_after("const c = { n: 1 }; const list = [c]; list.push(list); await services.keep.hold({ c });"), 1);
}

#[test]
fn a_cycle_through_a_long_chain_is_freed_when_the_request_ends() {
  assert_eq!(holders_after("const c = { n: 1 }; const head = { c }; let at = head; for (let i = 0; i < 200; i++) { const next = { i }; at.next = next; at = next; } at.next = head; await services.keep.hold({ c });"), 1);
}
