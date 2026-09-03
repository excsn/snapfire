# 201. Graduating to Rust

The question this chapter answers: how does one loader, action or route move from TypeScript into Rust without the rest of the application noticing and what stops that from happening by accident?

**For:** platform developers.

## The unit is one name

The plan file names things: sources by id, actions by id, routes by pattern, rendered modules by module id. The host binds each name to what answers it. That is the whole of the graduation story, because Rust can answer a name too. The builder has one method per kind:

```rust
Host::from(root)?
  .source_override("pricing", |ctx| async move { ... })
  .action_override("cart.checkout", |ctx, input| async move { ... })
  .route("/about", about_plan())
  .build()?
```

Taking over `pricing` leaves every other source in TypeScript. The page that reads `pricing` receives the same props, the tests that mock the service still pass, the report changes one row from `lowered` to `rust override`. Nothing else in the application knows, because from the caller's side a Rust function answering a name is the same rows the interpreter would have produced.

The same seam exists for evaluators. `.evaluator(predicate, evaluator)` answers a set of modules with a Rust renderer, which is how the shell is rendered and how a Tera template can sit beside a React page; `.shell(evaluator)` replaces the document itself.

## The binding rule

A name can be claimed by the plan file and by Rust; the host refuses to guess which wins. The rule has three parts and every one produces a boot error with the name rather than a silent choice.

**Adding is additive.** `.source("pricing", f)` on a name the plan file does not lower simply binds it. A route added with `.route` on a pattern the plan file does not have is a new route.

**Replacing is deliberate.** `.source("cart", f)` on a name the plan file lowered is refused: "claimed by the plan file and by Rust; mark the Rust one as an override". Write `.source_override("cart", f)` and the override is what runs. The word is the whole point: a reader of `main.rs` can see every place TypeScript was overruled.

**Overriding nothing is a mistake.** `.source_override("carts", f)` when no such source exists is refused too, since it almost always means a rename left the override dangling; a dangling override that silently bound nothing would leave the TypeScript body running while everyone believed Rust had taken it.

The report says which is which:

```
sources   cart                   rust override
          index                  lowered
routes    /about                 rust
```

## What graduates and what does not

A source or an action graduates when its shape is worth keeping and its body is not: a hot loader that wants a cache Rust already has, an action that needs a library the IR will never grow, a call pattern that wants a connection the host owns. The Rust function receives the same `RequestCtx` the interpreter did, with `params`, `query`, the session cell and the service handle, so it can call the same services through the same registry and the same interceptors; its session writes persist the same way.

A route graduates when its plan is not a page: a redirect, a health check, a stream of something that is not a document. `about_plan()` in the storefront is the smallest case, a plan node whose module is a component with no loader, added in Rust to show that a route is a binding rather than a fixed artifact.

A rendered module graduates when a Rust evaluator renders it better than the lowered tree does, which today means a template engine. The evaluator returns the same node kinds the assembler already stitches, so a Tera page and a React page compose in one document.

What never graduates is the contract. Rust code calls services through the same registry, checked against the same document, because the boundary is the artifact and not the language on either side of it.

## The lab

In the storefront's `main.rs`, add `.source("cart", |_ctx| async { Ok(Data::new()) })` before `.build()` and run it. Boot refuses: `cart` is claimed by the plan file and by Rust. Change it to `.source_override` and boot again: the report's `cart` row now reads `rust override`; the cart page renders an empty cart whatever the session holds, since your function answers the name. Then rename it to `.source_override("carts", ...)`: refused again, since the plan file lowers no such source. Remove the line.
