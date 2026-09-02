//! The five `shopping_react_ts` bodies, hand-written as IR, run against a mock
//! service layer. The Rust originals live in the example's `loaders.rs`,
//! `actions.rs` and `cart.rs`; these must produce the same values.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_ir::ast::{ArithOp, CompareOp, Entry, Lit, Stmt};
use snapfire_fsr_ir::{Body, Expr, Interpreter, IrAction, IrSource};
use snapfire_fsr_runtime::{
  ActionHandler, DataSource, FailureKind, Identity, RequestCtx, ServiceCaller, ServiceError,
  ServiceHandle, SessionCell,
};

#[derive(Default)]
struct Mock {
  returns: Mutex<ValueMap>,
  calls: Mutex<Vec<(String, ValueMap)>>,
  barrier: Option<Arc<tokio::sync::Barrier>>,
}

impl Mock {
  fn returning(method: &str, value: Value) -> Arc<Self> {
    let mock = Self::default();
    mock.returns.lock().insert(method.to_owned(), value);
    Arc::new(mock)
  }

  fn calls(&self) -> Vec<(String, ValueMap)> {
    self.calls.lock().clone()
  }
}

impl ServiceCaller for Mock {
  fn call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let key = format!("{service}.{method}");
    self.calls.lock().push((key.clone(), args));
    let answer = self.returns.lock().get(&key).cloned();
    let barrier = self.barrier.clone();
    let (service, method) = (service.to_owned(), method.to_owned());
    Box::pin(async move {
      if let Some(barrier) = barrier {
        barrier.wait().await;
      }
      answer.ok_or_else(|| ServiceError::new(FailureKind::NotFound, service, method, "no answer recorded"))
    })
  }
}

fn product(id: i64, name: &str, price: i64) -> Value {
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::int(id));
  map.insert("name".to_owned(), Value::str(name));
  map.insert("price_cents".to_owned(), Value::int(price));
  Value::Map(map)
}

fn ctx(mock: Arc<Mock>, params: &[(&str, &str)], session: ValueMap) -> RequestCtx {
  let mut p = Params::new();
  for (k, v) in params {
    p.insert((*k).to_owned(), (*v).to_owned());
  }
  RequestCtx {
    params: p,
    query: Params::new(),
    session: SessionCell::new(session, None),
    csrf: None,
    services: ServiceHandle::new(mock),
  }
}

fn cart_of(entries: &[(&str, i64)]) -> ValueMap {
  let mut cart = ValueMap::new();
  for (id, qty) in entries {
    cart.insert((*id).to_owned(), Value::int(*qty));
  }
  let mut session = ValueMap::new();
  session.insert("cart".to_owned(), Value::Map(cart));
  session
}

// export async function load({ params, services }) {
//   return { products: await services.shopping.listProducts({ tag: params.tag }) };
// }
fn catalog_loader() -> Body {
  vec![Stmt::Return(Expr::object(vec![(
    "products",
    Expr::call("shopping", "listProducts", vec![("tag", Expr::Param("tag".into()))]),
  )]))]
}

// export async function load({ params, services }) {
//   return { product: await services.shopping.getProduct({ id: BigInt(params.id) }) };
// }
fn product_loader() -> Body {
  vec![Stmt::Return(Expr::object(vec![(
    "product",
    Expr::call("shopping", "getProduct", vec![("id", Expr::BigInt(Box::new(Expr::Param("id".into()))))]),
  )]))]
}

// export async function load({ session, services }) {
//   const catalog = await services.shopping.listProducts({});
//   const lines = catalog
//     .filter((p) => session.cart[String(p.id)])
//     .map((p) => ({ ...p, quantity: session.cart[String(p.id)] }));
//   return { lines };
// }
fn cart_loader() -> Body {
  let held = || Expr::Session("cart".into()).index(Expr::Str(Box::new(Expr::var("p").field("id"))));
  vec![
    Stmt::Let { name: "catalog".into(), expr: Expr::call("shopping", "listProducts", vec![]) },
    Stmt::Let {
      name: "lines".into(),
      expr: Expr::Map(
        Box::new(Expr::Filter(Box::new(Expr::var("catalog")), Box::new(Expr::lambda(&["p"], held())))),
        Box::new(Expr::lambda(
          &["p"],
          Expr::Object(vec![Entry::Spread(Expr::var("p")), Entry::Field("quantity".into(), held())]),
        )),
      ),
    },
    Stmt::Return(Expr::object(vec![("lines", Expr::var("lines"))])),
  ]
}

