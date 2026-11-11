# API Reference: snapfire_fsr_core

The shared types of SnapFire FSR: the value model, the payload tree, the render plan, module identity, fingerprinting, the extension contract, plain JSON, dates and file locks.

## Contents

* [1. Type Aliases and Constants](#1-type-aliases-and-constants)
  * [`Props`](#props)
  * [`Data`](#data)
  * [`Params`](#params)
  * [`Text`](#text)
  * [`Key`](#key)
  * [`Items`](#items)
  * [`ValueHasher`](#valuehasher)
  * [`CARD_MAX_SIDE`](#card_max_side)
* [2. The Value Model](#2-the-value-model)
  * [`Value`](#value)
  * [`ValueStr`](#valuestr)
  * [`ValueSeq`](#valueseq)
  * [`ValueMap`](#valuemap)
  * [`Fields`](#fields)
  * [`Collection`](#collection)
  * [`RefKind`](#refkind)
  * [`TypedArray`](#typedarray)
  * [`value::typed`](#valuetyped)
  * [`Dynamic`](#dynamic)
  * [`reaches` and `collect_cycles`](#reaches-and-collect_cycles)
  * [`on_enough_stack`](#on_enough_stack)
* [3. The Payload Tree](#3-the-payload-tree)
  * [`Html`](#html)
  * [`SlotId`](#slotid)
  * [`Node`](#node)
  * [`Place`](#place)
  * [`is_place_name`](#is_place_name)
* [4. The Render Plan](#4-the-render-plan)
  * [`NodeId`](#nodeid)
  * [`SlotName`](#slotname)
  * [`DataSourceId`](#datasourceid)
  * [`CacheKey`](#cachekey)
  * [`PlanNode`](#plannode)
* [5. Module Identity](#5-module-identity)
  * [`ModuleId`](#moduleid)
* [6. Fingerprinting](#6-fingerprinting)
  * [`Fingerprint`](#fingerprint)
* [7. Extensions](#7-extensions)
  * [`Reach`](#reach)
  * [`Ambient`](#ambient)
  * [`FileRoots`](#fileroots)
  * [`Table`](#table)
  * [`Catalogs`](#catalogs)
  * [`Ty`, `Field` and `Signature`](#ty-field-and-signature)
  * [Argument helpers](#argument-helpers)
* [8. Plain JSON](#8-plain-json)
  * [`stringify`](#stringify)
  * [`to_json`](#to_json)
  * [`to_value`](#to_value)
  * [`number_text`](#number_text)
* [9. Durations and Dates](#9-durations-and-dates)
  * [`parse_duration`](#parse_duration)
  * [`iso_string`](#iso_string)
  * [Calendar arithmetic](#calendar-arithmetic)
* [10. Locks](#10-locks)
  * [`exclusive` and `shared`](#exclusive-and-shared)
  * [`once`](#once)
* [11. Error Handling](#11-error-handling)
  * [`FailureKind`](#failurekind)
  * [`Fail`](#fail)
  * [`ParseModuleIdError`](#parsemoduleiderror)
  * [`DynamicError`](#dynamicerror)
  * [`Unwritable`](#unwritable)
  * [`OnceError`](#onceerror)

## 1. Type Aliases and Constants

### `Props`

What a `Node::Client` passes to a component.

* `pub type Props = ValueMap`

### `Data`

What a data source returns for one plan node.

* `pub type Data = ValueMap`

### `Params`

Matched route parameters.

* `pub type Params = indexmap::IndexMap<String, String>`
* Values are strings and are not parsed by this crate.

### `Text`

What a `ValueStr` holds.

* `pub type Text = smol_str::SmolStr`: up to 23 bytes inline, longer text shared behind one reference count.

### `Key`

A field name of an object.

* `pub type Key = smol_str::SmolStr`

### `Items`

What a `ValueSeq` holds.

* `pub type Items = Vec<Value>`

### `ValueHasher`

* `pub type ValueHasher = foldhash::fast::RandomState`: the hasher behind `Fields`. It is seeded per instance and must not be replaced with a fixed-state hasher.

### `CARD_MAX_SIDE`

* `pub const CARD_MAX_SIDE: u32 = 8192`: the largest side of a card image, in pixels.

## 2. The Value Model

Every type that may cross the server/browser boundary.

### `Value`

One value in the model. `Debug`, `Clone`, `PartialEq`. Not `Eq` or `Hash`.

* `Null`
* `Undefined`: JavaScript's `undefined`, distinct from `Null`. A field holding it is left out where the value is written as JSON.
* `Bool(bool)`
* `Int(i128)`: every signed integer and every unsigned one that fits.
* `UInt(u128)`: for magnitudes above `i128::MAX`.
* `F32(f32)`
* `F64(f64)`: distinct from `F32`. Nothing widens `F32` to `F64`.
* `Str(ValueStr)`
* `Bytes(Vec<u8>)`: never equal to `Str` of the same bytes.
* `TypedArray(TypedArray)`
* `Seq(ValueSeq)`
* `Map(ValueMap)`
* `Variant { tag: String, payload: Option<Box<Value>> }`: `payload: None` and `payload: Some(Value::Null)` are different values.
* `Ref { kind: RefKind, id: String }`

Constructors and conversions:

* `pub fn int(v: impl Into<i128>) -> Value`
* `pub fn uint(v: u128) -> Value`: `Int` when the value fits `i128`, `UInt` otherwise.
* `pub fn str(v: impl Into<ValueStr>) -> Value`
* `pub fn seq(items: impl Into<ValueSeq>) -> Value`
* `pub fn action_ref(id: impl Into<String>) -> Value`: `Ref` with `RefKind::Action`. There is no constructor for `RefKind::Module`.
* `From<bool>` is `Bool`. `From<&str>` and `From<String>` are `Str`. `From<i64>` and `From<u64>` are `Int`. `From<f64>` is `F64`. There is no `From<f32>`.

Methods:

* `pub fn frozen(&self) -> Value`: this value and all it holds, kept for the life of the process. A clone copies a pointer and a drop does nothing. The memory is never released.
* `pub fn frozen_root(&self) -> Option<Value>`: for a frozen object or array, the frozen value it was made part of.
* `pub fn address(&self) -> Option<usize>`: the address of an object or array, its identity while it lives. `None` for any other value.
* `pub fn nested_past(&self, limit: usize) -> bool`: whether objects and arrays nest deeper than `limit`. Does not recurse.

Equality:

* Floats compare by canonical bits. Every NaN equals every other NaN and `0.0` is not equal to `-0.0`.
* `UInt(n)` equals `Int(n)` when `n` fits `i128`.
* Map equality ignores insertion order.
* Equal values have equal fingerprints.

### `ValueStr`

A string of the value model. `Clone`, `Default`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Debug`, `Display`.

* `pub struct ValueStr(Text)`: read as `str` through `Deref`. `Borrow<str>` and `AsRef<str>`.
* `pub fn as_str(&self) -> &str`
* `pub fn into_text(self) -> Text`
* `pub fn frozen(&self) -> ValueStr`: a long string kept for the life of the process.
* `From<&str>`, `From<String>`, `From<&String>`, `From<Cow<str>>`, `From<Key>` and `From<&Key>`. `String: From<ValueStr>`.
* Compares equal with `str`, `&str` and `String`.

### `ValueSeq`

An array, copy on write. Clones share one array. `Clone`, `Default`, `Debug`, `PartialEq`, `PartialEq<Items>`.

* Read as `Items` through `Deref`. `DerefMut` copies the array first when it is shared, so the write reaches this holder only.
* `pub fn through(&self) -> Option<&mut Items>`: the shared array itself, so a write reaches every holder. `None` for a frozen array. No reference into the array may be alive across the write.
* `pub fn kind(&self) -> Collection`
* `pub fn with_kind(self, kind: Collection) -> Self`
* `pub fn holders(&self) -> usize`: how many holders share the array. `usize::MAX` for a frozen one.
* `pub fn is_frozen(&self) -> bool`
* `pub fn same(&self, other: &Self) -> bool`: whether both hold one array.
* `pub fn addr(&self) -> usize`: the array's address.
* `pub fn into_items(self) -> Items`
* `From<Items>`, `FromIterator<Value>`, `IntoIterator` by value and by reference, `Extend<Value>`.
* Dropping the last holder releases nested arrays and objects without recursing as deep as they nest.

### `ValueMap`

An object, insertion-ordered and copy on write. Clones share one object. `Clone`, `Default`, `Debug`, `PartialEq`, `PartialEq<Fields>`.

* Read as `Fields` through `Deref`. `DerefMut` copies first when the object is shared.
* `pub fn through(&self) -> Option<&mut Fields>`, `kind`, `with_kind`, `holders`, `is_frozen`, `same` and `addr`, as on `ValueSeq`.
* `pub fn into_fields(self) -> Fields`
* `pub fn into_fields_with_room(self, room: usize) -> Fields`: `into_fields` with room for `room` more fields. Taken in place when nothing else holds it, copied once otherwise.
* `From<Fields>`, `FromIterator<(impl Into<Key>, Value)>`, `IntoIterator` by value and by reference, `Extend`.
* There is no `ValueMap::new`. Use `ValueMap::default()`.
* Serialization emits insertion order. Equality and the fingerprint ignore it.

### `Fields`

An object's fields in insertion order. `Clone`, `Default`, `Debug`, `PartialEq`. Past 8 fields, a hash table of positions finds a key.

* `pub fn new() -> Self`
* `pub fn with_capacity(capacity: usize) -> Self`
* `pub fn with_capacity_and_hasher<H>(capacity: usize, _hasher: H) -> Self`: the hasher argument is ignored.
* `pub fn copy_with_room(&self, room: usize) -> Fields`
* `pub fn reserve(&mut self, more: usize)`
* `pub fn len(&self) -> usize`
* `pub fn is_empty(&self) -> bool`
* `pub fn get<Q: AsRef<str> + ?Sized>(&self, key: &Q) -> Option<&Value>`. `get_mut`, `get_key_value` and `contains_key` take the same key bound.
* `pub fn get_index(&self, at: usize) -> Option<(&Key, &Value)>`
* `pub fn insert(&mut self, key: impl Into<Key>, value: Value) -> Option<Value>`: replaces in place or appends.
* `pub fn shift_remove<Q>(&mut self, key: &Q) -> Option<Value>`: keeps the order of the rest. `remove` is `shift_remove`.
* `pub fn swap_remove<Q>(&mut self, key: &Q) -> Option<Value>`: moves the last field into the gap.
* `pub fn retain(&mut self, keep: impl FnMut(&Key, &mut Value) -> bool)`
* `pub fn clear(&mut self)`
* `pub fn sort_keys(&mut self)`
* `iter`, `iter_mut`, `keys`, `values`, `values_mut`, `into_keys` and `into_values`: double-ended and exact-size.
* `pub fn entry(&mut self, key: impl Into<Key>) -> Entry<'_>`, with `or_insert(value)`, `or_insert_with(make)` and `or_default()`.
* `Index<&str>`, `FromIterator<(impl Into<Key>, Value)>`, `Extend` and `IntoIterator`.

### `Collection`

Which JavaScript collection an object or array stands for. `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`, `Eq`.

* `Plain`, the default.
* `Map`: a JavaScript `Map`, kept as an object of its keys.
* `Set`: a JavaScript `Set`, kept as an array of its items.
* Equality, the fingerprint and the wire ignore it.

### `RefKind`

What a `Ref` names. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `Action`: a server action.
* `Module`: a client module.
* The same id under two kinds is two different values.

### `TypedArray`

A numeric series held as one `Vec`, matching JavaScript's typed arrays. `Debug`, `Clone`, `PartialEq`.

* `I8(Vec<i8>)`, `U8(Vec<u8>)`, `I16(Vec<i16>)`, `U16(Vec<u16>)`, `I32(Vec<i32>)`, `U32(Vec<u32>)`, `I64(Vec<i64>)`, `U64(Vec<u64>)`, `F32(Vec<f32>)`, `F64(Vec<f64>)`
* The element kind is part of the value. `F32(vec![1.0])` and `F64(vec![1.0])` differ. Neither equals a `Seq` of the same numbers.
* Integer kinds are never normalised into each other. `U8` and `I8` with the same numbers differ.
* Float elements compare by canonical bits, as `Value` floats do.

### `value::typed`

The tags of a `Value::Variant` that stands for a JavaScript value with no arm of its own.

* `pub const DATE: &str = "$date"`: payload is milliseconds since the epoch.
* `pub const MAP: &str = "$map"`: payload is an array of `[key, value]` pairs.
* `pub const SET: &str = "$set"`: payload is an array.
* `pub const BIGINT: &str = "$bigint"`: payload is an integer.
* `pub const URL: &str = "$url"`: payload is the href.
* `pub const TEMPORAL: &str = "$temporal"`: payload is `[type, text]`, a `Temporal` type's name and its canonical string.
* `pub const SYMBOL: &str = "$symbol"`: a symbol from `Symbol.for`, payload its key. JSON leaves it out of an object and writes `null` in an array.
* `pub const RAW_JSON: &str = "$rawJSON"`: `JSON.rawJSON(text)`, payload the text JSON writes as it is.
* `pub const PROTOTYPE: &str = "$prototype"`: a built-in constructor's `prototype`, payload the constructor's name.
* `pub const DEFERRED: &str = "$deferred"`: the field marking the object that stands for a promise `Promise.withResolvers()` made.
* `pub const REGEX: &str = "$regex"`: the field holding a regular expression's pattern in the object that stands for it. JSON writes that object as `{}`.
* `pub const ALL: [&str; 6]`
* `pub fn temporal(value: &Value) -> Option<(&str, &str)>`: a Temporal value's type name and string.
* `pub fn temporal_value(kind: &str, text: &str) -> Value`: the Temporal value of type `kind` whose string is `text`.

### `Dynamic`

Data whose shape a contract does not declare. `Debug`, `Clone`, `PartialEq`.

* `pub struct Dynamic(pub Value)`. `From<Value>` and `Value: From<Dynamic>`.
* `pub fn from_serde<T: Serialize + ?Sized>(value: &T) -> Result<Dynamic, DynamicError>`: writes no JSON. A unit enum variant becomes its name and a variant with data becomes a one-field map.
* `pub fn to_serde<T: DeserializeOwned>(&self) -> Result<T, DynamicError>`: a whole float reads as an integer. `Undefined` reads as absent.
* `pub fn from_json(json: &serde_json::Value) -> Dynamic`: `json::to_value`.
* `pub fn to_json(&self) -> Result<serde_json::Value, DynamicError>`: `json::to_json`.

### `reaches` and `collect_cycles`

* `pub fn reaches(value: &Value, target: usize, budget: usize) -> Option<bool>`: whether `value` reaches the object or array at address `target`. It looks at no more than `budget` objects and arrays or 16 levels down. It returns `None` when it gives up. A frozen value is not searched.
* `pub fn collect_cycles(candidates: Vec<Value>)`: frees the objects and arrays reachable from `candidates` that only hold each other. One held from outside survives with all it reaches. Call it only where nothing borrows a value reached from `candidates`.

### `on_enough_stack`

* `pub fn on_enough_stack<T>(f: impl FnOnce() -> T) -> T`: runs `f`, growing the thread's stack first when less than the margin is left.

## 3. The Payload Tree

What a response renders to.

### `Html`

Trusted markup. `Debug`, `Clone`, `PartialEq`, `Eq`.

* `pub struct Html(pub String)`
* Serialized without escaping. Anything untrusted placed in it reaches the browser unescaped.

### `SlotId`

A streaming slot within one response. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`.

* `pub struct SlotId(pub u32)`
* The assembler allocates them, unique per response. An id made elsewhere can collide with an allocated one.

### `Node`

One node of the payload tree. Eight variants. `Debug`, `Clone`, `PartialEq`. Not `Eq` or `Hash`.

* `Text(Cow<'static, str>)`: escaped by the HTML encoding.
* `Raw(Html)`: written verbatim.
* `Seq(Vec<Node>)`
* `Client { module: ModuleId, props: Props, children: Vec<Node>, rendered: bool }`: an island.
  * `children` is what the server rendered when `rendered` is true, else the regions it offers the browser, such as the content passed into the component.
  * `rendered` is false under the null evaluator.
* `Pending { slot: SlotId, fallback: Box<Node> }`: a hole a later stream row fills. The fallback is carried inline.
* `Slot(SlotName)`: where a child segment goes inside an island's own markup. The assembler replaces it with the child's node. A serializer writes nothing for one.
* `Placed { target: Place, nodes: Vec<Node> }`: content written at `target` instead of where the node sits. A navigation that replaces the segment the node sits in removes it.
* `Outlet(String)`: where content placed at `Place::Named` of this name is written, inside the element whose id is the name.
* `pub fn text(v: impl Into<Cow<'static, str>>) -> Node`
* `pub fn raw(v: impl Into<String>) -> Node`: wraps the string in `Html`.

### `Place`

Where a `Node::Placed` writes its content. `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`. `#[non_exhaustive]`.

* `Body`: the end of the document's body.
* `Named(String)`: inside the element a `Node::Outlet` of this name sits in.
* `pub fn spelling(&self) -> Cow<'_, str>`: `body` for the body, `#` followed by the name for a named place.
* `pub fn parse(target: &str) -> Option<Place>`: `None` for anything but `body` or `#` and a name `is_place_name` accepts.

### `is_place_name`

* `pub fn is_place_name(name: &str) -> bool`: one or more ASCII letters, digits, `-` and `_`.

## 4. The Render Plan

What a request renders, decided before any data is loaded.

### `NodeId`

One plan node. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`.

* `pub struct NodeId(pub u32)`
* Must be unique within one plan. Loaded data and failures are addressed by it.

### `SlotName`

The slot a child renders into within its parent module. `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`.

* `pub struct SlotName(pub String)`

### `DataSourceId`

The data source a plan node waits on. `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`.

* `pub struct DataSourceId(pub String)`
* This crate does not check that the name exists.

### `CacheKey`

The cache tag of a subtree, which invalidation matches on. `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`.

* `pub struct CacheKey(pub String)`
* It is not the stored key. The runtime composes that from this tag and the request.

### `PlanNode`

One segment of the render plan. `Debug`, `Clone`, `PartialEq`. `#[non_exhaustive]`: build one with `new` and assign the fields.

* `pub id: NodeId`
* `pub module: ModuleId`
* `pub data_source: Option<DataSourceId>`
* `pub deferred: bool`: this segment streams.
* `pub fallback: Option<ModuleId>`: the loading module, rendered from params alone.
* `pub error: Option<ModuleId>`: rendered with params and the failure message when the data source fails. `None` is the built-in error node.
* `pub error_kinds: Vec<(String, ModuleId)>`: error modules by failure kind, keyed by `FailureKind::as_str`. A named kind renders its module instead of `error`.
* `pub cache_key: Option<CacheKey>`
* `pub children: Vec<(SlotName, PlanNode)>`
* `pub keep: Vec<SlotName>`: slots this node leaves unfilled that the browser keeps as they are.
* `pub fn new(id: NodeId, module: ModuleId) -> PlanNode`: every other field `None`, `false` or empty.

## 5. Module Identity

### `ModuleId`

A component's source path and the export it is reached through. `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`. `#[non_exhaustive]`: build one with `new` or by parsing.

* `pub path: String`
* `pub export: String`
* `pub fn new(path: impl Into<String>, export: impl Into<String>) -> ModuleId`
* `Display`: `path#export`.
* `FromStr` with `Err = ParseModuleIdError`: splits on the last `#`. An empty path or export is an error.
* The module's content hash is not part of the id.

## 6. Fingerprinting

### `Fingerprint`

Canonical content hashing: xxh3-64 over a canonical byte string.

* `fn write_canonical(&self, h: &mut xxhash_rust::xxh3::Xxh3)`: required.
* `fn fingerprint(&self) -> u64`: provided. Hashes `write_canonical` into a fresh `Xxh3`.
* Implemented for `Value`, `ValueMap`, `TypedArray`, `Node` and `PlanNode`. Not for `ModuleId`, `Html`, `SlotId`, `NodeId` or `Params`.

Canonical rules:

* Maps hash in sorted key order. Insertion order never affects a digest.
* Every NaN collapses: `f64` NaNs hash as `0x7ff8000000000000` and `f32` NaNs as `0x7fc00000`, as scalars and as typed-array elements.
* A `UInt` that fits `i128` hashes as its `Int` form.
* Every string, byte string, sequence, map and typed array is prefixed with its length as an 8-byte little-endian `u64`.
* Every variant has its own type tag. `Str` and `Bytes` of the same bytes differ, a `TypedArray` and a `Seq` of the same numbers differ, `Int` and `F64` of the same magnitude differ and the two `RefKind`s under one id differ.
* Optional fields write a presence byte. An absent `Variant` payload differs from a `Null` one.
* `Value::Map(m).fingerprint()` writes the `Map` type tag before the entries. `m.fingerprint()` writes the entries alone, so the two differ.
* `PlanNode` and `ValueMap` write no leading type tag. `Value` and `Node` always do.
* `NodeId` is inside a plan's digest and `SlotId` is inside a `Pending` node's digest.
* Integers are little-endian. The digest is not portable to a big-endian target.

## 7. Extensions

Module `snapfire_fsr_core::ext`: the contract the Rust half of a native extension is written against. `snapfire_fsr_host` and `snapfire_fsr` re-export it as `ext`, with its types at their roots.

### `Reach`

Where a call may run. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `Render`: on the server and in the browser. It must give the same answer in both.
* `Body`: on the server only, in a loader, an action, a handler or middleware. Refused on a component's render path.
* `Terminal`: in a terminal program, with no browser half. A render may read the clock.
* `Later`: in a program that keeps running after a handler returns, so a timer, an `await` or a listener can run later.
* `Process`: in a program that starts processes, reads its environment and owns the screen.
* A terminal page may use all five. A web page may use `Render` and `Body` only. The build refuses the others there.
* `pub fn as_str(&self) -> &'static str`: `render`, `body`, `terminal`, `later` or `process`.
* `pub fn needs(&self) -> Option<&'static str>`: why a web page cannot use the reach, worded to follow "`x`, which ...". `None` for `Render` and `Body`.

### `Ambient`

What a call runs under. `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`. `#[non_exhaustive]`: build one with `Default` and the setters.

* `pub locale: String`: the request's locale as the application spells it, such as `fr_FR`. Empty when nothing set one.
* `pub now: i128`: milliseconds since the Unix epoch.
* `pub catalogs: Option<Arc<Catalogs>>`: the catalogs `i18n.t` reads.
* `pub files: Option<Arc<FileRoots>>`: what `file` may reach in a web host. `None` in a terminal program, where a path is relative to the working directory.
* `pub fn locale(self, locale: String) -> Self`
* `pub fn now(self, now: i128) -> Self`
* `pub fn catalogs(self, catalogs: Option<Arc<Catalogs>>) -> Self`
* `pub fn files(self, files: Option<Arc<FileRoots>>) -> Self`
* `pub fn bcp47(&self) -> String`: the locale as BCP 47, `fr-FR` for `fr_FR`. `en` when the locale is empty.

### `FileRoots`

The directories a web host's bodies may read and write. `Debug`, `Clone`, `PartialEq`, `Eq`.

* `pub fn new(base: impl Into<PathBuf>, roots: Vec<PathBuf>) -> FileRoots`: `base` is the application's directory. Each root is relative to it or absolute.
* `pub fn roots(&self) -> &[PathBuf]`
* `pub fn place(&self, what: &str, path: &str) -> Result<PathBuf, Fail>`: `path` resolved against `base`, with `.` and `..` resolved.
  * It must lie under a root once links are followed through its longest existing ancestor.
  * Otherwise it is a `Forbidden` failure naming `[files] roots` and the roots by name. With no roots, the failure says the host reads and writes no files.

### `Table`

* `pub type Table = BTreeMap<String, String>`: one locale's messages by dotted key.

### `Catalogs`

Message catalogs, one table per locale, each merged over the default locale's. `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`.

* `pub fn from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Catalogs`: `tables` by locale tag. `default` names the table that fills in for the others. Each merged table's JSON is written once, here.
* `pub fn is_empty(&self) -> bool`
* `pub fn default_tag(&self) -> &str`
* `pub fn rows(&self) -> Vec<(String, usize)>`: every locale and how many keys its own table held, sorted by tag.
* `pub fn table(&self, tag: &str) -> Option<&Arc<Table>>`: the merged table for `tag`. The default's when `tag` has none.
* `pub fn json(&self, tag: &str) -> Option<Arc<str>>`: `table` as JSON.
* `pub fn lookup(&self, tag: &str, key: &str) -> Option<&str>`

### `Ty`, `Field` and `Signature`

The TypeScript declaration of a function FSR provides with no TypeScript of its own. All three are `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `pub enum Ty { String, Number, Boolean, Unknown, Void, Undefined, Named(&'static str), Object(&'static [Field]), Union(&'static [Ty]), Function(&'static [Field], &'static Ty), Promise(&'static Ty) }`
* `pub fn Ty::typescript(&self) -> String`: the type as TypeScript writes it.
* `pub struct Field { pub name: &'static str, pub ty: Ty, pub optional: bool }`
* `pub const fn Field::required(name: &'static str, ty: Ty) -> Field`
* `pub const fn Field::optional(name: &'static str, ty: Ty) -> Field`
* `pub fn Field::typescript(&self) -> String`: `name: T` or `name?: T`.
* `pub struct Signature { pub doc: &'static str, pub generics: &'static [&'static str], pub params: &'static [Field], pub returns: Ty }`
* `pub fn Signature::declare(&self, name: &str) -> String`: `/** doc */` and `export declare function name<T>(a: T): R;`.

### Argument helpers

Read an extension's arguments. `i` counts from 0.

* `pub fn number(what: &str, args: &[Value], i: usize) -> Result<f64, Fail>`: `Int`, `UInt`, `F32` or `F64`, as `f64`.
* `pub fn text<'a>(what: &str, args: &'a [Value], i: usize) -> Result<&'a str, Fail>`
* `pub fn text_opt<'a>(what: &str, args: &'a [Value], i: usize) -> Result<Option<&'a str>, Fail>`: absent, `Null` or `Undefined` is `None`.
* `pub fn option<'a>(what: &str, args: &'a [Value], i: usize, field: &str) -> Result<Option<&'a Value>, Fail>`: a field of an optional options object. `None` when the object or the field is absent, `Null` or `Undefined`.
* `pub fn type_error(what: &str, wanted: &str, got: &Value) -> Fail`: `Internal` with ``{what} wants {wanted}, got {kind}``.
* `pub fn kind_name(value: &Value) -> &'static str`: `null`, `undefined`, `bool`, `int`, `uint`, `float`, `string`, `bytes`, `typed array`, `array`, `object`, `variant` or `ref`.
* A wrong kind fails through `type_error`.
* A missing required argument fails as `Internal` with ``{what} takes a number as argument {n}`` or ``{what} takes a string as argument {n}``, `n` counting from 1.

## 8. Plain JSON

Module `snapfire_fsr_core::json`: the JSON that code outside FSR reads and writes. Nothing is tagged, so a key named `$` is an ordinary key. The encoding between the host and its own client is `snapfire_fsr_payload`.

### `stringify`

* `pub fn stringify(value: &Value, indent: &str, integers: Integers) -> Result<String, Unwritable>`: what `JSON.stringify(value, null, indent)` writes.
  * `Undefined` and `Ref` are left out of an object and written `null` in an array or at the top.
  * A non-finite float is `null`.
  * A `$date` variant is its ISO string. An invalid one is `null`. `$map` and `$set` are `{}`. `$url` is its href. `$temporal` is its string, as `toJSON` writes it.
  * Bytes and typed arrays are objects keyed by index.
  * Any other variant is `{ "tag": .., "payload": .. }`.
* `pub enum Integers { Refuse, Digits }`: `Refuse` refuses an integer as `JSON.stringify` refuses a `BigInt`. `Digits` writes its digits. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

### `to_json`

* `pub fn to_json(value: &Value) -> Result<serde_json::Value, Unwritable>`: `stringify` with no indent and `Integers::Digits`, parsed back.

### `to_value`

* `pub fn to_value(json: &serde_json::Value) -> Value`: an integral number is `Int`, any other number `F64` and an object a map in document order, whatever its keys.

### `number_text`

* `pub fn number_text(n: f64) -> String`: `Number.prototype.toString()`, the shortest digits that read back as the double. Fixed between 1e-7 and 1e21, exponential outside.
* `pub fn write_number(out: &mut String, n: f64)`: `number_text` appended to `out`.

## 9. Durations and Dates

### `parse_duration`

* `pub fn parse_duration(raw: &str) -> Option<Duration>`: `<n>`, `<n>s`, `<n>m`, `<n>h`, `<n>d` or `<n>ms`, `n` a non-negative integer. Surrounding whitespace is ignored. `None` for another unit or an overflow.

### `iso_string`

Module `snapfire_fsr_core::time`: proleptic Gregorian arithmetic over milliseconds since 1970-01-01 UTC.

* `pub const MS_PER_DAY: f64 = 86_400_000.0`
* `pub const MAX_DATE_MS: f64 = 8.64e15`: the largest instant a `Date` holds on either side of 1970.
* `pub fn iso_string(ms: f64) -> Option<String>`: `Date.prototype.toISOString`. A year outside 0 to 9999 is written as a sign and six digits. `None` for an instant no `Date` holds.

### Calendar arithmetic

* `pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64`: days since 1970-01-01.
* `pub fn civil_from_days(z: i64) -> (i64, u32, u32)`: the year, month and day.
* `pub fn civil_from_ms(ms: f64) -> (i64, u32, u32)`: the UTC date of an instant.
* `pub fn clock_from_ms(ms: f64) -> (u32, u32, u32, u32)`: the UTC hours, minutes, seconds and milliseconds of an instant.

## 10. Locks

Module `snapfire_fsr_core::lock`: a lock on a file, the same across threads and processes. It is released when dropped or when the process ends.

### `exclusive` and `shared`

* `pub fn exclusive(path: impl AsRef<Path>) -> io::Result<Held>`: waits until no one else holds `path`, then holds it alone. The file is created when absent and left in place.
* `pub fn shared(path: impl AsRef<Path>) -> io::Result<Held>`: waits until no one holds it alone, then holds it beside other shared holders.
* `pub struct Held`: the lock. `Debug`. `pub fn path(&self) -> &Path`.

### `once`

* `pub fn once<E>(dir: impl AsRef<Path>, key: &str, setup: impl FnOnce() -> Result<(), E>) -> Result<bool, OnceError<E>>`: runs `setup` once for `key` across every thread and process sharing `dir`. `true` when this call ran it.
  * Uses `<dir>/<key>.lock` and records `<dir>/<key>.done` on success.
  * Other callers wait while it runs. A different `key` runs it again.

## 11. Error Handling

Everything outside `ext`, `json`, `lock`, `Dynamic` and `ModuleId` parsing is infallible.

### `FailureKind`

The failure kinds a UI renders: one per 4xx and 5xx status the IANA HTTP status code registry assigns, 418 aside, 39 in all. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`. `#[non_exhaustive]`.

* `Invalid` is 400, `Unavailable` 503 and `Timeout` 504.
* Every other kind is named after its status in snake case, such as `Forbidden` `forbidden` 403, `TooManyRequests` `too_many_requests` 429 and `BadGateway` `bad_gateway` 502. The [runtime reference](../runtime/API_REFERENCE.md) lists every kind under Error Handling.
* `pub const ALL: &'static [FailureKind]`: every kind, in status order.
* `pub fn as_str(&self) -> &'static str`
* `pub fn http_status(&self) -> u16`
* `pub fn from_name(name: &str) -> Option<FailureKind>`: the kind `as_str` spells.
* `pub fn from_status(status: u16) -> FailureKind`: the status's own kind, else `Invalid` for any other 4xx and `Internal` for anything else.
* `pub fn is_client_error(&self) -> bool`: a 4xx.
* `pub fn is_server_error(&self) -> bool`: a 5xx.
* At `snapfire_fsr_core::ext::FailureKind`. `snapfire_fsr_runtime` re-exports it at its root.

### `Fail`

What an extension call and the argument helpers fail with. `Debug`, `Clone`, `PartialEq`, `std::error::Error`. `#[non_exhaustive]`.

* `pub kind: FailureKind`
* `pub message: String`
* `pub thrown: Option<Box<Value>>`: what a `throw` threw, which a `catch` binds.
* `pub call: Option<String>`: the service call that raised it, `service.method`.
* `pub fn new(kind: FailureKind, message: impl Into<String>) -> Fail`
* `pub fn internal(message: impl Into<String>) -> Fail`: kind `Internal`.
* `pub fn with_thrown(self, value: Value) -> Fail`
* `pub fn with_call(self, call: impl Into<String>) -> Fail`
* `Display`: `{kind}: {message}`, the kind as `as_str` spells it.

### `ParseModuleIdError`

Returned by `<ModuleId as FromStr>::from_str` for a string with no `#`, an empty path or an empty export. `Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`.

* `pub struct ParseModuleIdError`
* `Display`: ``module id must be `path#export` with a non-empty path and export``.
* At `snapfire_fsr_core::module_id::ParseModuleIdError`. The crate root re-exports `ModuleId` only.

### `DynamicError`

Returned by `Dynamic::from_serde`, `to_serde` and `to_json`. `Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`.

* `pub struct DynamicError(pub String)`
* `Display`: the message.

### `Unwritable`

Returned by `json::stringify` and `json::to_json`. `Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`.

* `pub struct Unwritable(pub String)`: a value plain JSON has no form for under the rule given, such as an integer under `Integers::Refuse`.
* `Display`: the message.

### `OnceError`

Returned by `lock::once`. `Debug`. `Display` is the inner error's. It is `std::error::Error` when `E` is `Debug + Display`.

* `pub enum OnceError<E> { Io(io::Error), Setup(E) }`: `Io` when the lock or the marker failed. `Setup` when `setup` did.
* A failed setup records nothing, so the next caller runs it again.
