# Usage Guide: snapfire_fsr_lower

How to lower a loader or an actions module, what the recogniser accepts, how it reads the context and how residue is reported.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Lowering a Loader](#lowering-a-loader)
* [Lowering Actions](#lowering-actions)
* [Reading the Context](#reading-the-context)
* [Calling Services](#calling-services)
* [Writing the Session](#writing-the-session)
* [Guarding](#guarding)
* [Using Lambdas](#using-lambdas)
* [Reading a Schema](#reading-a-schema)
* [Folding Session Defaults](#folding-session-defaults)
* [Reading Residue](#reading-residue)
* [Error Handling](#error-handling)

## Core Concepts

* **Loader module** exports `load`, an async function or arrow taking the context.
* **Actions module** exports constants built with `action(body)` or `action<Input>(body)`, each taking the context.
* **Context** is the body's first parameter, either `ctx` or a destructuring of `params`, `query`, `session`, `services`, `identity`, `input` and `now`.
* **Read** is a context field the IR knows: `params.x`, `query.x`, `session.x`, `identity.x`, `input`, `now`.
* **Call** is `await services.<service>.<method>({ ...args })`, the only call shape besides builtins.
* **Builtin** is one of `String`, `Number`, `BigInt`, `Object.entries`, `Object.keys`, `Object.values`, `.length` and the array methods `map`, `filter`, `reduce`, `find`, `some`, `every`.
* **Lambda** is an arrow function passed to an array method, one expression or a block that only returns one.
* **Number and bigint** lower differently: `1` is a float, `1n` is an integer, as in TypeScript.
* **Residue** is anything else, reported with its position and never guessed at.
* **Imports** are ignored; a name that resolves to one is residue at its use, not at the import.
* **Schema** is a module of exported interfaces and string-literal unions that `read_schema` turns into contract types.

## Quick Start

```rust
use snapfire_fsr_lower::{lower_actions, lower_loader};

let body = lower_loader("routes/index/loader.ts", r#"
  export async function load({ params, services }) {
    return { products: await services.shopping.listProducts({ tag: params.tag }) };
  }
"#)?;

let actions = lower_actions("routes/cart/actions.ts", r#"
  export const checkout = action(async ({ session, services }) => {
    const order = await services.shopping.placeOrder({ lines: Object.entries(session.cart ?? {}) });
    session.cart = {};
    return order;
  });
"#)?;
assert_eq!(actions[0].export, "checkout");
```

## Lowering a Loader

`lower_loader` finds the exported `load` and lowers its body. The context parameter may be destructured or named.

```rust
let destructured = lower_loader("loader.ts", "export async function load({ params }) { return { id: params.id }; }")?;
let named = lower_loader("loader.ts", "export const load = async (ctx) => { return { id: ctx.params.id }; };")?;
assert_eq!(destructured, named);
```

A module with no `load` is `LowerError::MissingExport`.

## Lowering Actions

`lower_actions` lowers every exported `action(...)` in file order. The type argument, when present, is recorded as the input type name for the contract.

```rust
let actions = lower_actions("actions.ts", r#"
  export const addToCart = action<AddToCart>(async ({ input, session }) => {
    session.cart[String(input.product_id)] = input.quantity;
  });
"#)?;
assert_eq!(actions[0].input.as_deref(), Some("AddToCart"));
```

Exports that are not actions are skipped, so a module may also export types and helpers the body does not call.

## Reading the Context

Each context field lowers to a read. Reading a root as a whole is residue, since a body has no use for it.

```ts
params.id             // Expr::Param("id")
query.tag             // Expr::Query("tag")
session.cart          // Expr::Session("cart")
identity.subject      // Expr::Identity(["subject"])
identity.claims.role  // Expr::Identity(["claims", "role"])
input.quantity        // Expr::Field(Expr::Input, "quantity")
now                   // Expr::Now, also ctx.now
```

## Calling Services

A call names a service and a method under `services` and passes one object literal. Shorthand properties and `await` are understood; a spread or a second argument is residue.

```ts
await services.shopping.getProduct({ id: BigInt(params.id) })
await services.shopping.placeOrder({ lines })
```

## Writing the Session

Assignments and deletes under `session` lower to session statements. Anything under another root is residue.

```ts
session.cart = {};                       // Stmt::SessionSet { key: "cart", path: [] }
session.cart[key] = wanted;              // path: [Var("key")]
session.prefs.theme = "dark";            // path: [Lit("theme")]
delete session.cart[key];                // Stmt::SessionDelete
```

## Guarding

`if (cond) fail("kind", "message")`, bare or in a one-statement block, lowers to a guard. A bare `fail(...)` statement is a guard that always fires. The kind and the message must be string literals.

```ts
if (lines.length === 0) fail("invalid", "the cart is empty");
if (!identity) { fail("unauthorized", "sign in first"); }
```

## Using Lambdas

Array methods take an arrow function written in place. Its parameters may be names, an array pattern or an object pattern of plain names; the pattern reads as indexes or fields of a positional parameter named `$0`, `$1` and so on.

```ts
catalog.filter((p) => session.cart[String(p.id)])
Object.entries(session.cart).map(([id, quantity]) => ({ product_id: BigInt(id), quantity }))
lines.reduce((sum, l) => sum + l.price_cents * l.quantity, 0n)
```

A lambda body with statements other than one `return` is residue.

## Reading a Schema

`read_schema` reads every exported interface and string-literal union. Fields map onto the contract: `string`, `number`, `bigint`, `boolean`, `null`, `T[]`, `Array<T>`, `Record<string, T>`, typed arrays, named references, `?` and `| null` as optional.

```rust
use snapfire_fsr_lower::read_schema;

let types = read_schema("schemas/session.ts", r#"
  export interface Session { cart?: Record<string, bigint> }
  export type Theme = "dark" | "light";
"#)?;
assert_eq!(types[0].name, "Session");
```

An inline object type, a union of two real types or a generic reference is residue with its line, since the contract has no shape for it. `bigint` becomes `I64` and `number` becomes `F64`.

## Folding Session Defaults

`export const defaults` beside the `Session` interface names the value a body reads when a key is absent. `read_session_defaults` lowers the literals. `lower_loader_with` or `lower_actions_with` fold them into every read of that key as `session.key ?? default`.

```rust
use snapfire_fsr_lower::{lower_loader_with, read_session_defaults};

let defaults = read_session_defaults("schemas/session.ts", r#"
  export interface Session { cart: Record<string, bigint> }
  export const defaults: Session = { cart: {} };
"#)?;
let body = lower_loader_with("loader.ts", source, &defaults)?;
```

`lower_loader` and `lower_actions` are the same with no defaults. A key without a default reads plain.

## Reading Residue

A `Residue` names the file, the one-based line and column and the construct. Its `Display` is the diagnostic line a build prints.

```rust
use snapfire_fsr_lower::LowerError;

match lower_loader("routes/x/loader.ts", source) {
  Ok(body) => emit(body),
  Err(LowerError::Residue(r)) => eprintln!("{r}"),
  Err(other) => eprintln!("{other}"),
}
```

```
routes/x/loader.ts:5:18: `slugify` is not bound here; an import the build cannot follow, or a name from outside the body
```

## Error Handling

`LowerError` has three variants. `Parse` carries the parser's message with its position. `MissingExport` names the export a loader module lacks. `Residue` wraps a `Residue`, which is also its own error type with `file`, `line`, `column` and `message` fields.

```rust
use snapfire_fsr_lower::{LowerError, Residue};

let outcome = lower_loader(file, source);
let residue: Option<Residue> = match outcome {
  Err(LowerError::Residue(r)) => Some(r),
  _ => None,
};
```
