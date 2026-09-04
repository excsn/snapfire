# Usage Guide: snapfire_fsr

How to bind a plan file, add and replace routes, answer names in Rust, take lowered names back and read the report that says who answers what.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Binding a Plan File](#binding-a-plan-file)
* [Writing a Plan in Rust](#writing-a-plan-in-rust)
* [Adding and Replacing Routes](#adding-and-replacing-routes)
* [Answering a Data Source](#answering-a-data-source)
* [Taking a Lowered Name Back](#taking-a-lowered-name-back)
* [Answering an Action](#answering-an-action)
* [Rendering a Module in Rust](#rendering-a-module-in-rust)
* [Checking Action Input](#checking-action-input)
* [Caching and Services](#caching-and-services)
* [Reading the Report](#reading-the-report)
* [The Binding Rule](#the-binding-rule)
* [Error Handling](#error-handling)

## Core Concepts

* **Plan file** is `generated/plan.json`, read by `snapfire_fsr_plan`; `App::from_manifest` starts from its text.
* **Route** is a pattern and a plan, from the file or from Rust; a pattern claimed twice is refused unless the second is an override.
* **Plan** is the tree a route resolves to, written in Rust with the `Plan` builder or read from the file.
* **Data source** is a name a plan node loads through, answered by a lowered loader or a Rust function.
* **Action** is a name the browser calls, answered by a lowered body or a Rust function, its input checked against the contract when the row names a type.
* **Lowered row** is a source, action or component the build carried as data; it binds itself to the interpreter unless Rust overrides the name.
* **Override** is Rust taking a lowered name back; overriding a name the file does not have is an error, since it means a rename left it dangling.
* **Evaluator** is what renders a module; lowered components get the IR evaluator and Rust registers others by predicate.
* **Owner** is who answers a name: the plan file, a lowered row, Rust or a Rust override.
* **Report** is the list of routes, sources, actions and rendered modules with their owners, printed at boot.
* **App** is what `build` returns: matcher, resolver, runtime, services, actions and the report, everything a request needs.

## Quick Start

```rust
use snapfire_fsr::App;

fn main() -> Result<(), snapfire_fsr::BindError> {
  let text = std::fs::read_to_string("app/generated/plan.json").expect("run `fsr build app` first");
  let app = App::from_manifest(&text)?.build()?;
  print!("{}", app.report);
  Ok(())
}
```

The report of an application that owns nothing in Rust:

```
routes    /                      plan file
          /cart                  plan file
sources   index                  lowered
          cart                   lowered
actions   cart.addToCart         lowered
rendered  routes/index/page.tsx#default lowered
```

## Binding a Plan File

`from_manifest` reads the routes, remembers the lowered rows and the declared actions and hands back the builder. `build` binds every lowered row Rust did not override and refuses anything unanswered.

```rust
let builder = App::from_manifest(&text)?;
let app = builder.build()?;
assert!(app.report.sources.iter().all(|(_, owner)| *owner == snapfire_fsr::Owner::Lowered));
```

## Writing a Plan in Rust

`Plan` reads the way the tree looks. Node ids are assigned in tree order at build time, so nothing here numbers anything.

```rust
use snapfire_fsr::Plan;

let about = Plan::of("shell#document").slot("content", Plan::of("src/About.tsx#default"));
let product = Plan::of("shell#document").slot(
  "content",
  Plan::of("routes/product/[id]/page.tsx#default").source("product").deferred().fallback("routes/product/[id]/loading.tsx#default").error("routes/error.tsx#default").cache_key("product"),
);
```

A `PlanNode` built by hand is accepted wherever a `Plan` is, through `IntoPlan`.

```rust
use snapfire_fsr_core::{ModuleId, NodeId, PlanNode};

let node = PlanNode::new(NodeId(0), "shell#document".parse::<ModuleId>().unwrap());
let builder = App::builder(snapfire_fsr::Routes::new()).route("/bare", node);
```

## Adding and Replacing Routes

`route` adds a pattern the file does not have; `route_override` replaces one it has. Adding a pattern the file already claims is refused at `build`.

```rust
let app = App::from_manifest(&text)?
  .route("/about", Plan::of("shell#document").slot("content", Plan::of("src/About.tsx#default")))
  .route_override("/", Plan::of("shell#document").slot("content", Plan::of("src/Landing.tsx#default")))
  .build()?;
```

Routes can also be written with no file at all.

```rust
use snapfire_fsr::Routes;

let routes = Routes::new().add("/", Plan::of("shell#document")).add("/health", Plan::of("src/Health.tsx#default"));
let app = App::builder(routes).evaluator(|_| true, shell).build()?;
```

The tree for a path nothing matches is not a route. The plan file carries one when the application has a `routes/not-found.tsx`; `Routes::not_found` sets or replaces it from Rust. The host renders it with status 404 and `params.path`.

```rust
let routes = Routes::from_manifest(&text)?.not_found(Plan::of("shell#document").slot("content", Plan::of("src/Missing.tsx#default")));
```

## Answering a Data Source

`source` takes an async function of the request context; `source_impl` takes a `DataSource`. A source the plan names and nothing answers is a `BindError::Unbound` at `build`.

```rust
use snapfire_fsr_core::Data;
use snapfire_fsr_runtime::{LoadError, RequestCtx};

let app = App::from_manifest(&text)?
  .source("pricing", |ctx: RequestCtx| async move {
    let mut data = Data::new();
    data.insert("currency".into(), snapfire_fsr_core::Value::str(ctx.query.get("currency").cloned().unwrap_or_else(|| "USD".into())));
    Ok::<Data, LoadError>(data)
  })
  .build()?;
```

## Taking a Lowered Name Back

A lowered source is bound by default. `source_override` takes it back; `source` on the same name is refused, since two answers for one name is a mistake, not a preference.

```rust
let app = App::from_manifest(&text)?
  .source_override("cart", |ctx: RequestCtx| async move { cart_from_postgres(ctx).await })
  .build()?;
assert!(app.report.sources.contains(&("cart".to_owned(), snapfire_fsr::Owner::RustOverride)));
```

```rust
let refused = App::from_manifest(&text)?.source("cart", |_ctx| async { Ok(Data::new()) }).build();
assert!(matches!(refused, Err(snapfire_fsr::BindError::Claimed(name)) if name == "cart"));
```

## Answering an Action

The same three shapes: `action` for a Rust function, `action_impl` for an `ActionHandler`, `action_override` to take a lowered one back. An action the file declares as `rust` must be answered or `build` refuses with `UnboundAction`.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::ActionError;

let app = App::from_manifest(&text)?
  .action("cart.checkout", |ctx: RequestCtx, input: Value| async move { place_order(ctx, input).await })
  .action_override("cart.addToCart", |ctx: RequestCtx, input: Value| async move { add_with_limits(ctx, input).await })
  .build()?;
```

## Answering a Handler

A `route.ts` lowers to handler rows the plan file names by method and pattern. `handler` adds one written in Rust beside them, `handler_override` takes a lowered one back and `handler_impl` takes an `ActionHandler`. The report lists each as `METHOD pattern`.

```rust
let app = App::from_manifest(&text)?
  .handler("GET", "/api/health", |_ctx, _input| async { Ok(Value::str("ok")) })
  .handler_override("POST", "/api/cart", |ctx, input| async move { add_to_cart(ctx, input).await })
  .build()?;
```

A host matches handlers before pages: `app.handlers.match_request("GET", "/api/health")` gives the id and the parameters, `app.handlers.dispatch` runs it with a request context and the request body as its input.

## Answering the Middleware

`middleware.ts` lowers to `Manifest.middleware`. `middleware` binds one written in Rust when the plan has none and `middleware_override` replaces the lowered one. It is called with the request line, `{ method, path }`, as its input; the host reads its value.

```rust
let app = App::from_manifest(&text)?
  .middleware_override(|ctx, request| async move {
    let mut out = ValueMap::new();
    if ctx.session.identity().is_none() {
      out.insert("redirect".into(), Value::str("/login"));
    }
    let _ = request;
    Ok(Value::Map(out))
  })
  .build()?;
```

## Rendering a Module in Rust

Lowered components get the IR evaluator automatically and appear under `rendered` as `lowered`. Anything else is an evaluator registered by predicate, checked in registration order.

```rust
use std::sync::Arc;
use snapfire_fsr_core::ModuleId;

let app = App::from_manifest(&text)?
  .evaluator(|m: &ModuleId| m.to_string() == "shell#document", Arc::new(my_shell))
  .evaluator(|m: &ModuleId| m.path().ends_with(".tera"), Arc::new(tera))
  .build()?;
```

## Checking Action Input

A lowered action whose row names an input type is wrapped so the value is checked against the contract before the body runs; a bad input is an `Invalid` failure that never reaches the body. Pass the contract or `build` refuses with `NoContract`.

```rust
let contract = snapfire_fsr_service::Contract::from_json(&std::fs::read_to_string("app/generated/contract.json")?)?;
let app = App::from_manifest(&text)?.contract(contract).build()?;
```

## Caching and Services

`cache` gives the runtime a `NodeCache` for subtrees with a `cache_key`; `services` is the registry a lowered body's `services.<name>.<method>` reaches. Without `services`, an empty registry is built and any call fails as unavailable.

```rust
use std::time::Duration;
use snapfire_fsr_runtime::FibreCache;

let app = App::from_manifest(&text)?
  .cache(Arc::new(FibreCache::bounded(10_000, Duration::from_secs(60))))
  .services(services)
  .build()?;
```

## Reading the Report

Four sections, each name with its owner, routes sorted by pattern and the rest in binding order.

```rust
for (pattern, owner) in &app.report.routes {
  println!("{pattern} is answered by the {}", owner.as_str());
}
```

```
routes    /                      plan file
          /about                 rust
sources   cart                   rust override
          index                  lowered
actions   cart.addToCart         lowered
          cart.checkout          rust
rendered  routes/index/page.tsx#default lowered
```

## The Binding Rule

`build` checks, in order: every override names something the file has; every lowered source binds unless overridden and a plain claim on a lowered name is refused; the same for actions; every source a plan names has exactly one owner; every declared action is answered; every pattern is one the matcher accepts. The report is only written once all of that holds, so a printed report is a running application.

```rust
match App::from_manifest(&text)?.build() {
  Ok(app) => print!("{}", app.report),
  Err(e) => eprintln!("refusing to start: {e}"),
}
```

## Error Handling

`BindError` is what `from_manifest` and `build` return. Every variant names the offending name and says what to do about it.

```rust
use snapfire_fsr::BindError;

match App::from_manifest(&text).and_then(|b| b.build()) {
  Ok(app) => run(app),
  Err(BindError::Unbound { name }) => eprintln!("the plan needs `{name}`; write a page.loader.ts or a `source(\"{name}\", ..)`"),
  Err(BindError::Claimed(name)) => eprintln!("`{name}` is lowered; use `source_override`"),
  Err(BindError::UnboundAction { id }) => eprintln!("the plan declares `{id}`; answer it with `action`"),
  Err(BindError::Plan(e)) => eprintln!("plan file: {e}"),
  Err(e) => eprintln!("{e}"),
}
```
