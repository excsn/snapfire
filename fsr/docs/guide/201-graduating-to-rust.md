# 201. Graduating to Rust

The question this chapter answers: how does one loader, action or route move from TypeScript into Rust without the rest of the application noticing and what stops that from happening by accident?

**For:** platform developers.

## Overriding by name

The plan file names things: sources by id, actions by id, routes by pattern, rendered modules by module id. The host binds each name to what answers it and Rust can answer any of those names. The builder has one method per kind:

```rust
Host::from(root)?
  .source_override("pricing", |ctx| async move { ... })
  .action_override("cart.checkout", |ctx, input| async move { ... })
  .route("/about", about_plan())
  .build()?
```

Taking over `pricing` leaves every other source in TypeScript. The page that reads `pricing` receives the same props, the tests that mock the service still pass and the report changes one row from `lowered` to `rust override`. Nothing else in the application changes, because a Rust function answering a name returns the same rows the interpreter would have produced.

Evaluators can be replaced the same way. `.evaluator(predicate, evaluator)` answers a set of modules with a Rust renderer, which is how the shell is rendered and how a Tera template can sit beside a React page; `.shell(evaluator)` replaces the document itself.

## The binding rule

A name can be claimed by the plan file and by Rust and the host does not guess which wins. Three rules decide it. Breaking one is a boot error that names the name, never a silent choice.

**A new name binds directly.** `.source("pricing", f)` on a name the plan file does not lower simply binds it. A route added with `.route` on a pattern the plan file does not have is a new route.

**Replacing a lowered name needs `_override`.** `.source("cart", f)` on a name the plan file lowered is refused: "claimed by the plan file and by Rust; mark the Rust one as an override". Write `.source_override("cart", f)` and the override is what runs. The `_override` suffix lets a reader of `main.rs` find every place Rust replaced TypeScript.

**An override with nothing to replace is refused.** `.source_override("carts", f)` when no such source exists is refused, since it almost always means a rename left the override dangling; a dangling override that silently bound nothing would leave the TypeScript body running while everyone believed Rust had taken it.

The report says which is which:

```
sources   cart                   rust override
          index                  lowered
routes    /about                 rust
```

## What graduates and what does not

A source or an action graduates when its name and signature should stay and its implementation should move to Rust: a hot loader that wants a cache Rust already has, an action that needs a library the IR will never grow, a call pattern that wants a connection the host owns. The Rust function receives the same `RequestCtx` the interpreter did, with `params`, `query`, the session cell and the service handle, so it can call the same services through the same registry and the same interceptors; its session writes persist the same way.

A route graduates when its plan is not a page: a redirect, a health check, a stream of something that is not a document. `about_plan()` in the storefront is the smallest case, a plan node whose module is a component with no loader, added in Rust to show that a route is a binding rather than a fixed artifact.

A rendered module graduates when a Rust evaluator renders it better than the lowered tree does, which in practice means a template engine. The evaluator returns the same node kinds the assembler already stitches, so a Tera page and a React page compose in one document.

The contract never moves to Rust. Rust code calls services through the same registry and is checked against the same document.

## Calling Rust from a body

The overrides above take over a whole name. For something smaller, such as a computation that belongs in Rust while the loader stays in TypeScript, mark an `impl` block and call it:

```rust
#[native]
impl Digest {
  pub fn words(&self, bodies: Vec<String>) -> i64 {
    bodies.iter().map(|b| b.split_whitespace().count() as i64).sum()
  }
}
```

```rust
Host::from(...).native("digest", Arc::new(Digest))
```

```ts
export async function load({ params, services, native }: Ctx<"/room/{id}">) {
  const transcript = await services.rooms.getRoom({ id: params.id });
  const bodies = transcript.messages.map((m) => m.body);
  return { ...transcript, words: native.digest.words({ bodies }) };
}
```

A native call is synchronous. The build types a `fn` as returning a value and an `async fn` as returning a promise, so `words` returns a number and needs no `await`.

Only what the block declares `pub` crosses into TypeScript, so a module holding another calls it as an ordinary method and that one never appears in TypeScript. A native call has no contract, transport or interceptor chain, because it does not leave the process. The declaration comes from the signature: `fsr` reads the Rust with `syn` the way it reads your TypeScript with swc, before anything compiles, so the two cannot drift.

Use a service instead for something over a wire that a document already describes. Use one too when you want the cache and the interceptors that come with a call crossing a boundary.

## A service written in Rust

A service with no document behind it is still declared once. Mark the `impl` block `#[service]` instead of `#[native]` and the attribute writes the transport and the contract off the same signatures:

```rust
#[derive(Record)]
pub struct Server {
  pub name: String,
  pub load: f64,
}

#[service]
impl Fleet {
  #[cache(ttl = "15s", tags = ["servers"], scope = "shared")]
  pub fn list(&self, section: String) -> Result<Vec<Server>, ServiceError> { /* ... */ }

  #[writes("servers")]
  pub fn add(&self, name: String, load: f64) -> Result<Added, ServiceError> { /* ... */ }
}
```

```rust
Host::from(...).service(Arc::new(fleet))
```

```ts
export async function load({ params, services }: Ctx<"/dash/{section}">) {
  return { servers: await services.fleet.list({ section: params.section }) };
}
```

Unlike a native call, a service call crosses the contract, so its arguments and its answer are checked, the interceptors run, a `#[cache]` policy is honoured and a `Result<T, ServiceError>` reaches the body as the failure it names. A method that takes a parameter typed `Caller` is told who is calling, the identity the session resolved and the metadata the interceptors added, without the body passing anything and without the contract listing it; `caller.require()?` refuses an anonymous call. `fsr build` reads the block the way it reads a native one and writes the contract to `generated/contracts/rust.json`, which is what types `services.fleet` in TypeScript; the host merges the contract the attribute wrote over that file and refuses a disagreement at boot, so a build that fell behind the Rust is a boot failure rather than a call that fails later.

## The lab

In the storefront's `main.rs`, add `.source("cart", |_ctx| async { Ok(Data::new()) })` before `.build()` and run it. Boot refuses: `cart` is claimed by the plan file and by Rust. Change it to `.source_override` and boot again: the report's `cart` row now reads `rust override`; the cart page renders an empty cart whatever the session holds, since your function answers the name. Then rename it to `.source_override("carts", ...)`: refused again, since the plan file lowers no such source. Remove the line.