// export const addToCart = action<AddToCart>(async ({ input, session }) => {
//   const key = String(input.product_id);
//   const wanted = (session.cart[key] ?? 0n) + input.quantity;
//   if (wanted <= 0n) delete session.cart[key];
//   else session.cart[key] = wanted;
//   return { lines: session.cart };
// });
fn add_to_cart() -> Body {
  vec![
    Stmt::Let { name: "key".into(), expr: Expr::Str(Box::new(Expr::Input.field("product_id"))) },
    Stmt::Let {
      name: "wanted".into(),
      expr: Expr::Arith(
        ArithOp::Add,
        Box::new(Expr::Coalesce(
          Box::new(Expr::Session("cart".into()).index(Expr::var("key"))),
          Box::new(Expr::lit_int(0)),
        )),
        Box::new(Expr::Input.field("quantity")),
      ),
    },
    Stmt::If {
      cond: Expr::Compare(CompareOp::Le, Box::new(Expr::var("wanted")), Box::new(Expr::lit_int(0))),
      then: vec![Stmt::SessionDelete { key: "cart".into(), path: vec![Expr::var("key")] }],
      r#else: vec![Stmt::SessionSet { key: "cart".into(), path: vec![Expr::var("key")], value: Expr::var("wanted") }],
    },
    Stmt::Return(Expr::object(vec![("lines", Expr::Session("cart".into()))])),
  ]
}

// export const checkout = action(async ({ session, services }) => {
//   const lines = Object.entries(session.cart).map(([id, quantity]) => ({ product_id: BigInt(id), quantity }));
//   if (lines.length === 0) fail("invalid", "the cart is empty");   // length is a number, 0 is a number
//   const order = await services.shopping.placeOrder({ lines });
//   session.cart = {};
//   return order;
// });
fn checkout() -> Body {
  vec![
    Stmt::Let {
      name: "lines".into(),
      expr: Expr::Map(
        Box::new(Expr::Entries(Box::new(Expr::Session("cart".into())))),
        Box::new(Expr::lambda(
          &["e"],
          Expr::object(vec![
            ("product_id", Expr::BigInt(Box::new(Expr::var("e").index(Expr::Lit(Lit::Float(0.0)))))),
            ("quantity", Expr::var("e").index(Expr::Lit(Lit::Float(1.0)))),
          ]),
        )),
      ),
    },
    Stmt::Guard {
      cond: Expr::Compare(CompareOp::Eq, Box::new(Expr::Length(Box::new(Expr::var("lines")))), Box::new(Expr::Lit(Lit::Float(0.0)))),
      kind: "invalid".into(),
      message: "the cart is empty".into(),
    },
    Stmt::Let { name: "order".into(), expr: Expr::call("shopping", "placeOrder", vec![("lines", Expr::var("lines"))]) },
    Stmt::SessionSet { key: "cart".into(), path: vec![], value: Expr::Object(vec![]) },
    Stmt::Return(Expr::var("order")),
  ]
}

fn run(body: &Body, ctx: &RequestCtx, input: Option<Value>) -> Result<Value, snapfire_fsr_ir::Fail> {
  tokio::runtime::Runtime::new()
    .unwrap()
    .block_on(Interpreter::default().run(body, ctx, input))
    .map(|o| o.value)
}

