use snapfire_fsr_core::{Params, Value};
use snapfire_fsr_ir::ast::{CompareOp, Entry, Lit, Stmt};
use snapfire_fsr_ir::compiled::Program;
use snapfire_fsr_ir::{Body, Expr, Interpreter, Prepared};
use snapfire_fsr_runtime::{FailureKind, RequestCtx};

#[path = "written/bodies.rs"]
mod bodies;
use bodies::*;

fn ctx(slug: &str) -> RequestCtx {
  let mut params = Params::new();
  params.insert("slug".to_owned(), slug.to_owned());
  RequestCtx::anonymous(params)
}

fn missing(name: &str) -> Expr {
  Expr::Compare(CompareOp::Eq, Box::new(Expr::Var(name.to_owned())), Box::new(Expr::Lit(Lit::Str("missing".into()))))
}

fn guard(cond: Expr) -> Stmt {
  Stmt::Guard { cond, kind: "not_found".to_owned(), message: Expr::Lit(Lit::Str("No project by that name".into())) }
}

fn guarded() -> Body {
  vec![
    Stmt::Let { name: "slug".to_owned(), expr: Expr::Param("slug".to_owned()) },
    guard(missing("slug")),
    Stmt::Return(Expr::Object(vec![Entry::Field("slug".to_owned(), Expr::Var("slug".to_owned()))])),
  ]
}

fn outcome(body: &Body, prepared: Prepared<'_>, slug: &str) -> Result<Value, (FailureKind, String)> {
  tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(Interpreter::default().run_with(body, prepared, &ctx(slug), None)).map(|o| o.value).map_err(|f| (f.kind, f.message))
}

#[test]
fn a_guard_at_the_body_level_runs_prepared_and_natively() {
  let body = guarded();
  assert!(Program::of(&body).is_some());
  snapfire_fsr_ir::native::emit_or_why("b", &body, None).unwrap();
}

#[test]
fn a_prepared_guard_fails_the_request_the_way_the_interpreter_does() {
  let body = guarded();
  let program = Program::of(&body).unwrap();
  for slug in ["missing", "fsr"] {
    assert_eq!(outcome(&body, Prepared::Compiled(&program), slug), outcome(&body, Prepared::No, slug), "{slug}");
  }
  assert_eq!(outcome(&body, Prepared::Compiled(&program), "missing").unwrap_err().0, FailureKind::NotFound);
}

#[test]
fn a_guard_inside_a_block_expression_answers_prepared_as_it_does_interpreted() {
  let body = vec![
    Stmt::Let { name: "slug".to_owned(), expr: Expr::Param("slug".to_owned()) },
    Stmt::Let { name: "found".to_owned(), expr: Expr::Block(vec![guard(missing("slug")), Stmt::Return(Expr::Var("slug".to_owned()))]) },
    Stmt::Return(Expr::Var("found".to_owned())),
  ];
  let program = Program::of(&body).unwrap();
  for slug in ["missing", "fsr"] {
    assert_eq!(outcome(&body, Prepared::Compiled(&program), slug), outcome(&body, Prepared::No, slug), "{slug}");
  }
}

struct Clock;

impl snapfire_fsr_runtime::Native for Clock {
  fn call(&self, _: &str, _: snapfire_fsr_core::ValueMap) -> futures_util::future::BoxFuture<'static, Result<Value, snapfire_fsr_runtime::ServiceError>> {
    Box::pin(async { Ok(Value::str("UTC")) })
  }

  fn call_sync(&self, method: &str, _: snapfire_fsr_core::ValueMap) -> Option<Result<Value, snapfire_fsr_runtime::ServiceError>> {
    (method == "year").then(|| Ok(Value::F64(2026.0)))
  }
}

fn year(sync: bool) -> Body {
  vec![Stmt::Return(Expr::Object(vec![Entry::Field(
    "year".to_owned(),
    Expr::NativeCall { module: "clock".to_owned(), method: "year".to_owned(), args: Vec::new(), sync },
  )]))]
}

fn with_clock(body: &Body, prepared: Prepared<'_>) -> Result<Value, (FailureKind, String)> {
  let mut natives = snapfire_fsr_runtime::Natives::new();
  natives.register("clock", std::sync::Arc::new(Clock));
  let mut ctx = ctx("fsr");
  ctx.natives = snapfire_fsr_runtime::NativeHandle::new(std::sync::Arc::new(natives));
  tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(Interpreter::default().run_with(body, prepared, &ctx, None)).map(|o| o.value).map_err(|f| (f.kind, f.message))
}

