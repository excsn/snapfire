# API Reference: snapfire_fsr_ir

The lowered form of a loader or action body and the interpreter that runs it over the value model.

## Contents

* [1. The Tree](#1-the-tree)
  * [Body](#body)
  * [Stmt](#stmt)
  * [Expr](#expr)
  * [Tmpl](#tmpl)
  * [Component](#component)
  * [Entry](#entry)
  * [Lit](#lit)
  * [ArithOp](#arithop)
  * [CompareOp](#compareop)
  * [LogicOp](#logicop)
* [2. The JSON Form](#2-the-json-form)
  * [from_json](#from_json)
  * [to_json](#to_json)
* [3. The Interpreter](#3-the-interpreter)
  * [Interpreter](#interpreter)
  * [Outcome](#outcome)
  * [Clock](#clock)
  * [Extensions](#extensions)
  * [Reach](#reach)
  * [Ambient](#ambient)
  * [The Standard Library](#the-standard-library)
  * [Catalogs](#catalogs)
* [4. Evaluation Rules](#4-evaluation-rules)
  * [Reads](#reads)
  * [Truthiness](#truthiness)
  * [Operators](#operators)
  * [Conversions](#conversions)
  * [Builtins](#builtins)
  * [Calls](#calls)
  * [Session writes](#session-writes)
  * [Guards](#guards)
  * [Parallel lets](#parallel-lets)
* [5. Runtime Adapters](#5-runtime-adapters)
  * [IrSource](#irsource)
  * [IrAction](#iraction)
* [6. Error Handling](#6-error-handling)
  * [Fail](#fail)
  * [ParseError](#parseerror)

## 1. The Tree

### Body

The statements of one loader or action, in order.

* `pub type Body = Vec<Stmt>`

### Stmt

One statement. Derives `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`; serialised externally tagged in `snake_case`.

* `Let { name: String, expr: Expr }` binds a name for the rest of the enclosing block.
* `If { cond: Expr, then: Body, else: Body }`; `else` is omitted from JSON when empty.
* `ForOf { name: String, over: Expr, body: Body }`; `over` must evaluate to `Value::Seq`.
* `Return(Expr)` ends the body, from any depth.
* `Guard { cond: Expr, kind: String, message: String }` fails the body with `kind` when `cond` is truthy; `kind` is a `FailureKind` name.
* `SessionSet { key: String, path: Vec<Expr>, value: Expr }`; `path` is omitted from JSON when empty.
* `SessionDelete { key: String, path: Vec<Expr> }`; `path` is omitted from JSON when empty.
* `Expr(Expr)` evaluates for effect and discards the value.

### Expr

One expression. Derives `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`; serialised externally tagged in `snake_case`.

* `Param(String)`, `Query(String)`, `Session(String)`, `Identity(Vec<String>)`, `Input`, `Now`, `Var(String)`.
* `Lit(Lit)`.
* `Object(Vec<Entry>)` takes `Field`, `Computed` and `Spread` entries; `Array(Vec<Entry>)` takes `Item` and `Spread` entries. A wrong entry kind is `Internal`.
* `Field(Box<Expr>, String)`, `Index(Box<Expr>, Box<Expr>)`.
* `Arith(ArithOp, Box<Expr>, Box<Expr>)`, `Compare(CompareOp, Box<Expr>, Box<Expr>)`, `Logic(LogicOp, Box<Expr>, Box<Expr>)`, `Not(Box<Expr>)`, `Coalesce(Box<Expr>, Box<Expr>)`, `Ternary(Box<Expr>, Box<Expr>, Box<Expr>)`, `Template(Vec<Expr>)`.
* `Call { service: String, method: String, args: Vec<(String, Expr)> }`; `args` defaults to empty in JSON.
* `Lambda { params: Vec<String>, body: Box<Expr> }`; evaluating a lambda as a value is `Internal`.
* `Map(over, f)`, `Filter(over, f)`, `Reduce(over, init, f)`, `Find(over, f)`, `Some(over, f)`, `Every(over, f)`, all `Box<Expr>`; `f` must be a `Lambda`.
* `Entries(Box<Expr>)`, `Keys(Box<Expr>)`, `Values(Box<Expr>)`, `Length(Box<Expr>)`.
* `Str(Box<Expr>)`, `Num(Box<Expr>)`, `BigInt(Box<Expr>)`.
* `Ext { module: String, name: String, args: Vec<Expr> }`: an extension call, `intl.number(n)`, answered by the interpreter's `Extensions` under `module.name` with the arguments evaluated and the request's locale and clock as `Ambient`. A name the registry lacks is `Internal`, ``extension `x.y` is not registered``. `has_call` is false for a standard member and true for any other, so an application's pair may sit on the async path. `Expr::ext(module, name, args)` builds one.
* `Hoist { id: u32, expr: Box<Expr> }`: a render-path expression whose inputs are props only. Evaluates as `expr`; under a render the value is also recorded in the environment's `Hoists` under `id`, so the browser reads it instead of computing it. `id` is unique within the component's module. In a body it is `expr` and nothing more.
* `Expr::lit_str(s: impl Into<String>) -> Expr`, `Expr::lit_int(n: impl Into<i128>) -> Expr`, `Expr::var(name: impl Into<String>) -> Expr`.
* `Expr::field(self, name: impl Into<String>) -> Expr`, `Expr::index(self, key: Expr) -> Expr`.
* `Expr::call(service, method, args: Vec<(&str, Expr)>) -> Expr`.
* `Expr::lambda(params: &[&str], body: Expr) -> Expr`.
* `Expr::object(entries: Vec<(&str, Expr)>) -> Expr` builds `Field` entries only.
* `Expr::free_vars(&self, out: &mut Vec<String>)` appends every `Var` name read and not bound by an enclosing lambda, without duplicates.
* `Expr::visit(&self, f: &mut dyn FnMut(&Expr))` calls `f` on the expression and every expression beneath it, in tree order.
* `Expr::has_call(&self) -> bool` is true when any `Call` appears in the tree.
* `Expr::reads_request(&self) -> bool` is true when a `Param`, `Query`, `Session`, `Identity`, `Input` or `Now` appears in the tree.
* `body_reads_request(body: &Body) -> bool` is true when any statement reads the request or writes the session.
* `body_params_read(body: &Body) -> Vec<String>` is every route parameter the body reads, by name, without duplicates.

### Tmpl

A lowered component's tree: `Text`, `Expr`, `Element { tag, attrs, children }`, `Fragment`, `If`, `For`, `Let`, `Component { module, props, children }`, `Slot` and `Island { module, props, children, when, mode }`, a component placed as its own island with `when` its hydration timing, `"load"`, `"visible"` or `"idle"`, when the use named one, and `mode` `Some("server")` for an island whose events round-trip to the server, absent for the browser mode every island is in otherwise.

### Component

* `pub struct Component { pub body: Body, pub render: Tmpl, pub state: Vec<String>, pub handlers: Vec<Handler> }`; `Component::new(body, render)` has no state and no handlers.
* `state` names the body `let`s the browser can change, the `useState` and `useStore` bindings in order. `handlers` are the component's event handlers as bodies, for an island in server mode, each `Handler { event, body }` running with `$props`, `$state` and `$event` bound after the body's `let`s and returning an object whose keys are the state names it sets. An element binds one through an attribute `$on:<event>` holding the handler's index; an element whose handler did not lower carries `$unlowered` with the line and the reason; an element's React `key` is kept as `$key`. `render::HANDLER_ATTR`, `render::UNLOWERED_ATTR` and `render::KEY_ATTR` name them.

### Entry

One member of an object or array literal.

* `Field(String, Expr)`, an object member.
* `Computed(Expr, Expr)`, an object member whose key is computed; the key must evaluate to `Str`, anything else is `Internal`.
* `Item(Expr)`, an array member.
* `Spread(Expr)`; into an object the value must be `Value::Map` or `Value::Null`, into an array `Value::Seq` or `Value::Null`.

### Lit

* `Null`, `Bool(bool)`, `Int(i128)`, `Float(f64)`, `Str(String)`.
* `Int` serialises as a JSON number; values outside `i64` and `u64` fail to serialise without serde_json's `arbitrary_precision` feature.

### ArithOp

* `Add`, `Sub`, `Mul`, `Div`, `Rem`.

### CompareOp

* `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`.

### LogicOp

* `And`, `Or`.

## 2. The JSON Form

### from_json

* `pub fn ast::from_json(text: &str) -> Result<Body, ParseError>`

### to_json

* `pub fn ast::to_json(body: &Body) -> String`, pretty printed.

## 3. The Interpreter

### Interpreter

Runs a body. `Clone`; the default carries the system clock and the standard library.

* `Interpreter::default() -> Interpreter`
* `Interpreter::with_clock(clock: Arc<dyn Clock>) -> Interpreter`
* `Interpreter::with_extensions(self, extensions: Arc<Extensions>) -> Interpreter`: answers `Expr::Ext` from `extensions` in place of the standard library alone; `Interpreter::extensions(&self) -> &Arc<Extensions>` reads it back.
* `Interpreter::with_catalogs(self, catalogs: Option<Arc<Catalogs>>) -> Interpreter`: the message catalogs every `Ambient` carries; none by default. `Interpreter::catalogs(&self) -> Option<&Arc<Catalogs>>`.
* `Interpreter::render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: renders a lowered component with `props` bound as `$props`, byte for byte what React's server renderer writes, as `Rendered { html, islands }`. A `Tmpl::Island` renders its component apart, with the caller's children on the slot stack like a `Component`, and leaves `ISLAND_MARK`, its index in `islands` and a NUL in `html` where it sits; `RenderedIsland { module, props, when, body }` holds the evaluated props and the island's own `Rendered`. A root `Slot` with no caller leaves `ROOT_SLOT`. `bind::rendered_nodes(&Rendered) -> Vec<Node>` turns the markup into nodes: raw pieces, `Node::Slot("content")` at `ROOT_SLOT` and, at an island, `Node::raw("<sf-s data-sf-island[ data-sf-when=\"…\"]>")`, a `Node::Client` whose `ssr` is the island's body and `Node::raw("</sf-s>")`. Synchronous: a component body holds no service call, so nothing here suspends. An expression with no `Call` in it is evaluated the same way wherever it appears; only an expression that calls a service goes through the async path.
* `Interpreter::render_module(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: `render` for the component under `module`, which keys its hoisted values; `render` is this with an empty module. `Rendered.hoisted: ValueMap` holds every `Expr::Hoist` value the markup took, keyed `<module>|<id>` or `<module>|<id>@<i>.<j>` under `For` iterations, the callers' loops first, so a component placed from a loop keys below the iteration that placed it; a key recorded twice with different values is removed rather than left wrong. An island starts its own table: `RenderedIsland.body.hoisted`, and `RenderedIsland::mount_props(&self) -> ValueMap` is its props plus that table under `HOISTED_PROP`, `"$h"`, when it is not empty. `IrEvaluator` adds the same key to the root node's props. `interp::Hoists { module, path, table }` is the recorder: `key(id)` spells the key and `record(id, &value)` applies the collision rule. An element whose attributes carry `render::CHUNK_ATTR`, `$chunk`, with an integer id renders its children into a buffer of their own and records that markup as a string under the id before writing it; an attribute whose name starts with `$` is never printed, which also covers the lowerer's `$bound` mark.
* `Interpreter::island_step(&self, module, component, props, state, handler: Option<usize>, event: &Value, library) -> Result<Stepped, Fail>`: one round trip of an island in server mode. The body's `let`s run with `state` standing in for the component's state bindings, the handler at `handler` runs with `$props`, `$state` and `$event` bound and the object it returns is merged into the state for the keys the component names, then the component renders from that state in server mode: `$on:` markers print as `data-sf-on="click:0 change:1"` and `$key` as `data-sf-key`, neither of which prints in a browser-mode render. `None` for `handler` renders as is. `Stepped { state, rendered }`. A missing handler index is `Internal`.
* A `Tmpl::Island` in server mode renders its component the same way and `RenderedIsland { mode, state, .. }` carries the mode and the values the state `let`s took; `mount_props` adds them under `render::STATE_PROP` (`$s`) and `rendered_nodes` writes `data-sf-mode="server"` on the region.
* `render::ROOT_SLOT`: what a root component's own `Slot` writes into the markup, since it has no caller. `IrEvaluator` splits the markup there and emits a `Client` node whose `children` carry the pieces around a `Node::Slot("content")`, which is how a layout places its page.
* `Interpreter::run(&self, body: &Body, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`. `input` is `None` for a loader; a body reads `Expr::Input` as `Value::Null` then. Session writes go to a draft copied from `ctx.session` at entry and are committed to the cell, key by key, only on success.

### Outcome

* `pub struct Outcome { pub value: Value, pub written: Vec<String> }`
* `value` is `Value::Null` when the body ends without a `return`.
* `written` lists every session key set or deleted, in first-touch order, already committed.

### Clock

What `Expr::Now` reads.

* `pub trait Clock: Send + Sync { fn now(&self) -> i128; }`, milliseconds since the Unix epoch.

### Extensions

The registry an `Expr::Ext` is answered from, by `module.member`. `Clone`, `Default` (empty), `Debug` listing the names with their reach.

* `Extensions::empty() -> Extensions`; `Extensions::standard() -> Extensions`: the standard library.
* `register<F>(&mut self, name: impl Into<String>, reach: Reach, f: F)` where `F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static`; replaces what the name held.
* `get(&self, name: &str) -> Option<&Extension>`; `Extension { reach: Reach, .. }` with `call(&self, &Ambient, &[Value]) -> Result<Value, Fail>`.
* `contains(&self, name: &str) -> bool`; `names(&self) -> Vec<String>`, sorted.
* `call(&self, name: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail>`: `Internal` naming the name when nothing holds it.
* `ext::number`, `ext::text`, `ext::text_opt` and `ext::option` read an argument by index for an implementation: a number (`Int`, `UInt`, `F32` or `F64` as `f64`), a string, an optional string and a field of an optional options object, each `Internal` naming the extension on a wrong type.

### Reach

* `pub enum Reach { Render, Body }`, `Copy`, `Eq`; `as_str` is `render` or `body`.
* `Render`: pure, both sides, callable from every site. `Body`: server only, callable from a body, a handler and middleware; the lowerer refuses it on a component's render path.
* `pub const STANDARD: &[(&str, &str, Reach)]`: module, member and reach of every standard member; `standard_reach(module, name) -> Option<Reach>` looks one up. The registry `Extensions::standard` builds and the lowerer's checks are both taken from it.

### Ambient

* `pub struct Ambient { pub locale: String, pub now: i128, pub catalogs: Option<Arc<Catalogs>> }`, `Default`: what a call runs under, the request's locale as the application spells it, empty when none is set, the clock and the message catalogs the interpreter carries.
* `bcp47(&self) -> String`: `fr-FR` for `fr_FR`, `en` when empty. The browser half converts the same way.

### The Standard Library

Every member takes its arguments positionally and answers a `Value`; `std::register` fills an `Extensions` with them. Numbers arrive as `F64`, `Int` or `UInt`; every date is UTC and every instant milliseconds since the epoch, as the browser half's `Date.UTC`.

| Member | Reach | Arguments | Answer |
| --- | --- | --- | --- |
| `intl.number` | render | `n`, `{ minimumFractionDigits?, maximumFractionDigits? }?` | grouped for the locale, half away from zero to at most three fraction digits by default, trailing zeros dropped past the minimum; `NaN`, `∞` and `-∞` as JavaScript prints them |
| `intl.currency` | render | `n`, `code` | the amount with the ISO code and the currency's own fraction digits, `USD 1,234.50`, `1.234,50 EUR`; a code that is not three letters is `Internal` |
| `intl.date` | render | `when` (milliseconds or an ISO 8601 string), `style?` (`short`, `medium` by default, `long`, `full`) or `{ style }` | the calendar date in UTC at that `dateStyle` |
| `intl.plural` | render | `n` | the cardinal category, `zero`, `one`, `two`, `few`, `many` or `other` |
| `text.slug` | render | `s` | NFD, marks dropped, lowercased, every run outside `a-z0-9` one hyphen, none at either end |
| `text.truncate` | render | `s`, `max`, `ellipsis?` (`…`) | the first `max` code points and the ellipsis when longer, else `s` |
| `time.format` | render | `when`, `pattern` | `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced in UTC, every other character kept |
| `time.add` | render | `when`, `amount`, `unit` (`ms`, `s`, `m`, `h`, `d`) | `when` plus `amount` units, `F64` |
| `time.diff` | render | `later`, `earlier`, `unit` | the difference in units, fractional, `F64` |
| `time.parse` | render | `s` | `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`, as `F64` milliseconds; `Null` for anything else |
| `time.now` | body | | `Ambient.now` as `Int` |
| `crypto.hash` | render | `s` | SHA-256 of the UTF-8 bytes as lowercase hex |
| `crypto.verify` | render | `s`, `hash` | whether `hash` is the hash of `s`, compared in constant time, case insensitive |
| `crypto.random` | body | `bytes` (at most 1024) | that many random bytes as hex |
| `id.new` | body | | a UUID version 7 |
| `i18n.t` | render | `key`, `{ count?, …}?` | the message under `key` in the ambient locale's catalog; with `count` a number, `key.<cardinal category>` then `key.other` then `key`; `{name}` filled from a scalar argument, left as written otherwise; the key itself when nothing matches or no catalogs are set. What `t` from the client's std module lowers to |

The locale is `Ambient::bcp47`; a tag ICU4X cannot parse falls back to `en`. `fsr/ir/tests/conformance.rs` runs every `render` member against the client's `dist/std.js` through node over nine locales and fails on any difference the file does not list as a known CLDR divergence; it skips itself when node or the build is absent.

### Catalogs

Message tables by locale, `catalog::Catalogs`, `Clone`, `Default`, `Eq`. `pub type Table = BTreeMap<String, String>`.

* `Catalogs::from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Catalogs`: holds every locale's table merged over the default locale's, so a key a locale lacks reads as the default's, and each merged table as JSON.
* `is_empty(&self) -> bool`; `default_tag(&self) -> &str`; `rows(&self) -> Vec<(String, usize)>`: each locale with how many keys its own table held.
* `table(&self, tag: &str) -> Option<&Arc<Table>>` and `json(&self, tag: &str) -> Option<Arc<str>>`: the merged table for `tag`, the default locale's when `tag` has none, `None` when neither exists.
* `lookup(&self, tag: &str, key: &str) -> Option<&str>`.

## 4. Evaluation Rules

### Reads

* `Param(name)` and `Query(name)` are `Value::Str` or `Value::Null` when absent.
* `Session(key)` reads the draft, so a body sees its own earlier writes; `Value::Null` when absent.
* `Identity(path)` walks `{ subject, claims }` from the session's identity; `Value::Null` when anonymous or when a step is missing.
* `Input` is the value passed to `run`; `Now` is `Value::Int` from the clock.
* `Var(name)` is the innermost binding; an unbound name is `Internal`.

### Truthiness

`Null`, `false`, integer and float zero and the empty string are false. Every other value, including an empty `Seq` or `Map`, is true. `Coalesce` substitutes on `Null` only. `Logic` returns the deciding operand, as JavaScript does.

### Operators

* `Arith` accepts `Int` with `Int` or `F64` with `F64`, plus `Str` with `Str` for `Add` only. Anything else is `Internal`. Integer overflow and division by zero are `Internal`.
* `Compare` accepts two `Int`, two `F64`, two `Str`, two `Bool` or two `Null`. `Null` against anything else is `Eq` false and `Ne` true; any ordering against it is `Internal`. Other mixed pairs are `Internal`.
* `Template` concatenates the string form of each part, per `Str`.
* `Field` on a non-map is `Value::Null`. `Index` accepts a `Map` with a `Str` key or a `Seq` with an `Int`, `UInt` or integral `F64` index, out of range reads `Value::Null`; `Index` on `Null` is `Null`; other targets are `Internal`.

### Conversions

* `Str` renders `Null` as `null`, booleans, integers and strings as themselves and integral floats without a fraction; a collection is `Internal`.
* `Num` produces `F64` from integers, floats, booleans and parseable strings; an unparseable string is `Invalid`.
* `BigInt` produces `Int` from integers, integral floats, booleans and parseable strings; a fractional float or an unparseable string is `Invalid`.

### Builtins

* `Map`, `Filter`, `Find`, `Some`, `Every` apply a one-parameter lambda over a `Seq`; `Reduce` applies a two-parameter lambda `(acc, item)` from `init`. A non-`Seq` operand is `Internal`. `Find` yields `Value::Null` when nothing matches.
* `Entries` yields a `Seq` of two-element `Seq` pairs in insertion order; `Keys` and `Values` likewise; all three require a `Map`.
* `Length` counts `Seq` items, `Str` characters or `Map` entries, as `F64`, since a TypeScript `number` is a float and a `bigint` is an `Int`.
* `Builtin::Omit` takes a `Map` and string keys and yields the map without those keys, the rest of a destructuring; `Null` reads as an empty map and any other first argument is `Internal`.

### Calls

* Arguments evaluate in order; an argument whose value is `Value::Null` is omitted from the `ValueMap` sent.
* The call goes through `RequestCtx::services`. A `ServiceError` becomes a `Fail` with the same kind and message.

### Session writes

* `SessionSet` with an empty path replaces the key. With a path it walks `Map` steps by `Str` key, creating maps where a step is `Null`, plus `Seq` steps by `Int` or integral `F64` index within bounds.
* `SessionDelete` with an empty path removes the key; with a path it removes the last step's entry from its `Map` and is a no-op where the path does not exist.
* Both mark the key in `written`.

### Guards

* Before the body runs, top-level guards are scanned in order. A guard whose condition reads no `Var` and contains no `Call` is evaluated immediately. The scan stops at the first `If`, `ForOf`, `SessionSet` or `SessionDelete`.
* Every guard also runs in sequence at its own position.
* An unknown kind name is `Internal`.

### Parallel lets

* A run of consecutive `Let` statements in one block, none of which reads a name bound earlier in the run, is evaluated together when at least two of them contain a `Call`. Each evaluates over a snapshot of the current scope and draft, which is safe because writes are statements, never expressions.
* Any other statement is evaluated in sequence, as is any `Let` that reads an earlier name in the run.

## 5. Runtime Adapters

### IrSource

A body answering a data source id. Implements `snapfire_fsr_runtime::DataSource`.

* `IrSource::new(id: impl Into<String>, body: Body) -> IrSource`
* `IrSource::with_interpreter(self, interpreter: Interpreter) -> IrSource`
* `load` runs the body with no input and returns the `Value::Map` it returned as `Data`. A non-map return or a `Fail` is a `LoadError` carrying the id and the message.

### IrAction

A body answering an action id. Implements `snapfire_fsr_runtime::ActionHandler`.

* `IrAction::new(body: Body) -> IrAction`
* `IrAction::with_interpreter(self, interpreter: Interpreter) -> IrAction`
* `call` runs the body with the submitted value as `Input` and returns what it returned. A `Fail` is an `ActionError` with the kind preserved.

## 6. Error Handling

### Fail

* `pub struct Fail { pub kind: FailureKind, pub message: String }`; derives `Debug`, `Clone`, `PartialEq`, implements `std::error::Error` and `Display` as `{kind}: {message}`.
* `Fail::new(kind: FailureKind, message: impl Into<String>) -> Fail`
* A guard yields its named kind. A service error keeps its kind. `Num` and `BigInt` of unparseable input are `Invalid`. Type mismatches, unbound names, overflow and structural misuse are `Internal`.

### ParseError

* `pub struct ParseError(serde_json::Error)`, returned by `from_json`; `Display` is `malformed IR: {inner}`.
