use snapfire_fsr_core::{Data, Params, Value};
use snapfire_fsr_ir::ast::{Entry, Lit, Stmt};
use snapfire_fsr_ir::native::Frame;
use snapfire_fsr_ir::{Body, Expr, Fail, IrAction, IrMeta, IrPaths, IrSource, IrStore, PreparedBody};
use snapfire_fsr_runtime::{ActionHandler, DataSource, Metadata, Paths, RequestCtx, Seeds};

fn says(word: &str) -> Value {
  let mut fields = snapfire_fsr_core::Fields::default();
  fields.insert("title".to_owned(), Value::str(word));
  fields.insert("slug".to_owned(), Value::str(word));
  Value::Map(fields.into())
}

fn compiled(_: &mut Frame<'_>) -> Result<Value, Fail> {
  Ok(says("compiled"))
}

fn compiled_list(_: &mut Frame<'_>) -> Result<Value, Fail> {
  Ok(Value::Seq(vec![says("compiled")].into()))
}

fn object() -> Body {
  let field = |name: &str| Entry::Field(name.to_owned(), Expr::Lit(Lit::Str("interpreted".into())));
  vec![Stmt::Return(Expr::Object(vec![field("title"), field("slug")]))]
}

fn list() -> Body {
  let field = |name: &str| Entry::Field(name.to_owned(), Expr::Lit(Lit::Str("interpreted".into())));
  vec![Stmt::Return(Expr::Array(vec![Entry::Item(Expr::Object(vec![field("title"), field("slug")]))]))]
}

fn prepared(body: Body, native: Option<snapfire_fsr_ir::native::NativeBody>) -> PreparedBody {
  let prepared = PreparedBody::new(body, None);
  match native {
    Some(native) => prepared.with_native(native),
    None => prepared,
  }
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
  tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
}

fn word(value: &Value) -> String {
  match value {
    Value::Map(map) => format!("{:?}", map.get("title")),
    other => format!("{other:?}"),
  }
}

/// What each kind of lowered body answers when bound to `native`. With `None` it answers from its interpreted body.
fn answers(native: Option<snapfire_fsr_ir::native::NativeBody>, list_native: Option<snapfire_fsr_ir::native::NativeBody>) -> Vec<(&'static str, String)> {
  let ctx = RequestCtx::anonymous(Params::new());
  let data = Data::default();
  let source = block(IrSource::new("catalog", prepared(object(), native)).load(&ctx)).unwrap();
  let action = block(IrAction::new(prepared(object(), native)).call(ctx.clone(), Value::Null)).unwrap();
  let meta = block(IrMeta::new("catalog", prepared(object(), native)).describe(&ctx, &data)).unwrap();
  let store = IrStore::new("catalog").with_store(Some(prepared(object(), native))).with_defaults(Some(prepared(object(), native)));
  let seeded = block(store.seed(&ctx, &data)).unwrap();
  let defaults = block(store.defaults(&ctx, &data)).unwrap();
  let paths = block(IrPaths::new("catalog", prepared(list(), list_native)).paths(&ctx)).unwrap();
  vec![
    ("source", word(&Value::Map(source))),
    ("action", word(&action)),
    ("meta", format!("{:?}", meta.title.map(|t| Value::str(&t)))),
    ("store", word(&Value::Map(seeded))),
    ("defaults", word(&Value::Map(defaults))),
    ("paths", format!("{:?}", paths.first().and_then(|p| p.get("slug")).map(|s| Value::str(s)))),
  ]
}

#[test]
fn every_kind_of_lowered_body_runs_the_compiled_body_attached_to_it() {
  for (kind, said) in answers(Some(compiled), Some(compiled_list)) {
    assert!(said.contains("compiled"), "{kind} answered {said}");
  }
}

#[test]
fn every_kind_of_lowered_body_runs_interpreted_without_one() {
  for (kind, said) in answers(None, None) {
    assert!(said.contains("interpreted"), "{kind} answered {said}");
  }
}

#[test]
fn a_prepared_body_carries_the_plan_a_compiled_body_is_matched_by() {
  let body = object();
  let plan = snapfire_fsr_ir::native::plan_hash(&body, None);
  let prepared = PreparedBody::new(body, None);
  assert_eq!(prepared.plan(), plan);
  assert!(!prepared.is_compiled());
  assert!(prepared.with_native(compiled).is_compiled());
}

#[test]
fn a_bodys_plan_follows_the_constants_it_reaches_and_no_others() {
  use snapfire_fsr_ir::native::plan_hash;
  let body: Body = vec![Stmt::Return(Expr::Const("m#rows".to_owned()))];
  let consts = |rows: &str, limit: i64, other: &str| -> snapfire_fsr_ir::ast::Consts {
    [
      ("m#rows".to_owned(), Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Str(rows.into()))), Entry::Item(Expr::Const("m#limit".to_owned()))])),
      ("m#limit".to_owned(), Expr::Lit(Lit::Int(limit.into()))),
      ("m#other".to_owned(), Expr::Lit(Lit::Str(other.into()))),
    ]
    .into_iter()
    .collect()
  };
  let base = plan_hash(&body, Some(&consts("a", 1, "x")));
  assert_ne!(plan_hash(&body, Some(&consts("b", 1, "x"))), base, "a constant the body names");
  assert_ne!(plan_hash(&body, Some(&consts("a", 2, "x"))), base, "a constant reached through another");
  assert_eq!(plan_hash(&body, Some(&consts("a", 1, "y"))), base, "a constant the body cannot reach");
}

#[test]
fn bodies_hashed_against_one_table_of_constants_get_the_plan_hash_each_would_alone() {
  use snapfire_fsr_ir::native::{plan_hash, plan_hash_with, ConstHashes};
  let consts: snapfire_fsr_ir::ast::Consts = [
    ("m#rows".to_owned(), Expr::Array(vec![Entry::Item(Expr::Const("m#limit".to_owned()))])),
    ("m#limit".to_owned(), Expr::Lit(Lit::Int(3i64.into()))),
    ("m#other".to_owned(), Expr::Lit(Lit::Str("x".into()))),
  ]
  .into_iter()
  .collect();
  let table = ConstHashes::new(&consts);
  for name in ["m#rows", "m#limit", "m#other", "m#missing"] {
    let body: Body = vec![Stmt::Return(Expr::Const(name.to_owned()))];
    assert_eq!(plan_hash_with(&body, Some(&table)), plan_hash(&body, Some(&consts)), "{name}");
  }
  assert_eq!(plan_hash_with(&object(), None), plan_hash(&object(), None));
}
