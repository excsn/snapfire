# Usage Guide: snapfire_fsr_ir

How to write a body as IR, run it against a request, read and write its JSON form, bind it as a data source or an action and test it against a mock service layer.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Writing a Loader Body](#writing-a-loader-body)
  * [Reading the Context](#reading-the-context)
  * [Calling a Service](#calling-a-service)
  * [Shaping the Result](#shaping-the-result)
* [Writing an Action Body](#writing-an-action-body)
  * [Reading the Input](#reading-the-input)
  * [Writing the Session](#writing-the-session)
  * [Guarding Before Any Call](#guarding-before-any-call)
* [Running a Body](#running-a-body)
* [Reading and Writing the JSON Form](#reading-and-writing-the-json-form)
* [Binding a Body to the Runtime](#binding-a-body-to-the-runtime)
* [Pinning the Clock](#pinning-the-clock)
* [Testing Against a Mock Service Layer](#testing-against-a-mock-service-layer)
* [How Calls Are Ordered](#how-calls-are-ordered)
* [Error Handling](#error-handling)

## Core Concepts

* **Body** is a `Vec<Stmt>`, the statements of one loader or action in order.
* **Statement** is a `Stmt`: a `let`, an `if`, a `for...of`, a `return`, a guard, a session write, a session delete or a bare expression.
* **Expression** is an `Expr`: a read, a literal, an access, an operator, a lambda, a call or a builtin. Every expression produces a `Value`.
* **Read** is an expression with no operands that names something on the request: `Param`, `Query`, `Session`, `Identity`, `Input`, `Now`.
* **Call** is `Expr::Call`, an `await services.<service>.<method>(args)`; it goes through `RequestCtx::services`.
* **Lambda** is `Expr::Lambda`, an arrow function applied by a builtin; it is never a value.
* **Builtin** is one of the fixed array and conversion operations: `Map`, `Filter`, `Reduce`, `Find`, `Some`, `Every`, `Entries`, `Keys`, `Values`, `Length`, `Str`, `Num`, `BigInt`.
* **Guard** is `Stmt::Guard`, an `if (cond) fail(kind, message)`; a guard the interpreter can evaluate from reads alone runs before anything else.
* **Draft** is the copy of the session a body writes to; it is committed to the `SessionCell` when the body succeeds and dropped when it fails.
* **Outcome** is what a successful run returns: the value and the session keys written.
* **Fail** is what a failed run returns: a `FailureKind` and a message.
* **Value** and **ValueMap** come from `snapfire_fsr_core`; integers are `Value::Int`, an `i128`, so a TypeScript `bigint` crosses without loss.

## Quick Start

The catalog loader of the shopping example, `{ products: await services.shopping.listProducts({ tag: query.tag }) }`, run against a request.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Expr, Interpreter, Stmt};
use snapfire_fsr_runtime::RequestCtx;

let body = vec![Stmt::Return(Expr::object(vec![(
  "products",
  Expr::call("shopping", "listProducts", vec![("tag", Expr::Query("tag".into()))]),
)]))];

let ctx: RequestCtx = /* bound by the host, with params and a service handle */;
let outcome = Interpreter::default().run(&body, &ctx, None).await?;
let Value::Map(data) = outcome.value else { unreachable!("a loader returns an object") };
```

## Writing a Loader Body

### Reading the Context

Each read names one thing on the request. A missing param or session key reads as `Value::Null`.

```rust
Expr::Param("id".into())                       // params.id, a string
Expr::Query("tag".into())                      // query.tag, a string or null
Expr::Session("cart".into())                   // session.cart
Expr::Identity(vec!["subject".into()])         // identity.subject
Expr::Identity(vec!["claims".into(), "tenant".into()])
Expr::Now                                      // ctx.now, milliseconds since the epoch as Value::Int
```

### Calling a Service

Arguments are named. An argument that evaluates to `Value::Null` is left out, the way an absent optional argument is in TypeScript.

```rust
Expr::call("shopping", "getProduct", vec![("id", Expr::BigInt(Box::new(Expr::Param("id".into()))))])
```

`BigInt` of a non-numeric string fails the body with `FailureKind::Invalid` before any call is made.

### Shaping the Result

The cart loader joins the catalog with the session. `Expr::var` reads a `let`, `field` and `index` reach into values, `Entry::Spread` copies an object's fields.

```rust
use snapfire_fsr_ir::ast::Entry;

let held = || Expr::Session("cart".into()).index(Expr::Str(Box::new(Expr::var("p").field("id"))));
let body = vec![
  Stmt::Let { name: "catalog".into(), expr: Expr::call("shopping", "listProducts", vec![]) },
  Stmt::Let {
    name: "lines".into(),
    expr: Expr::Map(
      Box::new(Expr::Filter(Box::new(Expr::var("catalog")), Box::new(Expr::lambda(&["p"], held())))),
      Box::new(Expr::lambda(&["p"], Expr::Object(vec![Entry::Spread(Expr::var("p")), Entry::Field("quantity".into(), held())]))),
    ),
  },
  Stmt::Return(Expr::object(vec![("lines", Expr::var("lines"))])),
];
```

## Writing an Action Body

### Reading the Input

`Expr::Input` is the submitted value, already checked against the action's declared type by the host.

```rust
Expr::Input.field("product_id")
Expr::Input.field("quantity")
```

### Writing the Session

A write names a top-level key and an optional path beneath it. Writes land in the draft; the `SessionCell` sees them only when the body completes.

```rust
use snapfire_fsr_ir::ast::{ArithOp, CompareOp};

let body = vec![
  Stmt::Let { name: "key".into(), expr: Expr::Str(Box::new(Expr::Input.field("product_id"))) },
  Stmt::Let {
    name: "wanted".into(),
    expr: Expr::Arith(
      ArithOp::Add,
      Box::new(Expr::Coalesce(Box::new(Expr::Session("cart".into()).index(Expr::var("key"))), Box::new(Expr::lit_int(0)))),
      Box::new(Expr::Input.field("quantity")),
    ),
  },
  Stmt::If {
    cond: Expr::Compare(CompareOp::Le, Box::new(Expr::var("wanted")), Box::new(Expr::lit_int(0))),
    then: vec![Stmt::SessionDelete { key: "cart".into(), path: vec![Expr::var("key")] }],
    r#else: vec![Stmt::SessionSet { key: "cart".into(), path: vec![Expr::var("key")], value: Expr::var("wanted") }],
  },
  Stmt::Return(Expr::object(vec![("lines", Expr::Session("cart".into()))])),
];
```

To replace a key outright, give an empty path: `Stmt::SessionSet { key: "cart".into(), path: vec![], value: Expr::Object(vec![]) }`.

### Guarding Before Any Call

A guard fails the body with a named kind. The kind is a `FailureKind` name: `unauthorized`, `not_found`, `invalid`, `conflict`, `timeout`, `unavailable` or `internal`.

```rust
Stmt::Guard {
  cond: Expr::Compare(CompareOp::Eq, Box::new(Expr::Length(Box::new(Expr::var("lines")))), Box::new(Expr::lit_int(0))),
  kind: "invalid".into(),
  message: "the cart is empty".into(),
}
```

A guard whose condition reads no `let` and makes no call, sitting before the first statement that could write the session, is evaluated before anything else runs. A guard that reads a `let` runs in sequence, which still precedes any call placed after it.

## Running a Body

`Interpreter::run` takes the body, the request context and the action input, `None` for a loader. On success the draft is committed and `Outcome::written` lists the keys. On failure the session is untouched.

```rust
let outcome = Interpreter::default().run(&body, &ctx, Some(input)).await?;
assert_eq!(outcome.written, vec!["cart".to_owned()]);
```

## Reading and Writing the JSON Form

The plan file carries a body as JSON. Round trips are exact.

```rust
use snapfire_fsr_ir::ast::{from_json, to_json};

let text = to_json(&body);
let back = from_json(&text)?;
assert_eq!(back, body);
```

The shape is serde's externally tagged form, one key per node kind in `snake_case`:

```json
[{"let": {"name": "catalog", "expr": {"call": {"service": "shopping", "method": "listProducts", "args": []}}}},
 {"return": {"object": [{"field": ["products", {"var": "catalog"}]}]}}]
```

## Binding a Body to the Runtime

`IrSource` answers a data source id and requires the body to return an object. `IrAction` answers an action id and returns whatever the body returns. Both are the runtime's own traits, so they register wherever a Rust closure would.

```rust
use std::sync::Arc;
use snapfire_fsr_ir::{IrAction, IrSource};
use snapfire_fsr_runtime::{ActionRegistry, DataSources};

let mut sources = DataSources::new();
sources.insert("cart_loader", Arc::new(IrSource::new("cart_loader", cart_loader())));

let mut actions = ActionRegistry::default();
actions.insert("checkout", Arc::new(IrAction::new(checkout())));
```

## Pinning the Clock

`Expr::Now` reads the interpreter's `Clock`. The default is the system clock in milliseconds. A test pins it so a body that stamps a time is reproducible.

```rust
use std::sync::Arc;
use snapfire_fsr_ir::{Clock, Interpreter};

struct Fixed;
impl Clock for Fixed {
  fn now(&self) -> i128 { 1_700_000_000_000 }
}

let interpreter = Interpreter::with_clock(Arc::new(Fixed));
let source = IrSource::new("stamped", body).with_interpreter(interpreter);
```

## Testing Against a Mock Service Layer

A body sees services only through `RequestCtx::services`, so a `ServiceCaller` that answers by name is the whole test double. `snapfire_fsr_service::MockTransport` behind a `Services` registry does the same with contract checking; the plain caller below is enough when no contract is loaded.

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, RequestCtx, ServiceCaller, ServiceError, ServiceHandle, SessionCell};

struct Answers(ValueMap);

impl ServiceCaller for Answers {
  fn call(&self, service: &str, method: &str, _args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let answer = self.0.get(&format!("{service}.{method}")).cloned();
    let (service, method) = (service.to_owned(), method.to_owned());
    Box::pin(async move { answer.ok_or_else(|| ServiceError::new(FailureKind::NotFound, service, method, "no answer")) })
  }
}

let mut answers = ValueMap::new();
answers.insert("shopping.listProducts".into(), Value::Seq(vec![]));
let ctx = RequestCtx {
  params: Default::default(),
  session: SessionCell::new(ValueMap::new(), None),
  csrf: None,
  services: ServiceHandle::new(Arc::new(Answers(answers))),
};
```

## How Calls Are Ordered

Consecutive `let` statements that read none of each other's names, two or more of which call a service, are issued together. A `let` that reads an earlier one waits for it. Nothing else reorders.

```rust
let body = vec![
  Stmt::Let { name: "a".into(), expr: Expr::call("shopping", "listProducts", vec![]) },
  Stmt::Let { name: "b".into(), expr: Expr::call("shopping", "getProduct", vec![("id", Expr::lit_int(1))]) },
  Stmt::Let { name: "c".into(), expr: Expr::call("shopping", "getProduct", vec![("id", Expr::var("a").index(Expr::lit_int(0)).field("id"))]) },
];
```

`a` and `b` are in flight at once. `c` starts when `a` has returned.

## Error Handling

A body fails with `Fail`, which carries a `FailureKind` and a message. A guard produces the kind it named. A service error keeps the kind the service layer mapped. `BigInt` or `Number` of an unparseable string is `Invalid`. Anything the build should have caught, an operand type mismatch, an unbound name, an index into a non-collection, is `Internal`.

```rust
use snapfire_fsr_ir::Fail;
use snapfire_fsr_runtime::FailureKind;

match Interpreter::default().run(&body, &ctx, None).await {
  Ok(outcome) => render(outcome.value),
  Err(Fail { kind: FailureKind::Invalid, message }) => reject(message),
  Err(Fail { kind, message }) => degrade(kind.http_status(), message),
}
```

`IrSource` maps a `Fail` onto `LoadError` with the source id and the message. `IrAction` maps it onto `ActionError` with the kind preserved. `ast::from_json` fails with `ParseError` wrapping the serde error.
