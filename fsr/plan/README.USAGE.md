# Usage Guide: snapfire_fsr_plan

How to read, write and build a plan file, what each row means and how a host turns the file into the trees and bindings it needs.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Reading a Plan File](#reading-a-plan-file)
* [Building a Manifest](#building-a-manifest)
  * [Routes](#routes)
  * [Source Rows](#source-rows)
  * [Action Rows](#action-rows)
  * [Component Rows](#component-rows)
* [Converting to Runtime Trees](#converting-to-runtime-trees)
* [Listing What a Host Must Bind](#listing-what-a-host-must-bind)
* [Reading the File by Hand](#reading-the-file-by-hand)
* [Format Versions](#format-versions)
* [Error Handling](#error-handling)

## Core Concepts

* **Manifest** is the file in memory: a version, the routes, the source rows, the action rows and the component rows.
* **Route entry** is a pattern and the plan it resolves to, in file order, so entry ids are stable across boots.
* **Node** is the serialized shape of a runtime `PlanNode`: an id, a module, an optional source, whether it is deferred, its fallback and error modules, its cache key and its children by slot.
* **Module id** is `path#export`, the same string the browser's island registry and the build's report use.
* **Source row** is one data source the build knows about, with an owner: `lowered` rows carry a body the host binds unless Rust overrides the name, `rust` rows are declarations Rust must answer.
* **Action row** is the same for an action, plus the name of the input type the host checks a call against before the body runs.
* **Component row** is a module the build lowered to a render tree, which the host renders in Rust and the browser hydrates over.
* **Owner** is `RowOwner`: `lowered`, `engine` or `rust`.
* **Body** is `snapfire_fsr_ir::Body`, a lowered loader or action, carried inline.
* **Format version** is the `version` field; the crate reads from `OLDEST_READABLE` to `FORMAT_VERSION` and writes the latter.

## Quick Start

```rust
use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_plan::{ActionEntry, Manifest, Node, RouteEntry, SourceEntry};

fn main() -> Result<(), snapfire_fsr_plan::PlanError> {
  let mut shell = PlanNode::new(NodeId(0), "shell#document".parse::<ModuleId>().unwrap());
  let mut page = PlanNode::new(NodeId(1), "routes/cart/page.tsx#default".parse::<ModuleId>().unwrap());
  page.data_source = Some(DataSourceId("cart".into()));
  shell.children.push((SlotName("content".into()), page));

  let manifest = Manifest::new(vec![RouteEntry { pattern: "/cart".into(), plan: Node::from_plan(&shell) }])
    .with_sources(vec![SourceEntry::rust("cart")])
    .with_actions(vec![ActionEntry::rust("cart.addToCart").with_input("AddToCart")]);
  let text = manifest.to_json();

  let read = Manifest::from_json(&text)?;
  assert_eq!(read.sources(), vec!["cart".to_owned()]);
  assert_eq!(read.routes()?[0].0, "/cart");
  Ok(())
}
```

## Reading a Plan File

`from_json` parses, checks the version and refuses a `lowered` row with no body. Everything else is checked when the trees are built.

```rust
let text = std::fs::read_to_string("app/generated/plan.json")?;
let manifest = snapfire_fsr_plan::Manifest::from_json(&text)?;
println!("format {} with {} routes", manifest.version, manifest.routes.len());
```

## Building a Manifest

### Routes

`Manifest::new` takes the route entries and stamps `FORMAT_VERSION`. A `Node` is written by hand or taken from a runtime tree with `from_plan`; optional fields are absent in the file rather than null.

```rust
use snapfire_fsr_plan::{Child, Manifest, Node, RouteEntry};

let plan = Node {
  id: 0,
  module: "shell#document".into(),
  source: None,
  deferred: false,
  fallback: None,
  error: None,
  cache_key: None,
  children: vec![Child { slot: "content".into(), node: Node { id: 1, module: "routes/index/page.tsx#default".into(), source: Some("index".into()), deferred: false, fallback: None, error: Some("routes/error.tsx#default".into()), cache_key: None, children: Vec::new() } }],
};
let manifest = Manifest::new(vec![RouteEntry { pattern: "/".into(), plan }]);
```

A deferred node names its fallback; a node that may fail names its error module. Both are modules a host must be able to evaluate, so `modules` lists them. `with_not_found` carries one more tree, outside the routes, that a host renders with status 404 for a path nothing matches; `not_found()` converts it the way `routes()` converts a route's.

```rust
let manifest = manifest.with_not_found(Some(Node { id: 0, module: "shell#document".into(), source: None, deferred: false, fallback: None, error: None, cache_key: None, children: vec![Child { slot: "content".into(), node: Node { id: 1, module: "routes/not-found.tsx#default".into(), source: None, deferred: false, fallback: None, error: None, cache_key: None, children: Vec::new() } }] }));
```

```rust
let mut node = Node { id: 1, module: "routes/product/[id]/page.tsx#default".into(), source: Some("product".into()), deferred: true, fallback: Some("routes/product/[id]/loading.tsx#default".into()), error: None, cache_key: None, children: Vec::new() };
node.cache_key = Some("product".into());
```

### Source Rows

A row per data source. `lowered` carries the body `fsr build` produced; `rust` is a declaration the host refuses to start without an answer to.

```rust
use snapfire_fsr_plan::{RowOwner, SourceEntry};

let lowered = SourceEntry::lowered("cart", "routes/cart/loader.ts", body);
let declared = SourceEntry::rust("pricing");
assert_eq!(lowered.owner, RowOwner::Lowered);
let manifest = manifest.with_sources(vec![lowered, declared]);
```

### Action Rows

The same shape, plus `input`, the name of the contract type a call's input is checked against before the body runs.

```rust
use snapfire_fsr_plan::ActionEntry;

let mut add = ActionEntry::lowered("cart.addToCart", "routes/cart/actions.ts", body);
add.export = Some("addToCart".into());
add.input = Some("AddToCart".into());
let checkout = ActionEntry::rust("cart.checkout").with_input("Checkout");
let manifest = manifest.with_actions(vec![add, checkout]);
```

### Component Rows

One per module the build lowered to a render tree. The host renders these in Rust; a module without a row mounts in the browser only.

```rust
use snapfire_fsr_plan::ComponentEntry;

let manifest = manifest.with_components(vec![ComponentEntry { module: "routes/cart/page.tsx#default".into(), body: component }]);
```

## Converting to Runtime Trees

`routes` builds a `PlanNode` per route, in file order, refusing an empty pattern, a module that is not `path#export`, a node id used twice within one route or a slot used twice on one node. The same node id in two routes is fine.

```rust
for (pattern, plan) in manifest.routes()? {
  println!("{pattern} -> {} nodes", count(&plan));
}
```

## Listing What a Host Must Bind

`sources` is every data source named in any tree, the not-found tree included, once, in tree order; `modules` is every module named anywhere, fallbacks, error modules and the not-found tree included; `action_ids` is every action row. `lowered_sources` and `lowered_actions` pick out the rows that carry a body.

```rust
let must_answer: Vec<&str> = manifest.sources.iter().filter(|row| row.owner == snapfire_fsr_plan::RowOwner::Rust).map(|row| row.id.as_str()).collect();
let must_render = manifest.modules();
for row in manifest.lowered_actions() {
  println!("{} lowered from {}", row.id, row.module.as_deref().unwrap_or("?"));
}
```

## Reading the File by Hand

A leaf route reads as three keys per node; absent fields are absent.

```json
{
  "version": 2,
  "routes": [
    { "pattern": "/cart", "plan": { "id": 0, "module": "shell#document", "children": [ { "slot": "content", "node": { "id": 1, "module": "routes/cart/page.tsx#default", "source": "cart", "error": "routes/error.tsx#default" } } ] } }
  ],
  "sources": [ { "id": "cart", "owner": "lowered", "module": "routes/cart/loader.ts", "body": [ ] } ],
  "actions": [ { "id": "cart.addToCart", "owner": "lowered", "module": "routes/cart/actions.ts", "export": "addToCart", "input": "AddToCart", "body": [ ] } ],
  "components": [ { "module": "routes/cart/page.tsx#default", "body": { "render": { "text": "" } } } ]
}
```

## Format Versions

`FORMAT_VERSION` is 2: the `sources` table exists and actions are rows. A format 1 file lists bare action ids, which read as `rust` rows, and has no sources.

```rust
let old = r#"{ "version": 1, "routes": [], "actions": ["cart.checkout"] }"#;
let manifest = snapfire_fsr_plan::Manifest::from_json(old)?;
assert_eq!(manifest.actions[0].owner, snapfire_fsr_plan::RowOwner::Rust);
```

## Error Handling

`PlanError` is what `from_json` and `routes` return. Every variant names where in the file it found the problem.

```rust
use snapfire_fsr_plan::{Manifest, PlanError};

match Manifest::from_json(text).and_then(|m| m.routes()) {
  Ok(routes) => serve(routes),
  Err(PlanError::Version { found }) => eprintln!("plan file version {found}; rebuild with this fsr"),
  Err(PlanError::Module { at, module }) => eprintln!("{at}: `{module}` is not path#export"),
  Err(PlanError::NoBody { id }) => eprintln!("`{id}` says lowered but carries nothing to run"),
  Err(e) => eprintln!("{e}"),
}
```
