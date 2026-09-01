# Usage Guide: snapfire_fsr_core

This guide covers building values in the FSR value model, assembling a payload `Node` tree, describing a request with a `PlanNode` tree, naming modules and taking canonical fingerprints of any of it.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Building Values](#building-values)
  * [Choosing an Integer Variant](#choosing-an-integer-variant)
  * [Converting from Rust Primitives](#converting-from-rust-primitives)
  * [Building a Map](#building-a-map)
* [Carrying Bytes and Numeric Arrays](#carrying-bytes-and-numeric-arrays)
* [Building a Tagged Variant](#building-a-tagged-variant)
* [Referencing an Action or a Module](#referencing-an-action-or-a-module)
* [Naming a Module](#naming-a-module)
  * [Parsing a Module Id](#parsing-a-module-id)
* [Building a Payload Tree](#building-a-payload-tree)
  * [Passing Props into an Island](#passing-props-into-an-island)
  * [Leaving a Pending Slot](#leaving-a-pending-slot)
* [Building a Render Plan](#building-a-render-plan)
  * [Declaring Deferral](#declaring-deferral)
  * [Naming a Cache Key](#naming-a-cache-key)
* [Fingerprinting a Value](#fingerprinting-a-value)
  * [Fingerprinting a Node or a Plan](#fingerprinting-a-node-or-a-plan)
  * [Composing a Cache Key](#composing-a-cache-key)
* [Comparing Values](#comparing-values)
* [Why the Model Is Shaped This Way](#why-the-model-is-shaped-this-way)
* [Error Handling](#error-handling)

## Core Concepts

* **Value model** - The closed roster of types that may cross the server/browser boundary. `Value` is the Rust half of it.
* **Sovereign** - The model decides what can exist and encodings are ranked projections of it. An encoding is lossless over the model or a declared degradation, never a silent one, so the model never shrinks to suit a format.
* **`ValueMap`** - An `IndexMap<String, Value>`, insertion-ordered for serialization. Aliased as `Props` for what goes into an island, and as `Data` for what a loader returns.
* **`Params`** - An `IndexMap<String, String>`, the matched route parameters. Strings, not values.
* **Normalization** - An unsigned integer that fits `i128` belongs in `Value::Int`. `Value::uint` enforces it, and `UInt` exists only for magnitudes above `i128::MAX`.
* **Typed array** - `Value::TypedArray`, a numeric series held as one Rust `Vec` rather than one `Value` per element, mirroring the JavaScript `TypedArray` element set.
* **Variant** - `Value::Variant`, a tag plus an optional payload. A Rust enum with data arrives in TypeScript as a discriminated union because the model carries the shape, not a map-shaped convention.
* **Ref** - `Value::Ref`, a closed pair of kind and id. `RefKind::Action` names a server action, `RefKind::Module` names a client module.
* **`Node`** - The payload tree, what a response renders to. Exactly five variants: `Text`, `Raw`, `Seq`, `Client` and `Pending`.
* **`Html`** - A newtype over `String` holding trusted markup. It is serialized without escaping, so nothing untrusted may be put inside it.
* **Island** - A `Node::Client`, a hydratable component named by `ModuleId` and carrying its props. `ssr` holds the evaluator's output when one ran.
* **Slot** - A hole in the payload. `Node::Pending` carries a `SlotId` and an inline fallback, and a later stream row fills it.
* **`PlanNode`** - The render plan, a tree naming a module, an optional data source and named child slots, decided before any data is loaded.
* **Deferral** - `PlanNode::deferred`, declared in the plan rather than discovered mid-render, which is what makes streaming plannable.
* **`ModuleId`** - Source path plus export name, `components/ServerChart.tsx#default`. The content hash lives in the build manifest, never here.
* **Fingerprint** - A canonical xxh3-64 content hash. Equal values hash equal regardless of construction history, map insertion order or NaN bit pattern.

## Quick Start

Build a page as a payload tree, with one island carrying a typed array, then take its fingerprint.

```rust
use snapfire_fsr_core::{Fingerprint, ModuleId, Node, TypedArray, Value, ValueMap};

fn chart_island(series: Vec<f64>) -> Node {
  let mut props = ValueMap::new();
  props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(series)));
  props.insert("title".to_owned(), Value::str("servers"));

  Node::Client {
    module: ModuleId::new("components/ServerChart.tsx", "default"),
    props,
    children: Vec::new(),
    ssr: None,
  }
}

fn main() {
  let page = Node::Seq(vec![
    Node::raw("<html><head></head><body><main>"),
    Node::Seq(vec![
      Node::raw("<section><h1>Servers</h1><table></table>"),
      chart_island(vec![1.0, 2.5, 3.0]),
      Node::raw("</section>"),
    ]),
    Node::raw("</main></body></html>"),
  ]);

  println!("{:016x}", page.fingerprint());
}
```

Describe the same page as a plan, before any loader has run.

```rust
use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

fn main() {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("routes/dash/servers/page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));

  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("routes/dash/layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), page));

  let (slot, child) = &layout.children[0];
  assert_eq!(slot.0, "content");
  assert_eq!(child.module.to_string(), "routes/dash/servers/page.tera#default");
}
```

## Building Values

Four constructors cover the cases where a variant is easy to pick wrong. Everything else is built by naming the variant.

```rust
use snapfire_fsr_core::Value;

let nothing = Value::Null;
let flag = Value::Bool(true);
let count = Value::int(42);
let name = Value::str("servers");
let ratio = Value::F64(0.75);
let single = Value::F32(0.75);
let raw = Value::Bytes(b"ab".to_vec());
let list = Value::Seq(vec![Value::int(1), Value::int(2)]);
```

`F32` and `F64` are separate variants and fingerprint differently, so a value stored as `F32` stays `F32` all the way to the browser rather than widening on the way out.

### Choosing an Integer Variant

Use `Value::int` for anything signed and `Value::uint` for anything unsigned. `uint` normalizes: a value that fits `i128` comes back as `Int`, and `UInt` is reached only above `i128::MAX`.

```rust
use snapfire_fsr_core::Value;

assert_eq!(Value::uint(42), Value::Int(42));
assert_eq!(Value::uint(u128::MAX), Value::UInt(u128::MAX));
```

Naming `Value::UInt` directly bypasses that check and produces a value that compares unequal to its `Int` form while fingerprinting identically. Prefer the constructor.

```rust
use snapfire_fsr_core::{Fingerprint, Value};

assert_ne!(Value::UInt(42), Value::Int(42));
assert_eq!(Value::UInt(42).fingerprint(), Value::Int(42).fingerprint());
```

### Converting from Rust Primitives

`From` is implemented for the six primitives that map onto exactly one variant, so a `Value` can be produced by `.into()` where the target type is known.

```rust
use snapfire_fsr_core::Value;

let a: Value = true.into();
let b: Value = "servers".into();
let c: Value = String::from("servers").into();
let d: Value = 7i64.into();
let e: Value = 7u64.into();
let f: Value = 0.5f64.into();
```

`u64` converts to `Int`, not `UInt`, for the same normalization reason. There is no `From<f32>`: name `Value::F32` so the choice of width is visible at the call site.

### Building a Map

A map is an `IndexMap<String, Value>`. Insertion order is preserved for serialization and ignored by equality and the fingerprint.

```rust
use snapfire_fsr_core::{Value, ValueMap};

let mut row = ValueMap::new();
row.insert("id".to_owned(), Value::int(7));
row.insert("hostname".to_owned(), Value::str("edge-01"));
row.insert("healthy".to_owned(), Value::Bool(true));

let record = Value::Map(row);
```

`Props` and `Data` are the same type under different names. Use `Props` for what an island receives and `Data` for what a loader returns.

## Carrying Bytes and Numeric Arrays

`Value::Bytes` is an opaque byte string; `Value::TypedArray` is a numeric series whose element type is part of the value. Both avoid one `Value` per element, and they never collide with each other or with a `Seq` of scalars.

```rust
use snapfire_fsr_core::{Fingerprint, TypedArray, Value};

let bytes = Value::Bytes(vec![1, 2, 3]);
let series = Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0]));
let ids = Value::TypedArray(TypedArray::U32(vec![7, 9, 11]));

let as_seq = Value::Seq(vec![Value::F64(1.0), Value::F64(2.5), Value::F64(3.0)]);
assert_ne!(series.fingerprint(), as_seq.fingerprint());
```

The element kind is part of the identity, so the same numbers held at two widths are two different values.

```rust
use snapfire_fsr_core::{Fingerprint, TypedArray, Value};

let narrow = Value::TypedArray(TypedArray::F32(vec![1.0]));
let wide = Value::TypedArray(TypedArray::F64(vec![1.0]));
assert_ne!(narrow.fingerprint(), wide.fingerprint());
```

The ten kinds are `I8`, `U8`, `I16`, `U16`, `I32`, `U32`, `I64`, `U64`, `F32` and `F64`.

## Building a Tagged Variant

A variant is a tag plus an optional payload, which is how a Rust enum reaches TypeScript as a discriminated union.

```rust
use snapfire_fsr_core::{Value, ValueMap};

let down = Value::Variant { tag: "Down".to_owned(), payload: None };

let mut detail = ValueMap::new();
detail.insert("since".to_owned(), Value::int(1_725_000_000));
let degraded = Value::Variant {
  tag: "Degraded".to_owned(),
  payload: Some(Box::new(Value::Map(detail))),
};
```

An absent payload and a `Null` payload are different values. Use `None` for a unit variant rather than wrapping `Value::Null`.

```rust
use snapfire_fsr_core::{Fingerprint, Value};

let unit = Value::Variant { tag: "Down".to_owned(), payload: None };
let with_null = Value::Variant { tag: "Down".to_owned(), payload: Some(Box::new(Value::Null)) };
assert_ne!(unit.fingerprint(), with_null.fingerprint());
```

## Referencing an Action or a Module

A reference is a kind and an id. `RefKind` is closed: `Action` for a server action the browser can invoke, `Module` for a client module. The same id under the two kinds is two different values.

```rust
use snapfire_fsr_core::{RefKind, Value};

let save = Value::action_ref("save_server");

let module = Value::Ref { kind: RefKind::Module, id: "components/Row.tsx#default".to_owned() };
```

`Value::action_ref` is the only constructor; build a module reference by naming `Value::Ref`.

## Naming a Module

A `ModuleId` is a source path plus an export name, because one file can export several components. It renders as `path#export`.

```rust
use snapfire_fsr_core::ModuleId;

let id = ModuleId::new("components/ServerChart.tsx", "default");
assert_eq!(id.to_string(), "components/ServerChart.tsx#default");
```

The content hash of the built module is not part of the id. It lives in the build manifest, so a cache entry keyed on module identity survives a rebuild that did not change the module.

### Parsing a Module Id

`FromStr` splits on the last `#` and rejects an empty half on either side.

```rust
use snapfire_fsr_core::ModuleId;

let id: ModuleId = "components/ServerChart.tsx#default".parse().unwrap();
assert_eq!(id, ModuleId::new("components/ServerChart.tsx", "default"));

assert!("components/ServerChart.tsx".parse::<ModuleId>().is_err());
assert!("#default".parse::<ModuleId>().is_err());
assert!("components/x.tsx#".parse::<ModuleId>().is_err());
```

## Building a Payload Tree

`Node` has five variants and no `Element`. Composition happens at plan-node boundaries: the runtime stitches there, a framework composes freely inside one plan node and nothing ever reaches inside a node's output, so there is no HTML AST here to maintain.

```rust
use snapfire_fsr_core::Node;

let text = Node::text("Servers");
let markup = Node::raw("<section><h1>Servers</h1></section>");
let both = Node::Seq(vec![markup, text]);
```

`Node::Text` is escaped when it reaches the HTML encoding. `Node::Raw` wraps `Html`, which is emitted verbatim, so only markup an evaluator produced belongs in it.

### Passing Props into an Island

`Node::Client` names the module, carries the props, carries server content as `children` and holds the evaluator's rendered output in `ssr`. Under the null evaluator `ssr` is `None` and the browser renders the component itself.

```rust
use snapfire_fsr_core::{ModuleId, Node, TypedArray, Value, ValueMap};

let mut props = ValueMap::new();
props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));

let island = Node::Client {
  module: ModuleId::new("components/ServerChart.tsx", "default"),
  props,
  children: vec![Node::raw("<p>No data yet</p>")],
  ssr: None,
};
```

Props hold values, never nodes. Content crossing into a component goes through `children`, which keeps the value model a closed flat roster instead of mutually recursive with `Node`.

### Leaving a Pending Slot

A `Pending` node marks a hole a later stream row fills. It carries its fallback inline, so the first response is complete on its own.

```rust
use snapfire_fsr_core::{Node, SlotId};

let pending = Node::Pending {
  slot: SlotId(1),
  fallback: Box::new(Node::raw("<div class=skl></div>")),
};
```

`SlotId` is allocated by the assembler and is unique per response. Do not mint one outside the assembler.

## Building a Render Plan

`PlanNode::new` takes the two fields that are never absent, then the optional ones are set on the value. Children are named slots, so the tree records where each subtree renders into its parent.

```rust
use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

let mut page = PlanNode::new(NodeId(1), ModuleId::new("routes/dash/servers/page.tera", "default"));
page.data_source = Some(DataSourceId("servers_loader".into()));

let mut layout = PlanNode::new(NodeId(0), ModuleId::new("routes/dash/layout.tera", "default"));
layout.data_source = Some(DataSourceId("layout_loader".into()));
layout.children.push((SlotName("content".into()), page));
```

`NodeId` is how loaded data is addressed back to the segment that asked for it, so ids must be unique within one plan.

### Declaring Deferral

Setting `deferred` says this segment streams: its loader does not block the first response, and `fallback` names the loading module rendered from params alone. Deferral is declared here, never discovered mid-render.

```rust
use snapfire_fsr_core::{DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

let mut chart = PlanNode::new(NodeId(2), ModuleId::new("routes/dash/servers/chart_section.tera", "default"));
chart.data_source = Some(DataSourceId("chart_loader".into()));
chart.deferred = true;
chart.fallback = Some(ModuleId::new("routes/dash/servers/chart_loading.tera", "default"));
chart.error = Some(ModuleId::new("routes/dash/servers/chart_error.tera", "default"));

let mut page = PlanNode::new(NodeId(1), ModuleId::new("routes/dash/servers/page.tera", "default"));
page.children.push((SlotName("chart".into()), chart));
```

`error` is the module rendered with params plus the failure message when this segment's data source fails. Leave it `None` to get the built-in error node.

### Naming a Cache Key

`cache_key` marks a subtree as cacheable and gives it the tag invalidation works on. It is a name, not the whole key: the runtime composes the stored key from this plus the matched params, the identity and the subtree's data fingerprint.

```rust
use snapfire_fsr_core::{CacheKey, ModuleId, NodeId, PlanNode};

let mut page = PlanNode::new(NodeId(1), ModuleId::new("routes/dash/servers/page.tera", "default"));
page.cache_key = Some(CacheKey("servers_page".into()));
```

## Fingerprinting a Value

`Fingerprint` writes a canonical byte string and digests it with xxh3-64. Implement nothing; call `fingerprint()` on a `Value`, a `ValueMap`, a `TypedArray`, a `Node` or a `PlanNode`.

```rust
use snapfire_fsr_core::{Fingerprint, Value, ValueMap};

let mut ab = ValueMap::new();
ab.insert("a".to_owned(), Value::int(1));
ab.insert("b".to_owned(), Value::int(2));

let mut ba = ValueMap::new();
ba.insert("b".to_owned(), Value::int(2));
ba.insert("a".to_owned(), Value::int(1));

assert_eq!(Value::Map(ab).fingerprint(), Value::Map(ba).fingerprint());
```

Three rules do the work, and each one is a place a naive hash would report a false difference. Map entries hash in sorted key order, so insertion order never shows. Every NaN collapses to one bit pattern, in a scalar and inside a typed array alike. An unsigned value that fits `i128` hashes as its signed form, so a value that skipped `Value::uint` still matches.

```rust
use snapfire_fsr_core::{Fingerprint, TypedArray, Value};

let quiet = Value::F64(f64::NAN);
let payload = Value::F64(f64::from_bits(0x7ff8_0000_0000_0001));
assert_eq!(quiet.fingerprint(), payload.fingerprint());

let a = Value::TypedArray(TypedArray::F64(vec![1.0, f64::NAN]));
let b = Value::TypedArray(TypedArray::F64(vec![1.0, f64::from_bits(0x7ff8_0000_0000_0001)]));
assert_eq!(a.fingerprint(), b.fingerprint());
```

Every string, byte string and sequence is length-prefixed, so no rearrangement of the same bytes across a boundary produces the same digest.

```rust
use snapfire_fsr_core::{Fingerprint, Value};

let a = Value::Seq(vec![Value::str("ab"), Value::str("c")]);
let b = Value::Seq(vec![Value::str("a"), Value::str("bc")]);
assert_ne!(a.fingerprint(), b.fingerprint());
```

### Fingerprinting a Node or a Plan

The same trait covers both trees, so a payload and the plan that produced it are each hashable in one call. A `Node` fingerprint tracks island props, and a `PlanNode` fingerprint tracks deferral.

```rust
use snapfire_fsr_core::{Fingerprint, ModuleId, NodeId, PlanNode};

let plan = PlanNode::new(NodeId(0), ModuleId::new("routes/dash/layout.tera", "default"));
let same = PlanNode::new(NodeId(0), ModuleId::new("routes/dash/layout.tera", "default"));
assert_eq!(plan.fingerprint(), same.fingerprint());

let renumbered = PlanNode::new(NodeId(1), ModuleId::new("routes/dash/layout.tera", "default"));
assert_ne!(plan.fingerprint(), renumbered.fingerprint());
```

`NodeId` is part of a plan's fingerprint, so two structurally identical plans whose nodes are numbered differently hash differently.

### Composing a Cache Key

A stored cache key is the plan's `cache_key` plus whatever else changes the answer. The data fingerprint is what makes the entry self-invalidating when a loader returns something new.

```rust
use snapfire_fsr_core::{Data, Fingerprint, PlanNode};

fn cache_key(node: &PlanNode, data: &Data) -> Option<String> {
  let tag = node.cache_key.as_ref()?;
  Some(format!("{}|{:016x}", tag.0, data.fingerprint()))
}
```

Hash a map through `Data` (that is, `ValueMap`) and through `Value::Map` and the two digests differ: `Value::Map` writes its variant tag first. Pick one form for a given key and stay with it.

## Comparing Values

`PartialEq` and the fingerprint agree on maps and disagree on floats, because `PartialEq` is derived and inherits IEEE semantics. Two NaNs are unequal to each other while hashing identically.

```rust
use snapfire_fsr_core::{Fingerprint, Value};

let a = Value::F64(f64::NAN);
let b = Value::F64(f64::NAN);
assert_ne!(a, b);
assert_eq!(a.fingerprint(), b.fingerprint());
```

Compare fingerprints when the question is "same content", which is what caching and change detection are asking. Use `PartialEq` only where IEEE equality is what you want. `Value` implements neither `Eq` nor `Hash`, so it cannot be a `HashMap` key; key on its fingerprint instead.

## Why the Model Is Shaped This Way

The model is sovereign and encodings are ranked projections of it. Admitting a type is a decision about what can exist, and an encoding then either carries it losslessly or degrades in a way it declares. That is why `Value` holds `i128`, `u128`, `f32` beside `f64`, raw bytes and typed arrays: JSON pays the price of tagging what it cannot spell, rather than the model shrinking to what JSON spells natively.

Typed arrays are a variant rather than a `Seq` of numbers because the element type is information the browser needs, and because a ten-thousand-point series should be one `Vec`, not ten thousand `Value`s. Variants are a model type rather than a map-shaped convention so that codegen, every encoding and the fingerprint agree on one representation. `undefined` does not exist; an absent key is the only absence.

`Node` stops at five variants because composition happens at plan-node boundaries. The runtime stitches there and a framework composes freely inside one plan node, so nothing manipulates the inside of a node's output and the HTML AST is work nobody owes. Islands are absent from the plan for the mirror reason: whether one renders can depend on loader data, so the plan carries may-use and the payload carries does-use.

The fingerprint is canonical because it is a cache key. Insertion order, NaN bit patterns and an unnormalized `UInt` are all differences in construction history rather than in content, and a hash that reported them would evict entries that were still correct. Length prefixes are the other half: without them, adjacent strings could be re-cut into the same byte stream and two different values would collide.

## Error Handling

The crate has one fallible operation, parsing a `ModuleId`, and one error type. Everything else is total: constructors cannot fail and fingerprinting cannot fail.

`ParseModuleIdError` is a unit struct carrying no data. It implements `Debug`, `Display`, `Clone`, `PartialEq`, `Eq` and `std::error::Error`, and it is returned when the string has no `#`, an empty path or an empty export.

```rust
use std::str::FromStr;

use snapfire_fsr_core::module_id::ParseModuleIdError;
use snapfire_fsr_core::ModuleId;

fn resolve(raw: &str) -> Option<ModuleId> {
  match ModuleId::from_str(raw) {
    Ok(id) => Some(id),
    Err(ParseModuleIdError) => None,
  }
}
```

The error type is reached through its module path; the crate root re-exports `ModuleId` alone. Its `Display` names the required shape and both non-empty halves, so propagating it with `?` into a `Box<dyn Error>` loses nothing.

```text
module id must be `path#export` with a non-empty path and export
```
