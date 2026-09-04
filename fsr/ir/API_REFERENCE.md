# API Reference: snapfire_fsr_ir

The lowered form of a loader or action body and the interpreter that runs it over the value model.

## Contents

* [1. The Tree](#1-the-tree)
  * [Body](#body)
  * [Stmt](#stmt)
  * [Expr](#expr)
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

Runs a body. `Clone`; the default carries the system clock.

* `Interpreter::default() -> Interpreter`
* `Interpreter::with_clock(clock: Arc<dyn Clock>) -> Interpreter`
* `Interpreter::render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<String, Fail>`: renders a lowered component with `props` bound as `$props`, byte for byte what React's server renderer writes. Synchronous: a component body holds no service call, so nothing here suspends. An expression with no `Call` in it is evaluated the same way wherever it appears; only an expression that calls a service goes through the async path.
* `render::ROOT_SLOT`: what a root component's own `Slot` writes into the markup, since it has no caller. `IrEvaluator` splits the markup there and emits a `Client` node whose `children` carry the pieces around a `Node::Slot("content")`, which is how a layout places its page.
* `Interpreter::run(&self, body: &Body, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`. `input` is `None` for a loader; a body reads `Expr::Input` as `Value::Null` then. Session writes go to a draft copied from `ctx.session` at entry and are committed to the cell, key by key, only on success.

### Outcome

* `pub struct Outcome { pub value: Value, pub written: Vec<String> }`
* `value` is `Value::Null` when the body ends without a `return`.
* `written` lists every session key set or deleted, in first-touch order, already committed.

### Clock

What `Expr::Now` reads.

* `pub trait Clock: Send + Sync { fn now(&self) -> i128; }`, milliseconds since the Unix epoch.

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