#[test]
fn a_sync_native_call_runs_prepared_and_natively() {
  let body = year(true);
  assert!(Program::of(&body).is_some());
  snapfire_fsr_ir::native::emit_or_why("b", &body, None).unwrap();
}

#[test]
fn a_sync_native_call_answers_prepared_as_it_does_interpreted() {
  let body = year(true);
  let program = Program::of(&body).unwrap();
  assert_eq!(with_clock(&body, Prepared::Compiled(&program)), with_clock(&body, Prepared::No));
  assert!(with_clock(&body, Prepared::No).is_ok());
}

#[test]
fn an_async_native_call_is_left_to_the_interpreter() {
  assert!(Program::of(&year(false)).is_none());
}

#[test]
fn every_read_of_the_request_is_written_natively() {
  let reads = [
    Expr::Param("slug".to_owned()),
    Expr::Query("q".to_owned()),
    Expr::ParamsAll,
    Expr::QueryAll,
    Expr::Session("cart".to_owned()),
    Expr::Locale,
    Expr::Locales,
    Expr::Path,
    Expr::Document,
    Expr::Host,
    Expr::Origin,
    Expr::Address,
    Expr::Config("site".to_owned()),
    Expr::Input,
    Expr::Now,
  ];
  for read in reads {
    let body = vec![Stmt::Return(read.clone())];
    snapfire_fsr_ir::native::emit_or_why("b", &body, None).unwrap_or_else(|why| panic!("{read:?}: {why}"));
  }
}

#[test]
fn number_string_and_bigint_conversions_are_written_natively() {
  let lit = || Box::new(Expr::Lit(Lit::Str("12".into())));
  for conversion in [Expr::Num(lit()), Expr::Str(lit()), Expr::BigInt(lit())] {
    let body = vec![Stmt::Return(conversion.clone())];
    snapfire_fsr_ir::native::emit_or_why("b", &body, None).unwrap_or_else(|why| panic!("{conversion:?}: {why}"));
  }
}

#[test]
fn a_guard_inside_a_try_is_caught_as_the_interpreter_catches_it() {
  let body = vec![
    Stmt::Let { name: "slug".to_owned(), expr: Expr::Param("slug".to_owned()) },
    Stmt::Try {
      body: vec![guard(missing("slug")), Stmt::Return(Expr::Lit(Lit::Str("found".into())))],
      name: Some("e".to_owned()),
      handler: Some(vec![Stmt::Return(Expr::Field(Box::new(Expr::Var("e".to_owned())), "kind".to_owned()))]),
      finally: Vec::new(),
    },
  ];
  let prepared = Program::of(&body);
  let run = |slug| match &prepared {
    Some(program) => outcome(&body, Prepared::Compiled(program), slug),
    None => outcome(&body, Prepared::No, slug),
  };
  assert_eq!(run("missing"), outcome(&body, Prepared::No, "missing"));
  assert_eq!(run("missing"), Ok(Value::str("not_found")));
}

#[test]
fn a_sort_with_a_comparator_without_one_and_in_place_is_written_natively() {
  for (what, body) in [("toSorted by n", sorted_copy(by_n())), ("toSorted", sorted_copy(Expr::Lit(Lit::Null))), ("sort in place", sorted_in_place())] {
    snapfire_fsr_ir::native::emit_or_why("b", &body, None).unwrap_or_else(|why| panic!("{what}: {why}"));
  }
}

const WRITTEN: &str = "tests/fixtures/native_bodies.rs";

fn written_now() -> String {
  let mut out = String::from("// Written by the native generator from `written_bodies` in tests/prepared.rs. `FSR_WRITE_FIXTURES=1 cargo nextest run -p snapfire_fsr_ir --test prepared` rewrites it.\n\n");
  for (name, body) in written_bodies() {
    out.push_str(&snapfire_fsr_ir::native::emit_in("snapfire_fsr_ir::native", name, &body, None).unwrap_or_else(|why| panic!("{name}: {why}")));
    out.push('\n');
  }
  out
}

#[test]
fn the_written_bodies_are_what_the_generator_writes_now() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(WRITTEN);
  let now = written_now();
  if std::env::var_os("FSR_WRITE_FIXTURES").is_some() {
    std::fs::write(&path, &now).unwrap();
  }
  assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), now, "{WRITTEN} is stale; rewrite it with FSR_WRITE_FIXTURES=1");
}
