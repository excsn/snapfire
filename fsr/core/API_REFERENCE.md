# API Reference: snapfire_fsr_core

Vocabulary types for SnapFire FSR: the value model, the payload tree, the render plan, module identity, canonical fingerprinting and the extension contract.

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
   * [Place](#place)
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
7. [Extensions](#7-extensions)
   * [Reach](#reach)
   * [Ambient](#ambient)
   * [FileRoots](#fileroots)
   * [Table](#table)
   * [Catalogs](#catalogs)
   * [Argument Helpers](#argument-helpers)
8. [Plain JSON](#8-plain-json)
   * [stringify](#stringify)
   * [to_json](#to_json)
   * [to_value](#to_value)
   * [number_text](#number_text)
9. [Dates](#9-dates)
   * [iso_string](#iso_string)
   * [Calendar Arithmetic](#calendar-arithmetic)
10. [Locks](#10-locks)
   * [exclusive and shared](#exclusive-and-shared)
   * [once](#once)
11. [Error Handling](#11-error-handling)
   * [Unwritable](#unwritable)
   * [OnceError](#onceerror)
   * [FailureKind](#failurekind)
   * [Fail](#fail)
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
* `Int(i128)` - every signed integer and every unsigned one that fits.
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
* The element kind is part of the value's identity: `F32(vec![1.0])` and `F64(vec![1.0])` fingerprint differently and neither matches a `Seq` of the same numbers.
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
* `Client { module: ModuleId, props: Props, children: Vec<Node>, rendered: bool }` - `props` holds values only. `children` is the island's markup: what the evaluator rendered when `rendered` is true, else the regions it offers the browser, such as the content passed into the component. `rendered` is false under the null evaluator.
* `Pending { slot: SlotId, fallback: Box<Node> }` - the fallback is carried inline, so the first response is complete without the resolution row.
* `Slot(SlotName)` - where a child segment goes inside an island's own markup, a layout's `children`. The assembler replaces it with the child's node; a serializer never sees one and writes nothing for it if it does.
* `Placed { target: Place, nodes: Vec<Node> }` - content written at `target` rather than where the node sits, such as a modal at the end of the body. Where it sits names the segment that owns it, so a navigation that replaces the segment removes it.
* `Outlet(String)` - where the content placed at `Place::Named` of this name is written: inside the element whose id is the name, which the markup around the outlet opens and closes.
* `pub fn text(v: impl Into<Cow<'static, str>>) -> Node`
* `pub fn raw(v: impl Into<String>) -> Node` - wraps the string in `Html`.
* Derives `Debug`, `Clone` and `PartialEq`. Not `Eq` or `Hash`, since props can hold floats.

### Place

Where a `Node::Placed` writes its content. `#[non_exhaustive]`.

* `Body` - the end of the document's body.
* `Named(String)` - the element a `Node::Outlet` of this name sits in, whose id is the name.
* `pub fn spelling(&self) -> Cow<'_, str>` - the payload's spelling: `body` for the body and `#` followed by the name for a named place.
* `pub fn parse(target: &str) -> Option<Place>` - `None` for anything but `body` or `#` and a name `is_place_name` accepts.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`.

### is_place_name

* `pub fn is_place_name(name: &str) -> bool` - one or more ASCII letters, digits, `-` and `_`, so a name is written into an attribute and a selector as it stands.

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

The cacheability tag for a subtree. Invalidation matches on this tag.

* `pub struct CacheKey(pub String)`
* Not the stored key. The runtime composes that from this tag plus the matched params, the identity and the subtree's data fingerprint.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`.

### PlanNode

One segment of the render plan. `#[non_exhaustive]`: build one with `new` and assign the fields.

* `pub id: NodeId`
* `pub module: ModuleId`
* `pub data_source: Option<DataSourceId>`
* `pub deferred: bool` - `true` means this segment streams. Deferral is declared here and never discovered during rendering.
* `pub fallback: Option<ModuleId>` - the loading module, rendered from params alone.
* `pub error: Option<ModuleId>` - the error module, rendered with params plus the failure message when this segment's data source fails. `None` means the built-in error node.
* `pub error_kinds: Vec<(String, ModuleId)>` - error modules for one failure kind each, keyed by `FailureKind::as_str`. A failure whose kind is named here renders that module instead of `error`.
* `pub cache_key: Option<CacheKey>`
* `pub children: Vec<(SlotName, PlanNode)>`
* `pub keep: Vec<SlotName>` - slots this node leaves unfilled that the browser keeps as they stand when the payload arrives.
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
* **Optional fields write a presence byte**, so an absent `Variant` payload differs from a `Null` one; an absent `data_source`, `fallback`, `error` or `cache_key` differs from a present one.
* **`Value::Map` and `ValueMap` do not agree.** `Value::Map(m).fingerprint()` writes the `Map` type tag before the entries; `m.fingerprint()` writes the entries alone. Choose one form per key and stay with it.
* **`PlanNode` and `ValueMap` write no leading type tag**, unlike `Value` and `Node`, whose byte strings always open with one.
* **`NodeId` is inside a plan's digest** and `SlotId` is inside a `Pending` node's digest, so renumbering changes a fingerprint even when nothing else did.
* Multi-byte integers are little-endian throughout. The digest is not portable to a big-endian target.

## 7. Extensions

The contract the Rust half of a native extension is written against, in `snapfire_fsr_core::ext`. The registry that holds extensions and the standard library are `snapfire_fsr_ir`'s. `snapfire_fsr_host` and `snapfire_fsr` re-export the module as `ext` and its types at their roots.

### Reach

Where a call may run.

* `Render` - on the server and in the browser, so it must give the same answer in both.
* `Body` - on the server only, in a loader, an action, a handler or middleware. Refused on a component's render path.
* `Terminal` - in a program that draws where it is viewed, with no browser half, so a render may read the clock.
* `Later` - in a program that keeps running after a handler returns, so a timer, an `await` or a listener can run the rest of the handler later.
* `Process` - in a program that starts processes, reads its environment and owns the screen.
* A terminal page may use all five. A web page may use `Render` and `Body` only; the build refuses the other three.
* `pub fn as_str(&self) -> &'static str` - `render`, `body`, `terminal`, `later` or `process`.
* `pub fn needs(&self) -> Option<&'static str>` - why a web page cannot use the reach, finishing "`x`, which ...". `None` for `Render` and `Body`.
* Derives `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`.

### Ambient

What a call runs under. `#[non_exhaustive]`: build one with `Default` and the setters.

* `pub locale: String` - the request's locale in the application's spelling, `fr_FR`; empty when nothing set one.
* `pub now: i128` - milliseconds since the Unix epoch, the clock `ctx.now` reads.
* `pub catalogs: Option<Arc<Catalogs>>` - the catalogs the host loaded, which `i18n.t` reads; `None` when the application has none.
* `pub files: Option<Arc<FileRoots>>` - where `file` resolves in a web host and what it reaches; `None` in a terminal program, where a path is the working directory's.
* `pub fn bcp47(&self) -> String` - the locale as BCP 47, `fr-FR` for `fr_FR`; `en` when the locale is empty. The browser half converts the same way.
* `pub fn locale(self, locale: String) -> Self`, `pub fn now(self, now: i128) -> Self`, `pub fn catalogs(self, catalogs: Option<Arc<Catalogs>>) -> Self`, `pub fn files(self, files: Option<Arc<FileRoots>>) -> Self` - setters for the field of the same name.
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Default`.

### FileRoots

The directories a web host's bodies read and write.

* `pub fn new(base: impl Into<PathBuf>, roots: Vec<PathBuf>) -> FileRoots` - `base` the application's directory, `roots` relative to it or absolute.
* `pub fn roots(&self) -> &[PathBuf]`
* `pub fn place(&self, what: &str, path: &str) -> Result<PathBuf, Fail>` - `path` against `base`, `.` and `..` resolved, when it lies under a root once links are followed through its longest existing ancestor. A `Forbidden` failure otherwise naming `[files] roots` and the roots by name, never where the host keeps them; with no roots, one saying the host reads and writes no files.
* Derives `Debug`, `Clone`, `PartialEq` and `Eq`.

### Table

* `pub type Table = BTreeMap<String, String>` - one locale's messages by dotted key.

### Catalogs

Message catalogs, one table per locale, each held merged over the default locale's so a key a locale lacks reads as the default's.

* `pub fn from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Catalogs` - `tables` by locale tag as the application spells it; `default` names the table that fills in for every other. Each merged table's JSON is written here, once.
* `pub fn is_empty(&self) -> bool`
* `pub fn default_tag(&self) -> &str`
* `pub fn rows(&self) -> Vec<(String, usize)>` - every locale and how many keys its own table held, sorted by tag.
* `pub fn table(&self, tag: &str) -> Option<&Arc<Table>>` - the merged table for `tag`; the default's when `tag` has none.
* `pub fn json(&self, tag: &str) -> Option<Arc<str>>` - `table` as JSON, the text a document or a payload carries.
* `pub fn lookup(&self, tag: &str, key: &str) -> Option<&str>`
* Derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Default`.

### Argument Helpers

Read an extension's arguments. A wrong kind fails as `Internal` with `what wants <wanted>, got <kind>`; a missing required argument fails as `Internal` with `what takes <a number|a string> as argument N`, counting from 1.

* `pub fn number(what: &str, args: &[Value], i: usize) -> Result<f64, Fail>` - `Int`, `UInt`, `F32` or `F64`, as `f64`.
* `pub fn text<'a>(what: &str, args: &'a [Value], i: usize) -> Result<&'a str, Fail>`
* `pub fn text_opt<'a>(what: &str, args: &'a [Value], i: usize) -> Result<Option<&'a str>, Fail>` - absent or `Null` is `None`.
* `pub fn option<'a>(what: &str, args: &'a [Value], i: usize, field: &str) -> Result<Option<&'a Value>, Fail>` - a field of an optional options object; `None` when the object or the field is absent or `Null`.
* `pub fn type_error(what: &str, wanted: &str, got: &Value) -> Fail` - the wrong-kind failure itself.
* `pub fn kind_name(value: &Value) -> &'static str` - `null`, `bool`, `int`, `uint`, `float`, `string`, `bytes`, `typed array`, `array`, `object`, `variant` or `ref`.

## 8. Plain JSON

`snapfire_fsr_core::json`: the JSON anything outside FSR reads and writes. Nothing is tagged, so a key named `$` is a key. The payload encoding between the host and its own client is `snapfire_fsr_payload`.

### stringify

* `pub fn stringify(value: &Value, indent: &str, integers: Integers) -> Result<String, Unwritable>`: `JSON.stringify(value, null, indent)`. `Undefined` and a `Ref` are left out of an object and written `null` in an array or alone. A non-finite double is `null`. A `$date` variant is its ISO string or `null` when invalid, a `$map` or `$set` is `{}`, a `$url` is its href, bytes and a typed array are objects keyed by index and any other variant is `{ "tag": .., "payload": .. }`.
* `pub enum Integers { Refuse, Digits }`: an integer, JavaScript's `BigInt`, refused as `JSON.stringify` refuses it or written as its digits. The IR's `JSON.stringify` refuses; route handlers, `Response.json`, service bodies, traces and sign-in steps write digits.

### to_json

* `pub fn to_json(value: &Value) -> Result<serde_json::Value, Unwritable>`: `stringify` with no indent and `Integers::Digits`, parsed back for an API that takes `serde_json::Value`.

### to_value

* `pub fn to_value(json: &serde_json::Value) -> Value`: an integral number an `Int`, any other number an `F64`, an object a map whatever its keys, in document order.

### number_text

* `pub fn number_text(n: f64) -> String`: `Number.prototype.toString()`, the shortest digits that read back as the double, fixed between 1e-7 and 1e21 and exponential outside.

## 9. Dates

`snapfire_fsr_core::time`: proleptic Gregorian arithmetic over milliseconds since 1970-01-01 UTC. `MS_PER_DAY` and `MAX_DATE_MS`, the largest instant a `Date` holds either side of 1970.

### iso_string

* `pub fn iso_string(ms: f64) -> Option<String>`: `Date.prototype.toISOString`, a year outside 0 to 9999 as a sign and six digits; `None` for an instant no `Date` holds.

### Calendar Arithmetic

* `pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64` and `pub fn civil_from_days(z: i64) -> (i64, u32, u32)`: a date and its day count since 1970-01-01.
* `pub fn civil_from_ms(ms: f64) -> (i64, u32, u32)` and `pub fn clock_from_ms(ms: f64) -> (u32, u32, u32, u32)`: the UTC date and the hours, minutes, seconds and milliseconds of an instant.

## 10. Locks

`snapfire_fsr_core::lock`: a lock on a file, held the same across threads and processes, released when the holder drops it or the process ends.

### exclusive and shared

* `pub fn exclusive(path: impl AsRef<Path>) -> io::Result<Held>`: waits until no one else holds `path` and holds it alone. The file is made when absent and left in place.
* `pub fn shared(path: impl AsRef<Path>) -> io::Result<Held>`: waits until no one holds it alone and holds it beside other shared holders.
* `pub struct Held`: the lock, with `path()`; dropping it releases the lock.

### once

* `pub fn once<E>(dir: impl AsRef<Path>, key: &str, setup: impl FnOnce() -> Result<(), E>) -> Result<bool, OnceError<E>>`: runs `setup` once for `key` across every thread and process sharing `dir`, under `<dir>/<key>.lock`, recording `<dir>/<key>.done` when it succeeds. `true` when this call ran it. `key` names the inputs, so different inputs run it again.

## 11. Error Handling

### Unwritable

* `pub struct Unwritable(pub String)`: a value plain JSON has no form for under the rule given, a `BigInt` under `Integers::Refuse`.

### OnceError

* `pub enum OnceError<E> { Io(io::Error), Setup(E) }`: the lock or the marker failed (`Io`) or the setup did (`Setup`). A failed setup records nothing, so the next caller runs it again.

### FailureKind

The failure shapes a UI has to render: one kind per 4xx and 5xx status the IANA HTTP status code registry assigns, 418 aside, 39 in all. `Invalid` is 400, `Unavailable` 503 and `Timeout` 504; every other kind is the status's own name in snake case, `Forbidden` `forbidden` 403, `TooManyRequests` `too_many_requests` 429, `BadGateway` `bad_gateway` 502. The table of every kind is in the runtime reference, section 13. `#[non_exhaustive]`.

* `pub const ALL: &'static [FailureKind]` - every kind, in status order.
* `pub fn as_str(&self) -> &'static str` and `pub fn http_status(&self) -> u16` - its name and its status.
* `pub fn from_name(name: &str) -> Option<FailureKind>` - the kind `as_str` spells.
* `pub fn from_status(status: u16) -> FailureKind` - the kind for a status, else `Invalid` for a 4xx no kind names and `Internal` for anything else.
* `pub fn is_client_error(&self) -> bool` and `pub fn is_server_error(&self) -> bool` - a 4xx and a 5xx.
* Derives `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`.
* Reached at `snapfire_fsr_core::ext::FailureKind`. `snapfire_fsr_runtime` re-exports it at its root.

### Fail

What an extension call and the argument helpers fail with.

* `pub kind: FailureKind`
* `pub message: String`
* `pub thrown: Option<Box<Value>>` - what a `throw` threw, which a `catch` binds.
* `pub call: Option<String>` - the service call that raised it, `service.method`.
* `pub fn new(kind: FailureKind, message: impl Into<String>) -> Fail`
* `pub fn internal(message: impl Into<String>) -> Fail` - kind `Internal`.
* `pub fn with_thrown(self, value: Value) -> Fail` and `pub fn with_call(self, call: impl Into<String>) -> Fail`.
* `impl fmt::Display` - `kind: message`, the kind spelled as `as_str` spells it.
* `impl std::error::Error` - no `source`.
* Derives `Debug`, `Clone` and `PartialEq`.

### ParseModuleIdError

Returned by module id parsing. Everything outside `ext` is total: constructors cannot fail and fingerprinting cannot fail.

* `pub struct ParseModuleIdError` - a unit struct carrying no data.
* Returned by `<ModuleId as FromStr>::from_str` when the string has no `#`, an empty path or an empty export.
* `impl fmt::Display` - `module id must be ...` naming the required `path#export` shape and both non-empty halves.
* `impl std::error::Error` - no `source`.
* Derives `Debug`, `Clone`, `PartialEq` and `Eq`.
* Reached at `snapfire_fsr_core::module_id::ParseModuleIdError`. The crate root re-exports `ModuleId` only.
