# API Reference: snapfire_fsr_core

Vocabulary types for Snapfire FSR: the value model, the payload tree, the render plan, module identity and canonical fingerprinting.

## Contents

1. [Type Aliases](#1-type-aliases)
   * [ValueMap](#valuemap)
   * [Props](#props)
   * [Data](#data)
   * [Params](#params)
2. [The Value Model](#2-the-value-model)
   * [Value](#value)
   * [RefKind](#refkind)
   * [TypedArray](#typedarray)
3. [The Payload Tree](#3-the-payload-tree)
   * [Html](#html)
   * [SlotId](#slotid)
   * [Node](#node)
4. [The Render Plan](#4-the-render-plan)
   * [NodeId](#nodeid)
   * [SlotName](#slotname)
   * [DataSourceId](#datasourceid)
   * [CacheKey](#cachekey)
   * [PlanNode](#plannode)
5. [Module Identity](#5-module-identity)
   * [ModuleId](#moduleid)
6. [Fingerprinting](#6-fingerprinting)
   * [Fingerprint](#fingerprint)
7. [Error Handling](#7-error-handling)
   * [ParseModuleIdError](#parsemoduleiderror)

## 1. Type Aliases

Declared in `lib.rs` and `value.rs`. All four are re-exported from the crate root.

### ValueMap

A string-keyed map of values, insertion-ordered.

* `pub type ValueMap = indexmap::IndexMap<String, Value>`
* Insertion order is preserved and is what serialization emits. Equality and the fingerprint ignore it.

### Props

What a `Node::Client` passes to a component.

* `pub type Props = ValueMap`
* An alias, not a distinct type. A `Props` and a `Data` are interchangeable to the compiler.

### Data

What a data source returns for one plan node.

* `pub type Data = value::ValueMap`

### Params

Matched route parameters.

* `pub type Params = indexmap::IndexMap<String, String>`
* Values are strings, never `Value`. Nothing in the crate parses them.

## 2. The Value Model

The closed roster of types that may cross the server/browser boundary. Adding a variant is a format change.

### Value

One value in the model.

* `Null`
* `Bool(bool)`
* `Int(i128)` - every signed integer, and every unsigned one that fits.
* `UInt(u128)` - only for magnitudes above `i128::MAX`. Constructing it below that bound produces a value that is `!=` its `Int` form while fingerprinting identically to it.
* `F32(f32)`
* `F64(f64)` - a distinct variant from `F32`, with a distinct fingerprint. Nothing widens `F32` to `F64`.
* `Str(String)`
* `Bytes(Vec<u8>)` - never collides with `Str` of the same bytes.
* `TypedArray(TypedArray)`
* `Seq(Vec<Value>)`
* `Map(ValueMap)`
* `Variant { tag: String, payload: Option<Box<Value>> }` - `payload: None` and `payload: Some(Value::Null)` are different values.
* `Ref { kind: RefKind, id: String }`
* `pub fn int(v: impl Into<i128>) -> Value`
* `pub fn uint(v: u128) -> Value` - returns `Int` when the value fits `i128`, `UInt` otherwise.
* `pub fn str(v: impl Into<String>) -> Value`
* `pub fn action_ref(id: impl Into<String>) -> Value` - `Ref` with `RefKind::Action`. There is no matching constructor for `RefKind::Module`.
* `impl From<bool> for Value` - `Bool`.
* `impl From<&str> for Value` - `Str`.
* `impl From<String> for Value` - `Str`.
* `impl From<i64> for Value` - `Int`.
* `impl From<u64> for Value` - `Int`, not `UInt`.
* `impl From<f64> for Value` - `F64`. There is no `From<f32>`.
* Derives `Debug`, `Clone` and `PartialEq`. It implements neither `Eq` nor `Hash`, so it cannot be a `HashMap` key or a `HashSet` member.
* `PartialEq` is derived and inherits IEEE float semantics: two `F64(NAN)` values are unequal while their fingerprints are equal. Compare fingerprints when the question is content identity.

### RefKind

Which kind of thing a `Ref` names. Closed; a new kind is a format change.

* `Action` - a server action.
* `Module` - a client module.
* Derives `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`.
* The same id under two kinds fingerprints differently.

### TypedArray

A numeric series held as one `Vec`, mirroring the JavaScript `TypedArray` element set.

* `I8(Vec<i8>)`, `U8(Vec<u8>)`, `I16(Vec<i16>)`, `U16(Vec<u16>)`, `I32(Vec<i32>)`, `U32(Vec<u32>)`, `I64(Vec<i64>)`, `U64(Vec<u64>)`, `F32(Vec<f32>)`, `F64(Vec<f64>)`
* Derives `Debug`, `Clone` and `PartialEq`.
* The element kind is part of the value's identity: `F32(vec![1.0])` and `F64(vec![1.0])` fingerprint differently, and neither matches a `Seq` of the same numbers.
* Integer elements are never normalized between kinds. `U8` and `I8` holding the same numbers are different values.

## 3. The Payload Tree

What a response renders to. Five variants, no `Element` and no HTML AST.

### Html

Trusted markup.

* `pub struct Html(pub String)`
* Serialized without escaping. Anything untrusted placed in it reaches the browser unescaped.
* Derives `Debug`, `Clone`, `PartialEq` and `Eq`.

### SlotId

Identifies a streaming slot within one response.

* `pub struct SlotId(pub u32)`
* Allocated by the assembler and unique per response. Values minted elsewhere can collide with allocated ones.
* Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `PartialOrd` and `Ord`.

### Node

One node of the payload tree.

* `Text(std::borrow::Cow<'static, str>)` - escaped by the HTML encoding, not here.
* `Raw(Html)` - emitted verbatim.
* `Seq(Vec<Node>)`
* `Client { module: ModuleId, props: Props, children: Vec<Node>, ssr: Option<Box<Node>> }` - `props` holds values only; content passed into the component goes in `children`. `ssr` is the evaluator's rendered output, `None` under the null evaluator.
* `Pending { slot: SlotId, fallback: Box<Node> }` - the fallback is carried inline, so the first response is complete without the resolution row.
* `pub fn text(v: impl Into<Cow<'static, str>>) -> Node`
* `pub fn raw(v: impl Into<String>) -> Node` - wraps the string in `Html`.
* Derives `Debug`, `Clone` and `PartialEq`. Not `Eq` or `Hash`, since props can hold floats.

## 4. The Render Plan

What a request renders, decided before any data is loaded.

### NodeId

Identifies one plan node.

* `pub struct NodeId(pub u32)`
* Must be unique within one plan: loaded data and failures are addressed back to a segment by this number.
* Part of a `PlanNode` fingerprint, so renumbering a structurally identical plan changes its digest.
* Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `PartialOrd` and `Ord`.

### SlotName

Names the slot a child renders into within its parent module.

* `pub struct SlotName(pub String)`
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`.

### DataSourceId

Names the data source a plan node waits on.

* `pub struct DataSourceId(pub String)`
* Resolved by the runtime's registry. Nothing in this crate checks that the name exists.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`.

### CacheKey

The cacheability tag for a subtree, and the tag invalidation matches on.

* `pub struct CacheKey(pub String)`
* Not the stored key. The runtime composes that from this tag plus the matched params, the identity and the subtree's data fingerprint.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`.

### PlanNode

One segment of the render plan.

* `pub id: NodeId`
* `pub module: ModuleId`
* `pub data_source: Option<DataSourceId>`
* `pub deferred: bool` - `true` means this segment streams. Deferral is declared here and never discovered during rendering.
* `pub fallback: Option<ModuleId>` - the loading module, rendered from params alone.
* `pub error: Option<ModuleId>` - the error module, rendered with params plus the failure message when this segment's data source fails. `None` means the built-in error node.
* `pub cache_key: Option<CacheKey>`
* `pub children: Vec<(SlotName, PlanNode)>`
* `pub fn new(id: NodeId, module: ModuleId) -> PlanNode` - every other field starts `None`, `false` or empty.
* Derives `Debug`, `Clone` and `PartialEq`.

## 5. Module Identity

### ModuleId

A component's source path plus the export it is reached through.

* `pub path: String`
* `pub export: String`
* `pub fn new(path: impl Into<String>, export: impl Into<String>) -> ModuleId`
* `impl fmt::Display for ModuleId` - formats as `path#export`.
* `impl FromStr for ModuleId` with `type Err = ParseModuleIdError` - splits on the **last** `#`, so a path containing `#` keeps every earlier one. An empty path or an empty export is an error.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `PartialOrd` and `Ord`.
* The built module's content hash is not part of the id. It lives in the build manifest, so an id stays stable across rebuilds that did not change the module.

## 6. Fingerprinting

### Fingerprint

Canonical content hashing, xxh3-64 over a canonical byte string.

* `fn write_canonical(&self, h: &mut xxhash_rust::xxh3::Xxh3)` - required.
* `fn fingerprint(&self) -> u64` - provided; opens a fresh `Xxh3`, calls `write_canonical` and returns the digest.
* Implemented for `Value`, `ValueMap` (and therefore `Props` and `Data`), `TypedArray`, `Node` and `PlanNode`. Not implemented for `ModuleId`, `Html`, `SlotId`, `NodeId` or `Params`.

Canonical rules, each of which a caller can violate silently by assuming otherwise:

* **Maps hash in sorted key order.** Insertion order never affects a digest, which matches `PartialEq` on `IndexMap` and diverges from what serialization emits.
* **Every NaN collapses.** `f64` NaNs hash as `0x7ff8000000000000` and `f32` NaNs as `0x7fc00000`, as scalars and as typed-array elements alike. Sign and payload bits are lost.
* **`UInt` normalizes on the way into the hash.** A `UInt` whose value fits `i128` produces the byte string of its `Int` form, so a value that skipped `Value::uint` still matches one that did not.
* **Lengths are prefixed.** Every string, byte string, sequence, map and typed array writes its length as an 8-byte little-endian `u64` first, so no re-cutting of the same bytes across a boundary collides.
* **Every variant carries a distinct type tag.** `Str` and `Bytes` of the same bytes never collide; nor do a `TypedArray` and a `Seq` of the same numbers; nor an `Int` and an `F64` of the same magnitude; nor the two `RefKind`s under one id.
* **Optional fields write a presence byte**, so an absent `Variant` payload differs from a `Null` one, and an absent `ssr`, `data_source`, `fallback`, `error` or `cache_key` differs from a present one.
* **`Value::Map` and `ValueMap` do not agree.** `Value::Map(m).fingerprint()` writes the `Map` type tag before the entries; `m.fingerprint()` writes the entries alone. Choose one form per key and stay with it.
* **`PlanNode` and `ValueMap` write no leading type tag**, unlike `Value` and `Node`, whose byte strings always open with one.
* **`NodeId` is inside a plan's digest** and `SlotId` is inside a `Pending` node's digest, so renumbering changes a fingerprint even when nothing else did.
* Multi-byte integers are little-endian throughout. The digest is not portable to a big-endian target.

## 7. Error Handling

### ParseModuleIdError

The crate's only error type. Everything else is total: constructors cannot fail and fingerprinting cannot fail.

* `pub struct ParseModuleIdError` - a unit struct carrying no data.
* Returned by `<ModuleId as FromStr>::from_str` when the string has no `#`, an empty path or an empty export.
* `impl fmt::Display` - `module id must be ...` naming the required `path#export` shape and both non-empty halves.
* `impl std::error::Error` - no `source`.
* Derives `Debug`, `Clone`, `PartialEq` and `Eq`.
* Reached at `snapfire_fsr_core::module_id::ParseModuleIdError`. The crate root re-exports `ModuleId` only.