#[test]
fn the_catalog_loader_passes_the_tag_and_omits_it_when_absent() {
  let mock = Mock::returning("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400)]));

  let data = run(&catalog_loader(), &ctx(mock.clone(), &[("tag", "printing")], ValueMap::new()), None).unwrap();
  let Value::Map(data) = data else { panic!("a loader returns an object") };
  assert_eq!(data.get("products"), Some(&Value::Seq(vec![product(1, "Filament", 2400)])));
  assert_eq!(mock.calls()[0].1.get("tag"), Some(&Value::str("printing")));

  run(&catalog_loader(), &ctx(mock.clone(), &[], ValueMap::new()), None).unwrap();
  assert!(mock.calls()[1].1.is_empty(), "an absent optional param is not sent as null");
}

#[test]
fn the_product_loader_coerces_the_id_and_rejects_a_non_number() {
  let mock = Mock::returning("shopping.getProduct", product(7, "Nozzle", 900));

  let data = run(&product_loader(), &ctx(mock.clone(), &[("id", "7")], ValueMap::new()), None).unwrap();
  let Value::Map(data) = data else { panic!() };
  assert_eq!(data.get("product"), Some(&product(7, "Nozzle", 900)));
  assert_eq!(mock.calls()[0].1.get("id"), Some(&Value::int(7)));

  let fail = run(&product_loader(), &ctx(mock.clone(), &[("id", "seven")], ValueMap::new()), None).unwrap_err();
  assert_eq!(fail.kind, FailureKind::Invalid);
  assert_eq!(mock.calls().len(), 1, "the backend is never asked for `seven`");
}

#[test]
fn the_cart_loader_joins_held_lines_with_the_catalog() {
  let mock = Mock::returning(
    "shopping.listProducts",
    Value::Seq(vec![product(1, "Filament", 2400), product(2, "Nozzle", 900), product(3, "Bed", 5000)]),
  );
  let data = run(&cart_loader(), &ctx(mock, &[], cart_of(&[("1", 2), ("3", 1)])), None).unwrap();
  let Value::Map(data) = data else { panic!() };
  let Some(Value::Seq(lines)) = data.get("lines") else { panic!("lines is an array") };

  let mut first = product(1, "Filament", 2400);
  if let Value::Map(m) = &mut first {
    m.insert("quantity".into(), Value::int(2));
  }
  let mut third = product(3, "Bed", 5000);
  if let Value::Map(m) = &mut third {
    m.insert("quantity".into(), Value::int(1));
  }
  assert_eq!(lines, &vec![first, third]);
}

#[test]
fn add_to_cart_adds_removes_and_returns_the_lines() {
  let mock = Arc::new(Mock::default());
  let input = |id: i64, qty: i64| {
    let mut m = ValueMap::new();
    m.insert("product_id".into(), Value::int(id));
    m.insert("quantity".into(), Value::int(qty));
    Value::Map(m)
  };

  let c = ctx(mock.clone(), &[], cart_of(&[("1", 2)]));
  let out = run(&add_to_cart(), &c, Some(input(1, 1))).unwrap();
  let Value::Map(out) = out else { panic!() };
  assert_eq!(out.get("lines"), Some(&Value::Map(cart_of(&[("1", 3)]).shift_remove("cart").map(|v| match v { Value::Map(m) => m, _ => unreachable!() }).unwrap())));
  assert_eq!(c.session.get("cart"), Some(Value::Map(cart_of(&[("1", 3)]).shift_remove("cart").map(|v| match v { Value::Map(m) => m, _ => unreachable!() }).unwrap())), "the draft was committed");

  let out = run(&add_to_cart(), &c, Some(input(1, -3))).unwrap();
  let Value::Map(out) = out else { panic!() };
  assert_eq!(out.get("lines"), Some(&Value::Map(ValueMap::new())), "a quantity at zero removes the line");
  assert!(mock.calls().is_empty(), "adding to the cart never calls the backend");
}

#[test]
fn checkout_refuses_an_empty_cart_before_any_call_and_clears_it_after() {
  let mock = Mock::returning("shopping.placeOrder", Value::str("order-1"));

  let empty = ctx(mock.clone(), &[], cart_of(&[]));
  let fail = run(&checkout(), &empty, Some(Value::Null)).unwrap_err();
  assert_eq!(fail.kind, FailureKind::Invalid);
  assert_eq!(fail.message, "the cart is empty");
  assert!(mock.calls().is_empty(), "the guard ran before the call");

  let full = ctx(mock.clone(), &[], cart_of(&[("2", 1), ("1", 4)]));
  let out = run(&checkout(), &full, Some(Value::Null)).unwrap();
  assert_eq!(out, Value::str("order-1"));
  let (_, args) = &mock.calls()[0];
  let Some(Value::Seq(lines)) = args.get("lines") else { panic!("lines were sent") };
  let mut line = ValueMap::new();
  line.insert("product_id".into(), Value::int(2));
  line.insert("quantity".into(), Value::int(1));
  assert_eq!(lines[0], Value::Map(line));
  assert_eq!(full.session.get("cart"), Some(Value::Map(ValueMap::new())), "the cart is cleared");
}

#[test]
fn a_failed_body_leaves_the_session_untouched() {
  let mock = Arc::new(Mock::default());
  let body = vec![
    Stmt::SessionSet { key: "cart".into(), path: vec![], value: Expr::Object(vec![]) },
    Stmt::Guard { cond: Expr::Lit(Lit::Bool(true)), kind: "conflict".into(), message: "no".into() },
  ];
  let c = ctx(mock, &[], cart_of(&[("1", 1)]));
  let fail = run(&body, &c, None).unwrap_err();
  assert_eq!(fail.kind, FailureKind::Conflict);
  assert_eq!(c.session.get("cart"), Some(Value::Map(cart_of(&[("1", 1)]).shift_remove("cart").map(|v| match v { Value::Map(m) => m, _ => unreachable!() }).unwrap())));
  assert!(!c.session.is_dirty());
}

#[tokio::test]
async fn independent_lets_issue_their_calls_together() {
  let mock = Arc::new(Mock {
    barrier: Some(Arc::new(tokio::sync::Barrier::new(2))),
    ..Default::default()
  });
  mock.returns.lock().insert("shopping.listProducts".into(), Value::Seq(vec![]));
  mock.returns.lock().insert("shopping.getProduct".into(), product(1, "Filament", 2400));
  let body = vec![
    Stmt::Let { name: "a".into(), expr: Expr::call("shopping", "listProducts", vec![]) },
    Stmt::Let { name: "b".into(), expr: Expr::call("shopping", "getProduct", vec![("id", Expr::lit_int(1))]) },
    Stmt::Return(Expr::object(vec![("a", Expr::var("a")), ("b", Expr::var("b"))])),
  ];
  let c = ctx(mock, &[], ValueMap::new());
  let outcome = tokio::time::timeout(Duration::from_secs(2), Interpreter::default().run(&body, &c, None))
    .await
    .expect("both calls were in flight at once, so the barrier released")
    .unwrap();
  let Value::Map(out) = outcome.value else { panic!() };
  assert_eq!(out.get("b"), Some(&product(1, "Filament", 2400)));
}

#[tokio::test]
async fn a_dependent_let_waits_for_the_one_it_reads() {
  let mock = Mock::returning("shopping.getProduct", product(1, "Filament", 2400));
  mock.returns.lock().insert("shopping.listProducts".into(), Value::Seq(vec![product(1, "Filament", 2400)]));
  let body = vec![
    Stmt::Let { name: "catalog".into(), expr: Expr::call("shopping", "listProducts", vec![]) },
    Stmt::Let {
      name: "first".into(),
      expr: Expr::call("shopping", "getProduct", vec![("id", Expr::var("catalog").index(Expr::lit_int(0)).field("id"))]),
    },
    Stmt::Return(Expr::var("first")),
  ];
  let c = ctx(mock.clone(), &[], ValueMap::new());
  let outcome = Interpreter::default().run(&body, &c, None).await.unwrap();
  assert_eq!(outcome.value, product(1, "Filament", 2400));
  assert_eq!(mock.calls()[1].1.get("id"), Some(&Value::int(1)));
}

#[test]
fn identity_and_now_are_reads() {
  struct Fixed;
  impl snapfire_fsr_ir::Clock for Fixed {
    fn now(&self) -> i128 {
      1_700_000_000_000
    }
  }
  let mut claims = ValueMap::new();
  claims.insert("tenant".into(), Value::str("acme"));
  let c = RequestCtx {
    params: Params::new(),
    query: Params::new(),
    session: SessionCell::new(ValueMap::new(), Some(Identity { subject: "u1".into(), claims })),
    csrf: None,
    services: ServiceHandle::default(),
  };
  let body = vec![Stmt::Return(Expr::object(vec![
    ("who", Expr::Identity(vec!["subject".into()])),
    ("tenant", Expr::Identity(vec!["claims".into(), "tenant".into()])),
    ("at", Expr::Now),
  ]))];
  let out = tokio::runtime::Runtime::new()
    .unwrap()
    .block_on(Interpreter::with_clock(Arc::new(Fixed)).run(&body, &c, None))
    .unwrap();
  let Value::Map(out) = out.value else { panic!() };
  assert_eq!(out.get("who"), Some(&Value::str("u1")));
  assert_eq!(out.get("tenant"), Some(&Value::str("acme")));
  assert_eq!(out.get("at"), Some(&Value::Int(1_700_000_000_000)));
}

#[test]
fn a_query_read_is_a_string_or_null() {
  let mock = Arc::new(Mock::default());
  let mut c = ctx(mock, &[], ValueMap::new());
  c.query.insert("tag".into(), "printing".into());
  let body = vec![Stmt::Return(Expr::object(vec![("tag", Expr::Query("tag".into())), ("missing", Expr::Query("other".into()))]))];
  let Value::Map(out) = run(&body, &c, None).unwrap() else { panic!() };
  assert_eq!(out.get("tag"), Some(&Value::str("printing")));
  assert_eq!(out.get("missing"), Some(&Value::Null));
}

#[test]
fn mixed_operand_types_are_an_internal_failure_not_a_coercion() {
  let body = vec![Stmt::Return(Expr::Arith(ArithOp::Add, Box::new(Expr::lit_int(1)), Box::new(Expr::lit_str("1"))))];
  let c = ctx(Arc::new(Mock::default()), &[], ValueMap::new());
  let fail = run(&body, &c, None).unwrap_err();
  assert_eq!(fail.kind, FailureKind::Internal);
  assert!(fail.message.contains("int and string"), "{}", fail.message);
}

#[test]
fn every_body_round_trips_through_json() {
  for body in [catalog_loader(), product_loader(), cart_loader(), add_to_cart(), checkout()] {
    let text = snapfire_fsr_ir::ast::to_json(&body);
    let back = snapfire_fsr_ir::ast::from_json(&text).unwrap();
    assert_eq!(back, body, "{text}");
  }
}

#[test]
fn the_bound_source_and_action_answer_through_the_runtime_traits() {
  let mock = Mock::returning("shopping.listProducts", Value::Seq(vec![product(1, "Filament", 2400)]));
  let rt = tokio::runtime::Runtime::new().unwrap();

  let source = IrSource::new("cart_loader", cart_loader());
  let c = ctx(mock.clone(), &[], cart_of(&[("1", 1)]));
  let data = rt.block_on(source.load(&c)).unwrap();
  assert!(matches!(data.get("lines"), Some(Value::Seq(lines)) if lines.len() == 1));

  let not_an_object = IrSource::new("bad", vec![Stmt::Return(Expr::lit_int(1))]);
  let err = rt.block_on(not_an_object.load(&c)).unwrap_err();
  assert_eq!(err.source_id, "bad");
  assert!(err.message.contains("must return an object"));

  let action = IrAction::new(checkout());
  let err = rt.block_on(action.call(ctx(mock, &[], cart_of(&[])), Value::Null)).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid);
}
