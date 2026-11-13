# API Reference: snapfire_fsr_ir

The translated form of FSR's server TypeScript, its two text forms, the interpreter and renderer that run it and the adapters that bind it to the runtime.

## Contents

* [1. The Tree](#1-the-tree)
  * [`Body`](#body)
  * [`Consts`](#consts)
  * [`Stmt`](#stmt)
  * [`Expr`](#expr)
  * [`Builtin`](#builtin)
  * [`Entry`](#entry)
  * [`Lit`](#lit)
  * [`ArithOp`](#arithop)
  * [`CompareOp`](#compareop)
  * [`LogicOp`](#logicop)
  * [`Tmpl`](#tmpl)
  * [`Component`](#component)
  * [`Handler`](#handler)
  * [`Owner`](#owner)
  * [`ShadowRoot`](#shadowroot)
  * [`ShadowMode`](#shadowmode)
  * [`ScopedStyle`](#scopedstyle)
* [2. Tree Queries](#2-tree-queries)
  * [`body_visit`](#body_visit)
  * [`body_visit_mut`](#body_visit_mut)
  * [`body_free_vars`](#body_free_vars)
  * [`set_targets`](#set_targets)
  * [`body_reads_request`](#body_reads_request)
  * [`body_reads_ambient`](#body_reads_ambient)
  * [`body_params_read`](#body_params_read)
  * [`media_sources`](#media_sources)
  * [`read_by_host`](#read_by_host)
  * [`head_scripts`](#head_scripts)
  * [`HeadScript`](#headscript)
* [3. The S-Expression Form](#3-the-s-expression-form)
  * [`Sx`](#sx)
  * [`parse`](#parse)
  * [`forms`](#forms)
  * [`Forms`](#forms-1)
  * [`print`](#print)
  * [`read_all`](#read_all)
  * [`expr_to_sx`](#expr_to_sx)
  * [`tmpl_to_sx`](#tmpl_to_sx)
  * [`stmt_to_sx`](#stmt_to_sx)
  * [`component_to_sx`](#component_to_sx)
  * [`component_sections`](#component_sections)
* [4. The Interpreter](#4-the-interpreter)
  * [`Interpreter`](#interpreter)
  * [`Outcome`](#outcome)
  * [`Clock`](#clock)
  * [`Derive`](#derive)
  * [`Prepared`](#prepared)
  * [`Program`](#program)
  * [`native`](#native)
  * [`Read`](#read)
  * [`Frame`](#frame)
* [5. Rendering](#5-rendering)
  * [`Components`](#components)
  * [`bake_static_markup`](#bake_static_markup)
  * [`Rendered`](#rendered)
  * [`RenderedIsland`](#renderedisland)
  * [`RenderedPlace`](#renderedplace)
  * [`Stepped`](#stepped)
  * [`Emitted`](#emitted)
  * [`HandlerRef`](#handlerref)
  * [`Hoists`](#hoists)
  * [`Step`](#step)
  * [`rendered_nodes`](#rendered_nodes)
  * [Render functions](#render-functions)
  * [Markup marks](#markup-marks)
  * [Prop keys](#prop-keys)
  * [Attribute and tag names](#attribute-and-tag-names)
  * [Slot and context names](#slot-and-context-names)
  * [Limits](#limits)
* [6. Frameworks](#6-frameworks)
  * [`Frameworks`](#frameworks)
  * [`ReactMajor`](#reactmajor)
  * [`VueMajor`](#vuemajor)
  * [`SvelteMajor`](#sveltemajor)
  * [Markup rules](#markup-rules)
* [7. Extensions](#7-extensions)
  * [`Extensions`](#extensions)
  * [`Extension`](#extension)
  * [`Member`](#member)
  * [`Reach`](#reach)
  * [`Ambient`](#ambient)
  * [`Catalogs`](#catalogs)
  * [Argument readers](#argument-readers)
  * [The standard library](#the-standard-library)
* [8. Evaluation Rules](#8-evaluation-rules)
  * [Reads](#reads)
  * [Truthiness](#truthiness)
  * [Operators](#operators)
  * [Conversions](#conversions)
  * [Builtins](#builtins)
  * [Calls](#calls)
  * [Session writes](#session-writes)
  * [Guards](#guards)
  * [Parallel lets](#parallel-lets)
* [9. Regular Expressions and Math](#9-regular-expressions-and-math)
  * [`JsRegex`](#jsregex)
  * [`Found`](#found)
  * [`jsregex` functions](#jsregex-functions)
  * [`jsmath`](#jsmath)
* [10. Runtime Adapters](#10-runtime-adapters)
  * [`PreparedBody`](#preparedbody)
  * [`IrSource`](#irsource)
  * [`IrPaths`](#irpaths)
  * [`IrStore`](#irstore)
  * [`IrAction`](#iraction)
  * [`IrMeta`](#irmeta)
  * [`IrEvaluator`](#irevaluator)
* [11. The Host](#11-the-host)
  * [`Event`](#event)
  * [`Report`](#report)
  * [`Act`](#act)
  * [Host keys](#host-keys)
* [12. Error Handling](#12-error-handling)
  * [`Fail`](#fail)
  * [`SexprError`](#sexprerror)
  * [`ShadowRootError`](#shadowrooterror)

## 1. The Tree

Every enum in this section is serialized externally tagged in `snake_case` unless its entry says otherwise.

### Body

* `pub type Body = Vec<Stmt>`: the statements of one loader, action, handler or middleware, in order.

### Consts

* `pub type Consts = BTreeMap<String, Expr>`: the module-level constants a plan names, keyed `<file>#<name>`. `Expr::Const` reads one.

### Stmt

One statement. `Debug`, `Clone`, `PartialEq`.

* `Let { name: String, expr: Expr }`: binds `name` for the rest of the enclosing block.
* `Set { name: String, expr: Expr }`: gives the nearest binding of `name` a new value. A name nothing has bound is `Internal`.
* `If { cond: Expr, then: Body, else: Body }`: `else` is omitted from the plan text when empty.
* `ForOf { name: String, over: Expr, body: Body }`: `over` must evaluate to `Value::Seq`.
* `While { cond: Expr, body: Body, step: Body }`: `step` runs after every pass, including one a `Continue` ends. A loop past `interp::MAX_PASSES` passes fails the step. `step` is omitted from the plan text when empty.
* `Return(Expr)`: ends the body, from any depth.
* `Break`: leaves the nearest loop.
* `Continue`: ends this pass of the nearest loop.
* A `Break` or `Continue` that reaches a function's body or a block expression fails with `Internal`. The text forms are `(break)` and `(continue)`.
* `Guard { cond: Expr, kind: String, message: Expr }`: fails the body with `kind` when `cond` is truthy. `kind` is a `FailureKind` name. `message` is evaluated only when the guard fires.
* `SessionSet { key: String, path: Vec<Expr>, value: Expr }`: `path` is omitted from the plan text when empty.
* `SessionDelete { key: String, path: Vec<Expr> }`: `path` is omitted from the plan text when empty.
* `SessionExtend { seconds: Expr }`: the session ends `seconds` from now once the body commits.
* `Act { action: String, input: Expr }`: asks the host to dispatch `action` with `input`. Only a handler holds one. `island_step` collects it in `Stepped::acts`. A body that reaches one fails with `Internal`.
* `Try { body: Body, name: Option<String>, handler: Option<Body>, finally: Body }`: `try`, `catch` and `finally`.
* `Try`'s `handler` runs when `body` fails for any reason: a `throw`, a guard, a call or the interpreter. `None` is a `try` with no `catch`.
* `Try`'s `name` binds what was thrown. For a failure nothing threw, it binds `{ name, kind, message }`. `name` is the JavaScript error a message starting `TypeError: `, `RangeError: `, `SyntaxError: `, `ReferenceError: `, `EvalError: ` or `URIError: ` names, with that prefix taken off `message`, and `Error` otherwise.
* `Try`'s `finally` runs on every way out of the other two. When it returns or fails, that replaces how they ended.
* A deferred thrown for a Suspense boundary is not caught by `Try`.
* `Try`'s `name`, `handler` and `finally` are omitted from the plan text when absent or empty.
* `Expr(Expr)`: evaluates for effect and discards the value.

### Expr

One expression. `Debug`, `Clone`, `PartialEq`.

Request reads, each described in [Reads](#reads):

* `Param(String)`, `Query(String)`, `ParamsAll`, `QueryAll`, `Session(String)`, `Identity(Vec<String>)`.
* `Locale`, `Locales`, `Path`, `Document`, `Host`, `Origin`, `Address`, `Config(String)`, `Input`, `Now`.
* `Store(String)`: a store key, read from the seed the route's `store` settled on. A nested component reads it without a prop.
* `Context(String)`: `useContext(c)`, the value the nearest provider of the context `file#name` holds around the render point. `Value::Null` under none.

Names and literals:

* `Var(String)`: the innermost binding. An unbound name is `Internal`.
* `Const(String)`: a module-level constant from the plan's `consts`, `<file>#<name>`.
* `Lit(Lit)`.
* `Folded(Value)`: a value built once by `render::prepare`. Skipped by serde and never in a plan.
* `Object(Vec<Entry>)`: takes `Field`, `Computed`, `Spread` and `Baked` entries.
* `Array(Vec<Entry>)`: takes `Item` and `Spread` entries.
* A wrong entry kind in an `Object` or `Array` is `Internal`.

Operators:

* `Field(Box<Expr>, String)`, `Index(Box<Expr>, Box<Expr>)`.
* `Arith(ArithOp, Box<Expr>, Box<Expr>)`, `Compare(CompareOp, Box<Expr>, Box<Expr>)`, `Logic(LogicOp, Box<Expr>, Box<Expr>)`.
* `Not(Box<Expr>)`, `Coalesce(Box<Expr>, Box<Expr>)`, `Ternary(Box<Expr>, Box<Expr>, Box<Expr>)`, `Template(Vec<Expr>)`.

Calls:

* `Call { service: String, method: String, args: Vec<(String, Expr)> }`: `await services.<service>.<method>(args)`.
* `NativeCall { module: String, method: String, args: Vec<(String, Expr)>, sync: bool }`: `await native.<module>.<method>(args)`, the application's own Rust in this process. `args` defaults to empty and `sync` is omitted when false.
* `Next(Box<Expr>)`: middleware's `next(request)`, run through `RequestCtx.next`. The request is `undefined` for the request as it came. Only the async evaluator runs it.
* `After(Box<Expr>)`: `after(fn)`. The lambda is pushed onto `RequestCtx.after` with a copy of the environment it was evaluated in. Evaluates to `undefined`. A failure while the lambda runs is a warning under the target `fsr::after`.
* `Builtin { name: Builtin, args: Vec<Expr> }`: a fixed pure function, described in [Builtins](#builtins).
* `Ext { module: String, name: String, args: Vec<Expr> }`: an extension call such as `intl.number(n)`, answered by the interpreter's `Extensions` under `module.name` with the arguments evaluated and the request's `Ambient`. A name the registry lacks is `Internal`: ``extension `x.y` is not registered``.

Functions:

* `Lambda { params: Vec<String>, body: Box<Expr> }`: a parameter named `...x` takes every argument past the named ones as an array. It evaluates to a function value tagged `interp::FUNCTION_VALUE`.
* `Fix { name: String, f: Box<Expr> }`: `f`, a `Lambda`, as a function value that sees itself under `name`. A call nested `interp::MAX_APPLIES` (256) deep fails the step naming the function.
* `Apply { f: Box<Expr>, args: Vec<Expr> }`: calls `f` with `args`.
* `ApplyOf(Box<Expr>, Box<Expr>)`: calls `f` with the items of the array as its arguments, `f(a, ...rest)`. An array that is not a `Seq` is `TypeError`.
* `Block(Body)`: statements run in the enclosing scope. Its value is what the first `Return` yields, null when none.
* A `Let` inside a `Block` is dropped when the block ends. A `Set` to a name bound outside writes that binding.
* A `Block` runs on the sync path, which refuses a guard, a session write and a dispatch. It runs on the async path when a service call sits inside. Its s-expression form is `(block stmt ...)`.

Collections, each `Box<Expr>`:

* `Map(over, f)`, `Filter(over, f)`, `Reduce(over, init, f)`, `Find(over, f)`, `FindIndex(over, f)`, `Some(over, f)`, `Every(over, f)`, `FlatMap(over, f)`: `f` must be a `Lambda`.
* `Sort(over, f)`: a sorted copy, stable. `f` is the comparator lambda. `Lit::Null` sorts by `String(x)` in UTF-16 code units.
* `ReplaceWith(subject, pattern, f)`: `s.replace(pattern, f)`.
* `Entries`, `Keys`, `Values`, `Length`: one operand each.
* `Str`, `Num`, `BigInt`: one operand each, described in [Conversions](#conversions).

Render state:

* `Hoist { id: u32, expr: Box<Expr> }`: an expression whose inputs are props only. It evaluates as `expr`. Under a render the value is also recorded in the environment's `Hoists` under `id`. `id` is unique within the component's module.
* `HoistId(u32)`: a `useId` call. It evaluates to `Hoists::id`: `interp::ID_OPEN` (`_S_`), the hoist key and `interp::ID_CLOSE` (`_`). For example `_S_src/Form.tsx#default|0_` at the component's own level and `_S_src/Field.tsx#default|0@c1_` under a keyed placement.
* `HoistId` with no `Hoists` in the environment is an internal `Fail`. Its s-expression form is `(hoist-id n)`.
* `Cell(Box<Expr>)`: a fresh cell holding the value. It evaluates to a handle, a `Value::Variant` tagged `interp::CELL_VALUE` whose payload is the cell's index in the render's table.
* `ModuleCell { key: Box<Expr>, init: Box<Expr> }`: the render's cell named by the string `key` evaluates to, made from `init` the first time it is reached and the same cell after that.
* `ReadCell(Box<Expr>)`: what a handle's cell holds.
* `WriteCell(Box<Expr>, Box<Expr>)`: writes the value into the handle's cell and evaluates to the value.
* The s-expression forms are `(cell e)`, `(module-cell "key" e)`, `(read-cell e)` and `(write-cell h v)`.
* `WriteStore(String, Box<Expr>)`: writes the value to the store key and evaluates to the value.
* `Handler(u32)`: the rendering component's handler at that index as a value. It evaluates to `render::handler_value(path, index)`. Outside a render it is an error. The plan writes `(handler 2)`.
* `Markup(Arc<Tmpl>)`: markup written where a value goes. It evaluates to a markup value tagged `render::MARKUP_VALUE`, which renders where it is interpolated, in the scope it was written in.

Constructors:

* `lit_str(s: impl Into<String>) -> Expr`
* `lit_int(n: impl Into<i128>) -> Expr`
* `var(name: impl Into<String>) -> Expr`
* `field(self, name: impl Into<String>) -> Expr`
* `index(self, key: Expr) -> Expr`
* `call(service: impl Into<String>, method: impl Into<String>, args: Vec<(&str, Expr)>) -> Expr`
* `native_call(module: impl Into<String>, method: impl Into<String>, args: Vec<(&str, Expr)>, sync: bool) -> Expr`
* `ext(module: impl Into<String>, name: impl Into<String>, args: Vec<Expr>) -> Expr`
* `lambda(params: &[&str], body: Expr) -> Expr`
* `object(entries: Vec<(&str, Expr)>) -> Expr`: `Field` entries only.

Queries:

* `free_vars(&self, out: &mut Vec<String>)`: appends every `Var` read and not bound by an enclosing lambda, without duplicates. A `Block` also adds the names its `Set`s write.
* `visit(&self, f: &mut dyn FnMut(&Expr))`: calls `f` on the expression and every expression beneath it, in tree order.
* `visit_mut(&mut self, f: &mut dyn FnMut(&mut Expr))`: the same, each expression open to change.
* `has_call(&self) -> bool`: true when a `Call`, `NativeCall`, `Next` or `ApplyOf` appears in the tree. `After` and `Ext` do not count.
* `reads_request(&self) -> bool`: true when the tree reads anything that differs between requests.
* `reads_request` counts `Param`, `Query`, `ParamsAll`, `QueryAll`, `Session`, `Store`, `Context`, `Identity`, `Input`, `Now`, `Host`, `Address`, `Next`, `After` and an `Ext` whose standard reach is not `Render`.
* `reads_request` does not count `Locale`, `Locales`, `Path`, `Document`, `Origin` or `Config`.

### Builtin

The pure functions a body may call by name. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub enum Builtin { Round, Floor, Ceil, Abs, Min, Max, ToFixed, Repeat, Join, Trim, Upper, Lower, Includes, StartsWith, EndsWith, Split, Replace, EncodeUriComponent, DecodeUriComponent, LocaleNumber, Range, Omit, Slice, At, IndexOf, Concat, Reverse, PadStart, PadEnd, Substring, Json, JsonParse, Pow, Sqrt, Trunc, Sign, MinOf, MaxOf, DateMs, DatePart, IsoString, FromEntries, Unique, HasKey, ClassNames, TypeOf, IsArray, IsError, WriteBack, Log, LogOf, Throw, ComponentRef, IsElement, CloneElement, FormEncode, FormGet, FormHas, FormGetAll, FormEntries, ClassOf, CallMethod, GetMember, FormSet, FormAppend, FormDelete, UrlParse, Response, ResponseJson, ResponseRedirect, RedirectTo, RequestNew, ResponseMember, ResponseHeaderGet, ResponseHeaderSet, Uint8Array, UrlPart, UrlSet, RegexTest, Match, MatchAll, Search, ReplaceAll, Checked, LooseMatch, LooseEq, HeapOn, Overwrite, CopyDeep, MapKey, MapOf, AsMap, AsSet, AsError, Is, UrlTry, SymbolKeys, SymbolNew, SymbolDescription, SymbolKeyFor, RegExpEscape, TypedNew, TypedLike, TypedField, AsOpaque, IsNaN, IsNullish, ParseInt, ParseFloat, Substr, IsWellFormed, ToWellFormed, Exec, UtcString, EncodeUri, DecodeUri, SameValue, FromCharCode, FromCodePoint, DateUtc, ToPrecision, ToExponential, Normalize, ToSpliced, ArrayWith, Fill, CopyWithin, Iterate, Spread, Splice, ExecFrom, TestFrom, Tagged, TaggedPayload, IsTagged, ElementOf, ViewGet, ViewSet, Segments, Settle, LastIndexOf, CharAt, CharCodeAt, Math, CodePointAt, Radix, Graphemes, SetAt, Emit }`
* `pub fn atom(self) -> &'static str`: the builtin's name in the plan, `from-char-code` for `FromCharCode`.

### Entry

One member of an object literal, an array literal or an element's attributes. `Debug`, `Clone`, `PartialEq`.

* `Field(String, Expr)`: an object member.
* `Baked(String)`: attributes already printed, ` name="value"` text written as it stands. The plan spells it `(! "…")`.
* `Computed(Expr, Expr)`: `{ [key]: value }`. The key must evaluate to `Str`, else `Internal`.
* `Item(Expr)`: an array member.
* `Spread(Expr)`: into an object the value must be `Value::Map` or `Value::Null`. Into an array it must be `Value::Seq` or `Value::Null`.

### Lit

* `pub enum Lit { Null, Undefined, Bool(bool), Int(i128), Float(f64), Str(String) }`. `Debug`, `Clone`, `PartialEq`.

### ArithOp

* `pub enum ArithOp { Add, Sub, Mul, Div, Rem, Pow, Quot, BitAnd, BitOr, BitXor, Shl, Shr, UShr }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.
* `Div`: two `Int`s give an `Int` when the division is exact and an `F64` otherwise. Dividing by zero gives an infinity or `NaN`.
* `Pow`: `**`. An `Int` power of two `Int`s is computed as a `bigint` computes it.
* `Quot`: `bigint` division. It truncates toward zero and fails on zero. The plan spells it `quot`.
* `BitAnd`, `BitOr`, `BitXor`, `Shl` and `Shr`: `&`, `|`, `^`, `<<` and `>>`. Each side goes through `ToNumber` and `ToInt32`, the shift count is taken modulo 32 and the answer is an `Int`.
* `UShr`: `>>>`, its left side read through `ToUint32`.
* The plan spells the bitwise operators `bit-and`, `bit-or`, `bit-xor`, `<<`, `>>` and `>>>`.

### CompareOp

* `pub enum CompareOp { Eq, Ne, Lt, Le, Gt, Ge }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

### LogicOp

* `pub enum LogicOp { And, Or }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

### Tmpl

A translated component's tree. `Debug`, `Clone`, `PartialEq`.

* `compact(&mut self)`: the tree's attributes, props and children trimmed to their lengths, all the way down.

* `Text(String)`
* `Expr(Expr)`
* `Element { tag: String, attrs: Vec<Entry>, children: Vec<Tmpl> }`: attributes are `Entry::Field` in HTML spelling or `Entry::Spread` of a map in React's spelling, later entries winning.
* `Fragment(Vec<Tmpl>)`
* `If { cond: Expr, then: Box<Tmpl>, else: Option<Box<Tmpl>> }`
* `For { over: Expr, params: Vec<String>, body: Box<Tmpl> }`
* `Let { name: String, expr: Expr, then: Box<Tmpl> }`
* `Component { module: String, props: Vec<Entry>, children: Vec<Tmpl>, id: u32, keyed: bool }`: another component applied to props. `children` render in the caller's scope wherever the callee places its `Slot`.
* `Component`'s `id` is the placement's index among the enclosing component's hoist candidates. With `keyed` set, the placement adds `c<id>` to the key path of what it renders.
* `Island { module: String, props: Vec<Entry>, children: Vec<Tmpl>, when: Option<String>, mode: Option<String>, id: u32, define: bool }`: a component placed as its own island.
* `Island`'s `when` is its hydration timing, `"load"`, `"visible"` or `"idle"`. `mode` is `Some("server")` for an island whose events round-trip to the server.
* `Island`'s `define` marks a module that is an element definition. The server renders `children` inside the marker and the browser imports the module at the island's timing.
* `Slot(String)`: the caller's children where the callee places them. At a layout's root, the plan child of that name.
* `Marker(String)`: markup a framework's hydration reads, such as `<!--[0-->`, written as given. Nothing evaluates in it and it counts as no child. The plan spells it `(marker "<!--[0-->")`.
* `Html(Expr)`: the value written unescaped, `null` as nothing. The plan spells it `(html expr)`.
* `Baked { open: String, tag: Option<String>, children: Vec<Tmpl> }`: an element whose open tag is literal, made by `render::prepare`. Never in a plan. `tag` is `None` for a void element.
* `Place { target: String, children: Vec<Tmpl> }`: children written at `target` instead of in place. `target` is a `Place` spelling: `body` for the end of the body or `#name` for a named place. The plan spells it `(place "body" child …)`.
* `Place`'s children are recorded in `Rendered::places` with a `render::PLACE_MARK` where they sat. They are never static.
* `Outlet { name: String, hydrated: bool }`: where the content placed at `#name` is written, inside the element around it. The plan spells it `(outlet "name")`. With `hydrated` set it is `(outlet-hydrated "name")`.
* An `Outlet` inside markup a framework hydrates fails the render unless `hydrated` is set. It is never static.

### Component

A translated component: `let`s run once with `$props` bound, then the tree.

* `pub struct Component { pub body: Body, pub render: Tmpl, pub state: Vec<String>, pub stores: Vec<(String, String)>, pub handlers: Vec<Handler>, pub owner: Owner, pub shadow: Option<ShadowRoot>, pub scope: Option<ScopedStyle> }`. `Debug`, `Clone`, `PartialEq`.
* `state`: the body `let`s the browser can change, the `useState` and `useStore` bindings in order.
* `stores`: the `useStore` bindings among `state`, each with the store key it reads. The plan writes `(stores (count "cart/count") ...)`.
* `compact(&mut self)`: every list the component holds trimmed to its length: its body, its tree's attributes, props and children and every expression's entries and arguments.
* `handlers`: the component's event handlers, for an island in server mode.
* `owner`: what renders the component in the browser. The plan writes `(owner fsr)`, `(owner react)`, `(owner vue)` or `(owner svelte)`.
* `shadow`: the shadow root an element template declares, read by `ShadowRoot::take`. `None` for every other component.
* `scope`: a Vue component's scoped style. `None` for every other component.
* `new(owner: Owner, body: Body, render: Tmpl) -> Self`: no state, stores, handlers, shadow root or scope.
* `store_key(&self, name: &str) -> Option<&str>`: the store key `name` reads, when it is a `useStore` binding.
* `visit(&self, f: &mut dyn FnMut(&Expr))`: every expression in the body and the tree.
* `visit_mut(&mut self, f: &mut dyn FnMut(&mut Expr))`: the same open to change, the handlers' bodies included.
* `reads_prop(&self, name: &str) -> bool`: true when the component reads `$props.<name>` or uses `$props` whole.

### Handler

* `pub struct Handler { pub event: String, pub body: Body }`. `Debug`, `Clone`, `PartialEq`.
* The body runs with `$props`, `$state` and `$event` bound after the component's `let`s. It returns an object whose keys are the state names it sets.
* An element binds a handler through an attribute `$on:<event>` holding its index.

### Owner

What renders a component in the browser.

* `pub enum Owner { Fsr, React, Vue, Svelte }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.
* `Fsr`: the server renders it and nothing renders it again.
* `React`: a TSX component the React adapter hydrates.
* `Vue`: a `.vue` file the Vue adapter hydrates.
* `Svelte`: a `.svelte` file the Svelte adapter hydrates.
* `as_str(self) -> &'static str`: `fsr`, `react`, `vue` or `svelte`.
* `of(word: &str) -> Option<Owner>`: reads one back.
* `hydrates(self) -> bool`: true for every owner but `Fsr`.

### ShadowRoot

The declarative shadow root the server writes around an element template.

* `pub struct ShadowRoot { pub mode: ShadowMode, pub delegates_focus: bool, pub clonable: bool, pub serializable: bool }`. `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`. The default is an open root with no options.
* `take(render: &mut Tmpl) -> Result<Option<ShadowRoot>, ShadowRootError>`: reads the root `<template>` of an element template as its shadow root and leaves that template's children as the render.
* `take` treats as the root either the render itself or the only child of a root `Fragment`. `Ok(None)` when the root is anything else. A `<template shadowrootmode>` deeper in the tree is left alone.
* `shadowrootmode` must be the literal string `open` or `closed`.
* `shadowrootdelegatesfocus`, `shadowrootclonable` and `shadowrootserializable` must be literal booleans. A bare attribute is `true`.
* `take` refuses any other attribute, a spread, a root `<template>` without `shadowrootmode` and a `<template shadowrootmode>` beside other root nodes or in a branch of a root `If`.
* The plan writes `(shadow closed delegatesfocus)` after `(owner ...)`, naming each option that is true.

### ShadowMode

* `pub enum ShadowMode { Open, Closed }`. `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`. `Open` by default.
* `of(mode: &str) -> Option<ShadowMode>`: reads `open` or `closed`.
* `as_str(self) -> &'static str`

### ScopedStyle

A Vue component's scoped style.

* `pub struct ScopedStyle { pub id: String, pub slotted: bool }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `id`: the `data-v-<hash>` attribute its rules select on. A `.vue` child placed in the component's slot content takes `id` on its root.
* `slotted`: true when one of its rules is `:slotted()`.
* `slotted_attr(&self) -> String`: `<id>-s`. With `slotted` set, the renderer writes it on every element of the content the component's slots render.
* The plan writes `(scope "data-v-1a2b" slotted)` after `(owner ...)` and `(shadow ...)`, with `slotted` only when true.

## 2. Tree Queries

All in `snapfire_fsr_ir::ast`. Those marked as root re-exports are also at the crate root.

### body_visit

* `pub fn body_visit(body: &Body, f: &mut dyn FnMut(&Expr))`: every expression the body holds, in statement order. Root re-export.

### body_visit_mut

* `pub fn body_visit_mut(body: &mut Body, f: &mut dyn FnMut(&mut Expr))`: the same, open to change.

### body_free_vars

* `pub fn body_free_vars(body: &Body) -> Vec<String>`: the names a body reads and does not bind itself, in first-read order. A `let` binds for the statements after it and a `for-of` for its own body. Root re-export.

### set_targets

* `pub fn set_targets(body: &Body) -> Vec<String>`: the names a body's `Set` statements write, at any depth.

### body_reads_request

* `pub fn body_reads_request(body: &Body) -> bool`: true when any statement reads the request as `Expr::reads_request` counts it. Also true when any statement writes the session. Root re-export.

### body_reads_ambient

* `pub fn body_reads_ambient(body: &Body) -> bool`: true when a body reads anything of the request other than its input: a parameter, the query, the session, the identity or the clock. The locale is not counted. Root re-export.

### body_params_read

* `pub fn body_params_read(body: &Body) -> Vec<String>`: every route parameter the body reads, by name, without duplicates. Root re-export.

### media_sources

Each takes a `&Tmpl` and answers a `Vec<String>` in the order written. A value counts when it is a string literal or a template whose first part is a string, which gives that part.

* `pub fn media_sources(tmpl: &Tmpl) -> Vec<String>`: the `src` of every `video`, `audio`, `source` and `track`.
* `pub fn image_sources(tmpl: &Tmpl) -> Vec<String>`: the `src` of every `img`.
* `pub fn embed_players(tmpl: &Tmpl) -> Vec<String>`: the player URL of every `data-sf-embed` placeholder. For a computed id it is the URL's start.
* `pub fn computed_embed_players(tmpl: &Tmpl) -> Vec<String>`: the start of the player URL of every embed placeholder whose id the render computes.

### read_by_host

* `pub fn ast::read_by_host(tag: &str, attrs: &[Entry]) -> bool`: true for `style`, `img`, `video`, `audio`, `source` and `track` and for an element carrying `data-sf-embed`, which the host reads out of a loaded plan.
* `render::bake_static_markup` never prebuilds such an element.

### head_scripts

* `pub fn head_scripts(body: &Body) -> Vec<HeadScript>`: every `{ tag: "script" }` object in a `meta` body.
* A parameter of an inlined helper reads as the literal it was given.
* A row whose `type` reads as a data block (`snapfire_fsr_runtime::is_data_block`), `jsonLd`'s among them, is not listed. A `type` that cannot be read is taken as code.

### HeadScript

* `pub struct HeadScript { pub src: Option<String>, pub inline: bool }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `src`: set when it is a string literal or a parameter of an inlined helper given one.
* `inline`: true when the row carries `children`.

## 3. The S-Expression Form

The module `snapfire_fsr_ir::sexpr`. The plan's own forms are in `snapfire_fsr_plan::sexpr`.

Every `_to_sx` and `_from_sx` pair round-trips: `parse(print(to(x))) == to(x)` and `from(to(x)) == x`.

### Sx

* `pub enum Sx { Sym(String), Str(String), List(Vec<Sx>), Interp(Box<Sx>) }`: one node of the syntax.
* A symbol and a string are distinct terms. `nil` is the null literal and `"nil"` the string.
* `Interp` is `{expr}`, an expression where a template child is expected.

### parse

* `pub fn parse(src: &str) -> Result<Vec<Sx>, SexprError>`: every top-level form.
* `;` runs to the end of the line. `"..."` is a string. `|...|` is a symbol whose bare spelling would not lex.
* `\n`, `\r` and `\t` are those characters. Any other escape is the whole character after it, so `\é` is `é`.
* A syntax error names its line.

### forms

* `pub fn forms(src: &str) -> Forms<'_>`: the top-level forms, read one at a time.

### Forms

* `pub struct Forms<'a>`: `Iterator<Item = Result<Sx, SexprError>>`. After an error it yields nothing more.

### print

* `pub fn print(forms: &[Sx]) -> String`: one form per top-level line, wrapped at 100 columns and indented by two.

### read_all

* `pub fn read_all<T>(items: &[Sx], read: impl FnMut(&Sx) -> Result<T, SexprError>) -> Result<Vec<T>, SexprError>`: `read` over every item into a vector of exactly `items.len()`, stopping at the first error.

### expr_to_sx

* `pub fn expr_to_sx(expr: &Expr) -> Sx`
* `pub fn expr_from_sx(sx: &Sx) -> Result<Expr, SexprError>`
* A string literal is a quoted term and a variable a bare symbol, so `(class "wide")` is a field holding a literal.

### tmpl_to_sx

* `pub fn tmpl_to_sx(tmpl: &Tmpl) -> Sx`
* `pub fn tmpl_from_sx(sx: &Sx) -> Result<Tmpl, SexprError>`

### stmt_to_sx

* `pub fn stmt_to_sx(stmt: &Stmt) -> Sx`
* `pub fn stmt_from_sx(sx: &Sx) -> Result<Stmt, SexprError>`

### component_to_sx

* `pub fn component_to_sx(component: &Component) -> Sx`
* `pub fn component_from_sx(sx: &Sx) -> Result<Component, SexprError>`

### component_sections

* `pub fn component_sections(component: &Component) -> Vec<Sx>`: the sections without the `component` head.
* `pub fn component_from_sections(sections: &[Sx]) -> Result<Component, SexprError>`
* A component's sections open with `(owner ...)`. The legacy sections `(static)`, `(tree)` and `(vue)` read as `Fsr`, `React` and `Vue`.

## 4. The Interpreter

### Interpreter

Runs bodies and renders components. `Clone`. `Default` carries the system clock, the standard library and no catalogs, files, constants, frameworks, island reads, derives or config.

Building:

* `with_clock(clock: Arc<dyn Clock>) -> Interpreter`: the default with `clock`.
* `with_extensions(self, extensions: Arc<Extensions>) -> Interpreter`: answers `Expr::Ext` from `extensions` in place of the standard library alone.
* `extensions(&self) -> &Arc<Extensions>`
* `with_catalogs(self, catalogs: Option<Arc<Catalogs>>) -> Interpreter`: the message catalogs every `Ambient` carries. With none, every `i18n.t` key answers as itself.
* `catalogs(&self) -> Option<&Arc<Catalogs>>`
* `with_files(self, files: Option<Arc<FileRoots>>) -> Interpreter`: the roots every `Ambient` carries, which `file` resolves and confines through. With none, a path is the working directory's.
* `with_consts(self, consts: Option<Arc<Consts>>) -> Interpreter`: the plan's constants, which `Expr::Const` reads.
* `consts(&self) -> Option<&Arc<Consts>>`
* `with_config(self, config: Arc<ValueMap>) -> Interpreter`: the deployment's `[public]` values, which `Expr::Config` reads.
* `config(&self) -> &Arc<ValueMap>`
* `with_frameworks(self, frameworks: Frameworks) -> Interpreter`: the vendored frameworks. With none set, every component is written as plain markup.
* `frameworks(&self) -> Frameworks`
* `with_island_reads(self, reads: BTreeMap<String, Vec<String>>) -> Interpreter`: the plan's `reads`. An island the renderer has no body for is held while one of its keys is pending. It then carries `$aw` with those keys.
* `with_derives(self, derives: Vec<Derive>) -> Interpreter`: the plan's derived keys, each after the derived keys it reads.

Running a body:

* `run(&self, body: &Body, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`: `input` is `None` for a loader. `Expr::Input` then reads `Value::Null`.
* `run` writes session changes to a draft copied from `ctx.session` at entry. They are committed to the cell, key by key, only on success. A `SessionExtend` is applied to the cell after the draft, on success only.
* `run_with(&self, body: &Body, prepared: Prepared<'_>, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`: `run` with `body` run as `prepared` has it.
* `run_prepared(&self, body: &Body, program: Option<&Program>, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`: `run_with` with `Prepared::Compiled(program)`. It uses `Prepared::No` when `program` is `None`.
* `evaluate(&self, expr: &Expr, scope: Vec<(String, Value)>) -> impl Future<Output = Result<Value, Fail>>`: one expression over `scope`, with no request.
* `evaluate_program(&self, program: &Program) -> Result<Value, Fail>`: a prepared body run with no request.
* `evaluate_native(&self, native: NativeBody) -> Result<Value, Fail>`: a compiled body run with no request.
* `apply(&self, lambda: &Expr, args: Vec<Value>) -> impl Future<Output = Result<Value, Fail>>`: a lambda applied to `args` with no request.
* `compute(&self, ctx: &RequestCtx, body: &Expr, store: &ValueMap) -> Option<Value>`: `body` evaluated under `ctx` with `store` as the render's store. `None` when it fails.

Rendering, each synchronous:

* `render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: renders `component` with `props` bound as `$props`. It is `render_module` with an empty module.
* `render` lifts a `$store` prop and a `locale` prop into the environment, where a nested component's `Expr::Store` and `Expr::Locale` reach them.
* `render_module(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: `render` for the component under `module`, which keys its hoisted values.
* `render_island(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: `render_module` for an island placed with no children. A `Slot` it renders writes an empty `render::CHILDREN_OPEN` region where a root component writes `render::slot_mark`.
* `island_step(&self, module: &str, component: &Component, props: &ValueMap, state: &ValueMap, handler: Option<HandlerRef>, event: &Value, library: &Components) -> Result<Stepped, Fail>`: one round trip of an island in server mode.
* `island_step` runs the body's `let`s with `state` standing in for the state bindings. It then runs the handler with `$props`, `$state` and `$event` bound and merges the object it returns into the state.
* `island_step` writes a key that is a store binding to the render's store and answers it in `Stepped.store`. It then renders from the new state in server mode.
* `island_step` with `handler` set to `None` renders without running a handler.
* `island_step` recomputes each derived key that has a body after the handler's writes and before the render, except a key the handler wrote itself. A derived key is never answered in `Stepped.store`.
* A state binding of a component rendered inside the island at address `<path>` is the entry `<path>/<name>` of `state`.
* `island_step` is `NotFound` when the `HandlerRef` names a path the render does not reach or an index the component there lacks. A missing handler index on the island's own component is `Internal`.
* The answered `state` is what the render consumed. A component the render no longer places leaves no entry.

Rendering rules:

* The markup is what the server renderer of the framework hydrating each component writes. See [Markup rules](#markup-rules).
* A `Tmpl::Component` or `Tmpl::Island` is a call into `library` by module id, so a component may render itself. Nesting deeper than `render::MAX_CALLS` (512) fails with an internal `Fail` naming the module.
* A `Tmpl::Island` renders its component apart and leaves `ISLAND_MARK`, its index in `islands` and a NUL in `html`.
* A root `Slot` with no caller leaves `render::slot_mark(name)`: `SLOT_MARK`, the slot's name and a NUL.
* Outside an island's body and outside a component that hydrates, a `Tmpl::Component` whose component hydrates renders as an island with no timing.
* A `Tmpl::Component` naming a module `library` does not hold renders as an island whose body is empty.
* Inside an island, a module `library` does not hold writes nothing. `whole` is then false and the island holding it is written with an empty body.
* An island's children render inside `render::CHILDREN_OPEN`, closed after them, wherever its component places its `children`. Nothing they hoist is kept.
* A `React` or `Vue` island whose render did not place its children writes them after its markup inside `render::CHILDREN_HELD_OPEN`, closed as a template. They render in the caller's scope.
* A body `let` named `RERUN_PREFIX` and a state binding that holds `[value]` runs the body's `let`s again with that binding at `value`. The tree renders once, from the last pass.
* A body still asking for another pass after `RERUN_LIMIT` (25) passes fails with `Too many re-renders. React limits the number of renders to prevent an infinite loop.`
* An element whose attributes carry `CHUNK_ATTR` with an integer id renders its children into a buffer of their own and records that markup as a string under the id.
* An element also carrying `UNTAKEN_ATTR` is rendered into the table alone when the `If` branch holding it is not taken.
* An island held for a pending store key records every chunk of its component from its props alone.
* An element carrying `RAW_ATTR` writes that string unescaped and renders no children. A null writes nothing.
* An attribute whose name starts with `$` is never printed.
* An element carrying `SHADOW_ATTR` holding a module id writes that component after its open tag under plain markup, with the element's other attributes as its props.
* The `SHADOW_ATTR` component's `shadow` gives the wrapping `<template>`: its `shadowrootmode` and each option that is true as a bare attribute. With no `shadow` the wrapper is `<template shadowrootmode="open">`.
* Nothing rendered inside a shadow root is recorded among an island's hoisted values. A non-scalar attribute on such an element reaches the template and is not written on the host element.

Hoisted values:

* `Rendered.hoisted` holds every `Expr::Hoist` value the markup took, keyed `<module>|<id>`. Under `For` iterations the key is `<module>|<id>@<i>.<j>`, the callers' loops first.
* A `Tmpl::Component` with `keyed` set adds `c<id>` to the path of what the component renders. The caller's children render under the caller's module and path.
* A key recorded twice with different values is removed.
* An island starts its own table, `RenderedIsland.body.hoisted`.

Server-mode islands:

* `$on:` markers print as `data-sf-on="click:0 change:1"` and `$key` as `data-sf-key`. Neither prints in a browser-mode render.
* A marker of a component inside the island prints as `<address>/<index>`, the address of the component that owns the handler.
* A server-mode island's `RenderedIsland` carries its mode, the values its state `let`s took, the store values it was rendered from and the store keys it reads, `render::store_keys(component, library)`.
* `rendered_nodes` writes `data-sf-mode="server"` on a server-mode island's region.
* A server-mode island that reads a derived key also asks the browser for that key's sources.

Held islands:

* An island whose `store_keys` include one of the pending keys its props carry under `snapfire_fsr_runtime::PENDING_PROP` is held. Its component is not rendered and its children are.
* `RenderedIsland::awaits` lists those keys.
* `IrEvaluator` holds a root a framework hydrates the same way: a `Node::Client` with its slot regions and `$aw`.

### Outcome

What `run` answers. `#[non_exhaustive]`.

* `pub struct Outcome { pub value: Value, pub written: Vec<String>, pub extended: Option<u64>, pub redirected: Option<String> }`
* `value`: `Value::Null` when the body ends without a `return`.
* `written`: every session key set or deleted, in first-touch order, already committed.
* `extended`: the seconds the last `SessionExtend` named, already applied through `SessionCell::extend`. `None` when the body ran none.
* `redirected`: the `Location` of the redirect response the body returned, from `RedirectTo` or `ResponseRedirect`. `None` for any other value.

### Clock

What `Expr::Now` reads.

* `pub trait Clock: Send + Sync { fn now(&self) -> i128; }`: milliseconds since the Unix epoch.

### Derive

A store key the browser's `derive` computes. Root re-export.

* `pub struct Derive { pub key: String, pub sources: Vec<String>, pub body: Option<Expr> }`. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.
* `body` reads each source as `Expr::Store`. `None` for a derive that was not translated.
* `new(key: String, sources: Vec<String>, body: Option<Expr>) -> Self`

### Prepared

How `Interpreter::run_with` runs a body. Root re-export.

* `pub enum Prepared<'a> { No, Compiled(&'a compiled::Program), Native(native::NativeBody) }`. `Clone`, `Copy`.
* `No` interprets the body as written. `Compiled` and `Native` answer exactly what `No` answers, failures included.
* The top-level guard scan in [Guards](#guards) runs before any of them.

### Program

`snapfire_fsr_ir::compiled::Program`: a body with every local at a fixed position, built once and run for every request.

* `pub fn of(body: &Body) -> Option<Program>`
* `of` is `None` for a body holding, anywhere, a `Call`, a `Next`, an `ApplyOf` or a `NativeCall` whose `sync` is false.
* `of` is `None` for a body holding a `SessionSet`, `SessionDelete`, `SessionExtend` or `Act` statement.
* `of` accepts a `Guard` with a known kind at the body's own level and inside its `If`, `ForOf` and `While`. A guard inside a `Try` or with an unknown kind makes it `None`.
* An expression the program has no step for runs interpreted in place.
* `pub type compiled::Shared = Arc<Program>`

### native

`snapfire_fsr_ir::native`: Rust source for a body, compiled into the application by a production build.

* `pub type NativeBody = fn(&mut Frame<'_>) -> Result<Value, Fail>`
* `pub struct Native { pub source: &'static str, pub plan: u64, pub body: NativeBody }`: one entry of the `BODIES` list a build writes. `source` is the row it was written for and `plan` the `plan_hash` it was written from.
* `plan_hash(body: &Body, consts: Option<&Consts>) -> u64`: xxh3 over the body as plan text and each constant it reaches, directly or through another constant.
* `plan_hash_with(body: &Body, consts: Option<&ConstHashes<'_>>) -> u64`: `plan_hash` against a table every body of one plan can share. It gives the value `plan_hash` gives.
* `pub struct ConstHashes<'a>`: a plan's constants, each digested the first time a body reaches it. `ConstHashes::new(consts: &'a Consts) -> Self`. Not `Sync`.
* `emit(name: &str, body: &Body, consts: Option<&Consts>) -> Option<String>`: the source of `pub fn <name>(f: &mut <root>::Frame<'_>) -> Result<<root>::Value, <root>::Fail>`, with `<root>` the path `snapfire_fsr_ir::native`.
* `emit_or_why(name: &str, body: &Body, consts: Option<&Consts>) -> Result<String, String>`: the same. The `Err` names the first statement or expression not written.
* `emit_in(root: &str, name: &str, body: &Body, consts: Option<&Consts>) -> Result<String, String>`: the same, written against a re-export of this module at `root`, such as `snapfire_fsr_host::bodies`.
* A body gets no source when `Program::of` refuses it or when it holds an expression the generator does not write.
* The module re-exports `Value`, `Fail`, `Builtin`, `Read`, `MAX_PASSES`, `ArithOp` as `Arith` and `CompareOp` as `Compare`.

### Read

A request read, as `Frame::request` takes it.

* `pub enum Read<'a> { Param(&'a str), Query(&'a str), ParamsAll, QueryAll, Session(&'a str), Locale, Locales, Path, Document, Host, Origin, Address, Config(&'a str), Input, Now }`. `Debug`, `Clone`, `Copy`.

### Frame

`snapfire_fsr_ir::native::Frame`: what generated code calls. Each method applies the interpreter's own rule.

* `request(&self, read: Read<'_>) -> Value`: the same answer as the expression in [Reads](#reads).
* `native(&self, module: &str, method: &str, args: Vec<(&str, Value)>) -> Result<Value, Fail>`: a `NativeCall` whose `sync` is true.
* `guard(&self, kind: &str, message: &Value) -> Fail`: the failure for a guard that holds.
* `constant(&mut self, key: &str) -> Result<Value, Fail>`
* `field`, `index`, `arith`, `float`, `as_number`, `compare`, `truthy`, `text_part`, `text`, `number`, `bigint`, `string`: the value rules.
* `fields`, `spread`, `computed`, `spread_items`, `seq`, `range_len`, `range_len_of`: building objects and arrays.
* `write_fields`, `write_field`, `write_key`, `write_computed`, `write_at`, `push`, `push_one`: writes into objects and arrays.
* `write_whole(&mut self, target: &Value, next: Value) -> Result<Value, Fail>`: an `Overwrite` that replaces an object's or an array's contents in place.
* `order(answer: &Value) -> f64`: the order `sort` reads from a comparator's answer.
* `default_order(a: &Value, b: &Value) -> Result<f64, Fail>`: the order `sort` takes without a comparator.
* `builtin(&mut self, name: Builtin, values: Vec<Value>) -> Result<Value, Fail>`, `call(&mut self, key: &str, args: Vec<Value>) -> Result<Value, Fail>`, `heap_on(&mut self)`.
* `length`, `key_count`, `keys`, `values`, `past_passes`, `not_an_array`.

## 5. Rendering

The module `snapfire_fsr_ir::render`.

### Components

* `pub type Components = HashMap<String, Arc<Component>>`: translated components by module id.

### bake_static_markup

* `pub fn bake_static_markup(component: &mut Component)`: the render with its literal markup as `Tmpl::Marker` nodes. It renders byte for byte as before under every set of markup rules.
* An element is prebuilt when its attributes are literal and every set of markup rules writes its open tag alike. Text is prebuilt when every set of rules escapes it alike and it sits between two prebuilt pieces.
* Left as nodes: a component's root element, the children a component or an island is given, `svg`, `foreignObject`, `noscript`, a tag `baked_open` refuses and every element `ast::read_by_host` answers.

### Rendered

What a render answers. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Rendered { pub html: String, pub islands: Vec<RenderedIsland>, pub places: Vec<RenderedPlace>, pub hoisted: ValueMap, pub whole: bool }`
* `html`: the markup, with a mark and a NUL where each island, root slot, place and outlet sits.
* `hoisted`: the values the markup's hoisted expressions took, keyed as `Hoists::key` keys them.
* `whole`: false when the component's own tree reached a component that was not translated.

### RenderedIsland

A component rendered as an island. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct RenderedIsland { pub module: String, pub props: ValueMap, pub when: Option<String>, pub mode: Option<String>, pub state: ValueMap, pub reads: Vec<String>, pub rendered_store: ValueMap, pub awaits: Vec<String>, pub key: String, pub body: Rendered, pub rendered: bool }`
* `mode`: `Some("server")` for an island whose events round-trip to the server.
* `state`: the values the component's state `let`s took.
* `reads`: the store keys a server-mode island reads.
* `rendered_store`: the store values the island's markup was rendered from.
* `awaits`: the pending store keys the island reads, when the server held it.
* `key`: the region this placement owns, keyed as `Hoists::key` keys a hoist.
* `rendered`: false for a module the server has no body for, one held for a pending store key and one left to the browser.
* `mount_props(&self) -> ValueMap`: the props the browser mounts the island with.
* `mount_props` drops every handler reference and the `$t` key. A prop that `$t` names is wrapped in a `Value::Variant` of its tag. A `$map` carries the object's `[key, value]` pairs and a null prop stays null.
* `mount_props` adds `$h` when anything was hoisted, `$k` when `key` is set and `$aw` when `awaits` is not empty.
* In server mode `mount_props` adds `$s`. When `reads` is not empty it also adds `$sk` and `$sv`.
* In browser mode `mount_props` adds `$sv` when `rendered_store` is not empty.

### RenderedPlace

* `pub struct RenderedPlace { pub target: String, pub body: Rendered }`: a `Tmpl::Place`'s content as rendered. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

### Stepped

What `island_step` answers. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`.

* `pub struct Stepped { pub state: ValueMap, pub store: ValueMap, pub rendered: Rendered, pub acts: Vec<(String, Value)>, pub components: Vec<(String, String)>, pub emitted: Vec<Emitted>, pub cells: ValueMap }`
* `store`: the store keys the handler wrote, with their values.
* `acts`: every `Act` the handler ran, in order, as `(action id, input)`. Each input is evaluated where its statement stood. The interpreter dispatches nothing itself.
* `components`: each component the render entered, by its `HandlerRef` path and its module, root first.
* `emitted`: each value the render and the handler handed the host with `Builtin::Emit`, in order.
* `cells`: each module cell's value as the step left it, by key.

### Emitted

* `pub struct Emitted { pub path: String, pub kind: String, pub value: Value }`: a value a body handed its host, with the path of the component that emitted it. `Debug`, `Clone`, `PartialEq`.

### HandlerRef

The handler a step runs. Root re-export. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

* `pub struct HandlerRef { pub path: String, pub index: usize }`: `index` into the handlers of the component at `path`.
* `path` is empty for the island's own component. Otherwise it is the address of a component rendered inside it, `c1` for a keyed placement or `0.c2` inside a loop, as `Hoists::path_key` spells it.
* `own(index: usize) -> Self`
* `at(path: impl Into<String>, index: usize) -> Self`
* `parse(token: &str) -> Option<Self>`: reads a token as an element binds it, `2` or `c1/2`.

### Hoists

`snapfire_fsr_ir::interp::Hoists`: the recorder of one island's hoisted values. `Debug`, `Default`, `Clone`.

* `pub struct Hoists { pub module: String, pub path: Vec<Step>, pub table: ValueMap, .. }`
* `new(module: impl Into<String>) -> Self`
* `key(&self, id: u32) -> String`: `module|id`, with `@` and the path when there is one.
* `island_key(&self, id: u32) -> String`: the region key of an island placement, marked `i`.
* `id(&self, id: u32) -> String`: the `useId` string, `ID_OPEN`, the hoist key and `ID_CLOSE`.
* `path_key(&self) -> String`: the steps joined by `.`, `c<id>` for a keyed placement. Empty at the island's own component.
* `record(&mut self, id: u32, value: Value)`: records a value. A key recorded twice with different values is removed.

### Step

* `pub enum Step { Iteration(usize), Placement(u32) }`: one step of a key's path. `Iteration` prints as the number and `Placement` as `c` and the placement id. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

### rendered_nodes

* `pub fn rendered_nodes(rendered: &Rendered) -> Vec<Node>`: the nodes a rendered component's markup makes. Root re-export.
* Markup between marks becomes `Node::raw` pieces.
* A `SLOT_MARK` becomes `Node::Slot` with the slot's name.
* An `OUTLET_MARK` becomes `Node::Outlet` with the place's name.
* A `PLACE_MARK` becomes a `Node::Placed` holding the place's own nodes.
* An island becomes `Node::raw("<sf-s data-sf-island …>")`, a `Node::Client` and `Node::raw("</sf-s>")`.
* The island's opening tag carries `data-sf-region` when it has a key, `data-sf-when` when it has a timing and `data-sf-mode` when it has a mode.
* The island's `Node::Client` takes `mount_props()` as its props and the island's body as its child. Its `rendered` is the island's `rendered`.

### Render functions

* `prepare(component: &Component) -> Component`: every element whose open tag is entirely literal becomes a `Tmpl::Baked`. It never bakes a `<title>`, `<meta>` or `<link>`. It never bakes a tag the sets of markup rules print differently.
* `store_keys(component: &Component, library: &Components) -> Vec<String>`: every `Expr::Store` in the component and in the components it renders inline, sorted.
* `rendered_store(store: &ValueMap, keys: &[String]) -> ValueMap`: the entries of `store` among `keys`.
* `inline_styles(component: &Component) -> Vec<Option<String>>`: the content of every `<style>` the component renders, in the order written, as the server writes it under the component's markup. `None` for one whose content the render computes.
* `css_name(key: &str) -> String`: a style property's CSS spelling. `fontSize` is `font-size` and `WebkitLineClamp` is `-webkit-line-clamp`. A custom property keeps its name.
* `html_attr_name(name: &str) -> &str`: React's attribute spelling as HTML's. An HTML spelling passes through.
* `slot_mark(name: &str) -> String`: `SLOT_MARK`, `name` and a NUL.
* `handler_value(path: &str, index: u32) -> Value`: a `Value::Variant` tagged `HANDLER_VALUE` whose payload is the token `<index>` or `<path>/<index>`.
* `handler_token(value: &Value) -> Option<&str>`: a handler reference's token. `None` for any other value.
* `handler_ref(token: &str) -> Option<HandlerRef>`: the handler a token names. `None` for `BROWSER_HANDLER`.
* `pub use snapfire_fsr_core::value::typed`: the tags `$t` names.

### Markup marks

Each mark is followed by its payload and a NUL in `Rendered::html`.

* `ISLAND_MARK = "\u{0}sf-island:"`: an island, followed by its index in `islands`.
* `PLACE_MARK = "\u{0}sf-place:"`: a `Tmpl::Place`, followed by its index in `places`.
* `OUTLET_MARK = "\u{0}sf-outlet:"`: a `Tmpl::Outlet`, followed by the place's name.
* `SLOT_MARK = "\u{0}sf-slot:"`: a root component's own `Slot`, followed by the slot's name.
* `CHILDREN_OPEN = "<sf-s data-sf-children>"`: the region an island's children render in.
* `CHILDREN_HELD_OPEN = "<template data-sf-children>"`: where a React or Vue island's children go when its render did not place them.
* `svelte::ROOT_OPEN = "<!--[-->"` and `svelte::ROOT_CLOSE = "<!--]-->"`: written around a Svelte component entered from markup that is not Svelte's.

### Prop keys

* `HOISTED_PROP = "$h"`: an island's hoisted values.
* `STATE_PROP = "$s"`: a server-mode island's initial state.
* `KEY_PROP = "$k"`: an island's region key.
* `READS_PROP = "$sk"`: a server-mode island's store keys.
* `RENDERED_STORE_PROP = "$sv"`: the store values an island was rendered from.
* `AWAITS_PROP = "$aw"`: a held island's pending store keys.
* `TYPES_PROP = "$t"`: an object of prop name to a tag in `typed`. The browser mounts each named prop as the real `Map`, `Set`, `Date` or `BigInt`.
* `FUNCTIONS_PROP = "$f"`: an object of prop name to the module id `file#export` the browser imports that function from. Each named prop mounts as a `Value::Ref` of `RefKind::Module`, while the server's render reads the function value.
* A function value left among an island's mounted props is an internal `Fail`.
* `CELLS_PROP = "$cells"`: the module cells' values a host passes a step, as the last step left them.
* `MARKUP_PROP = "$sfm"`: the region number a marker in an island's props carries in place of a markup value.
* `PARENT_SCOPE_PROP = "$scope"`: the scoped-style attribute of the Vue parent that placed a child.
* `FALLTHROUGH_PROP = "$attrs"`: the attributes a Vue parent gave a child beyond its declared props.
* `CHILD_COUNT = "$childCount"`: `Children.count(children)`, empty nodes included.
* `CHILD_ITEMS = "$childItems"`: `Children.toArray(children).length`.
* `CHILD_VALUES = "$childValues"`: `Children.toArray(children)`, each child a markup value.
* A component whose body opens with a `let` named `CHILD_COUNT`, `CHILD_ITEMS` or `CHILD_VALUES` is passed that value as the prop of the same name, counted in the caller's scope.
* `null`, `undefined` and booleans count one each for `CHILD_COUNT` and none for `CHILD_ITEMS`. An array counts its items. Any other fragment counts one. A `Tmpl::Slot` counts what the enclosing placement passed.

### Attribute and tag names

* `HANDLER_ATTR = "$on:"`: the prefix of an attribute binding a handler by index, `$on:click`. Printed as `data-sf-on="click:0"` in server mode only.
* `KEY_ATTR = "$key"`: an element's React `key`. Printed as `data-sf-key` in server mode only.
* `UNLOWERED_ATTR = "$unlowered"`: on an element whose handler was not translated, holding the line and the reason. A server-mode island holding one is refused.
* `RAW_ATTR = "$html"`: an element's `dangerouslySetInnerHTML`, holding the `__html` expression.
* `SHADOW_ATTR = "$shadow"`: on a placement of a custom element whose template sits under `elements/`, holding the template's module.
* `CHUNK_ATTR = "$chunk"`: marks an element whose inner markup is recorded as a hoisted chunk, with its id.
* `UNTAKEN_ATTR = "$untaken"`: beside `CHUNK_ATTR` on a chunk under a branch on state.
* `SVELTE_FLAGS = "$svelte"`: Svelte's `$.attributes` flags for an element whose attributes come from a spread. 1 for SVG or MathML, 2 to keep the names' case, 4 for an `<input>` and 8 for `<svelte:element>`.
* `SVELTE_SELECTED = "$selected"`: on a Svelte `<option>`, whether `selected=""` follows its attributes.
* `MARKUP_ATTR = "data-sf-m"`: the attribute of the regions an island's markup props render in.
* `BOUNDARY_TAG = "$boundary"`: a React boundary. Its `kind` attribute is `$` for a `<Suspense>` whose children rendered and `&` for a visible `<Activity>`. Its children are written inside `<!--K-->` and `<!--/K-->`.
* `CHOSEN_TAG = "$tag"`: an element whose tag a value chose at render. Its first attribute is the element name or a component reference.
* `SUSPENSE_TAG = "$suspense"`: a terminal page's Suspense boundary, holding the content and the fallback in that order. A thrown `DEFERRED` renders the fallback and the step emits `SUSPENDED`.
* `HANDLER_VALUE = "$handler"`: the variant tag of a handler reference.
* `BROWSER_HANDLER: u32 = u32::MAX`: the handler index of a function only the browser holds. It renders and tests as a function and no server step can run it.
* `COMPONENT_VALUE = "$component"`: the tag of a component reference, whose payload is the module.
* `MARKUP_VALUE = "$markup"`: the tag of a markup value.
* `DEFERRED = "$deferred"`: the key a terminal page's promise is held under, `{ "$deferred": id }`.
* `WRITE_BACK = "$writeBack"`: the key of what `Builtin::WriteBack` answers.
* `SUSPENDED = "suspended"`: the kind a step emits for each deferred a Suspense boundary waits on.
* `ACT = "act"`: the kind a block in a step emits for an act it issues, `{ action, input }`.
* `SERVER_MODE = "server"`: the island mode whose events round-trip to the server.
* `FUNCTION_FORM_ACTION`: what React 19 writes as the `action` or `formAction` of a form whose action is a function.
* `interp::FUNCTION_VALUE = "$fn"`: the tag of a function value.
* `interp::CELL_VALUE = "$cell"`: the tag of a cell's handle.

### Slot and context names

* `SLOT_CONTENT_PREFIX = "$slot:"`: a `Tmpl::Let` among a placement's children named with this prefix and a slot is the caller's content for that slot. Its value is the condition it is given under.
* `SLOT_OUT_PREFIX = "$slotOut:"`: a `Tmpl::Let` named with this prefix and a slot is the child's `<slot>`. Its value is the props it hands the caller's content and its body the fallback.
* `SLOT_READ_PREFIX = "$slotRead:"`: a `{props.name}` read that takes the placement's `$slot:` fill for the prop before its value. It writes no fragment anchors.
* `SLOT_PROPS = "$slotProps"`: the name the caller's content reads a scoped slot's props under.
* `SLOTS_GIVEN = "$given"`: bound beside `$props` in every component's scope. An object naming each slot the placement's children fill, `default` as `true` for children that are no wrapper.
* `CONTEXT_PREFIX = "$ctx:"`: a `Tmpl::Let` named with this prefix and a context id is a provider. Nested components see it. An island does not.
* `ARRAY_ITEMS = "$items"`: a `Tmpl::Let` of this name around a `Tmpl::Fragment` marks an array's items, which render as the fragment does.
* `RERUN_PREFIX = "$rerun:"`: a body `let` holding a state binding's next value, `[value]` to render again and null not to.
* `HOLD_PREFIX = "$hold:"`: a body `let` holding a state binding's value from there on.
* An island's children fill its default slot and, under Vue's markup, every slot. Svelte's `{@render snippet(args)}` and `<slot>` use the same pair, the arguments as an array.

### Limits

* `MAX_CALLS: usize = 512`: how deep components may nest in one render.
* `RERUN_LIMIT: usize = 25`: how many passes a body may take.
* `interp::MAX_APPLIES: usize = 256`: how deep function values may nest their calls.
* `interp::MAX_PASSES: usize = 10_000_000`: how many passes one `Stmt::While` may run.
* `interp::CONSOLE_TARGET = "fsr::console"`: the `tracing` target of `Builtin::Log`.
* `interp::number_key(n: f64) -> String`: the string JavaScript makes of a number used as a key.

## 6. Frameworks

### Frameworks

The frameworks an application vendors, each at the major whose server markup the renderer writes. Root re-export.

* `pub struct Frameworks { pub react: Option<ReactMajor>, pub vue: Option<VueMajor>, pub svelte: Option<SvelteMajor> }`. `#[non_exhaustive]`. `Default` vendors nothing.
* `react(self, react: Option<ReactMajor>) -> Self`
* `vue(self, vue: Option<VueMajor>) -> Self`
* `svelte(self, svelte: Option<SvelteMajor>) -> Self`

### ReactMajor

* `pub enum ReactMajor { V18, V19 }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`. `Default` is `V18`.
* `of(version: &str) -> Option<ReactMajor>`: the major of a version such as `18.3.1` or `19.3.1`. `None` for any other major.
* `number(self) -> u32`
* `checked(self) -> (&'static str, &'static str)`: the earliest and latest releases of the major the markup was checked against.
* `ALL: [ReactMajor; 2]`, oldest first.

### VueMajor

* `pub enum VueMajor { V3 }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`. `Default` is `V3`.
* `of(version: &str) -> Option<VueMajor>`: the major of a version such as `3.5.44`.
* `number(self) -> u32`
* `checked(self) -> (&'static str, &'static str)`
* `ALL: [VueMajor; 1]`

### SvelteMajor

* `pub enum SvelteMajor { V5 }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`. `Default` is `V5`.
* `of(version: &str) -> Option<SvelteMajor>`: the major of a version such as `5.57.2`.
* `number(self) -> u32`
* `checked(self) -> (&'static str, &'static str)`
* `ALL: [SvelteMajor; 1]`

### Markup rules

* A component a vendored framework hydrates renders under that framework's rules at the vendored major.
* A component hydrated by a framework the application does not vendor renders as plain markup.
* An `Fsr` component keeps its caller's rules. At the top of a render those are plain markup.

| Written | React 18.3 | React 19 | Plain |
| --- | --- | --- | --- |
| adjacent text runs | `<!-- -->` between them | `<!-- -->` between them | joined |
| a void element | `<br/>` | `<br/>` | `<br>` |
| custom element, array or object | `'' + value`: `"1,2"`, `"[object Object]"` | omitted | `Internal`, naming the element and the attribute |
| custom element, `true` | `attr="true"` | `attr=""` | `attr="true"` |
| custom element, `false` | `attr="false"` | omitted | omitted |
| `inert={true}` | omitted | `inert=""` | `inert="true"` |
| empty `src`, empty `href` off `<a>` | written | omitted | written |
| a head tag React 19 hoists, outside `<svg>` and `<noscript>` | in place | `Internal`, naming the loader's `meta` export | in place |

* A head tag React 19 hoists is a `<title>` or a `<meta>` without `itemProp`. A `<link>` is hoisted when it has a `rel`, a non-empty `href` and no load or error handler. A stylesheet `<link>` also needs a `precedence` and no `disabled`.
* `className` on a custom element is written `class` under every set of rules.

Vue's rules, checked against `@vue/server-renderer` 3.5:

* Text and attributes escape `"`, `&`, `'`, `<` and `>`. Adjacent text runs are joined with no separator comment.
* An interpolation is `toDisplayString`: a boolean is its word, `null` nothing and an array or an object its two-space JSON.
* A void element closes with `>`. Attribute names are written as given.
* `class` is always written, normalized from a string, an array or an object of truthy keys.
* `style` is always written. A string is written as it stands. An object or an array is written as `name:value;` pairs with camelCase hyphenated and a number bare.
* A boolean attribute is bare when truthy or the empty string.
* `true` on any other attribute is bare, a string or a number is valued and anything else is absent.
* The anchors Vue writes around a multi-root component, a list, a branch and a slot are `Tmpl::Marker` nodes. Nothing hoists into the head.

Svelte's rules:

* Text escapes `&` and `<` and adjacent runs join. An attribute escapes `&`, `"` and `<`.
* `null` omits an attribute. So does a falsy boolean attribute other than `""`, from Svelte's `is_boolean_attribute` list.
* `hidden` is boolean unless it is `until-found`. `translate` writes `yes` and `no`. An empty `class` or `style` is omitted.
* A value prints as `String(value)`. A void element closes with `/>`.
* Entering a Svelte component from markup that is not Svelte's writes `<!--[-->` before it and `<!--]-->` after it. Every other marker is a `Tmpl::Marker`.

## 7. Extensions

### Extensions

The registry an `Expr::Ext` is answered from, by `module.member`. Root re-export. `Clone`. `Default` is empty. `Debug` lists the names with their reach.

* `empty() -> Extensions`: no members, the standard library included.
* `standard() -> Extensions`: the standard library.
* `register<F>(&mut self, name: impl Into<String>, reach: Reach, f: F)` where `F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static`: replaces what the name held.
* `get(&self, name: &str) -> Option<&Extension>`
* `contains(&self, name: &str) -> bool`
* `names(&self) -> Vec<String>`: sorted.
* `call(&self, name: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail>`: `Internal` naming the name when nothing holds it.
* `pub type ext::ExtFn = dyn Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync`
* `std::register(extensions: &mut Extensions)`: adds every standard member. Each `std::<module>::register` adds one module's.

### Extension

* `pub struct Extension { pub reach: Reach, .. }`. `Clone`, `#[non_exhaustive]`.
* `call(&self, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail>`

### Member

A standard member. Root re-export.

* `pub struct Member { pub module: &'static str, pub name: &'static str, pub reach: Reach, pub signatures: &'static [Signature], pub platform: Option<&'static str> }`. `Debug`, `Clone`, `Copy`.
* `signatures`: set for a member with no TypeScript in the client to declare it.
* `platform`: the platform code the translator writes this member for, such as `new Intl.RelativeTimeFormat(locale, options).format(value, unit)`. Application code writes the platform form and imports no such member.
* `pub const STANDARD: &[Member]`: every standard member.
* `pub fn standard_reach(module: &str, name: &str) -> Option<Reach>`: a standard member's reach.
* `Signature`, `Field` and `Ty` are re-exported from `snapfire_fsr_core::ext`.

### Reach

Where a call may run. Re-exported from `snapfire_fsr_core::ext`.

* `pub enum Reach { Render, Body, Terminal, Later, Process }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.
* `Render`: on the server and in the browser. It must give the same answer in both.
* `Body`: on the server only, in a loader, an action, a handler or middleware.
* `Terminal`: in a terminal page, where a render may read the clock.
* `Later`: in a program that keeps running after a handler returns.
* `Process`: in a program that starts processes, reads its environment and owns the screen.
* A terminal page may use all five. A web page may use `Render` and `Body` only.
* `as_str(&self) -> &'static str`: `render`, `body`, `terminal`, `later` or `process`.
* `needs(&self) -> Option<&'static str>`: why a web page cannot use the reach, finishing "`x`, which ...". `None` for `Render` and `Body`.

### Ambient

What a call runs under. Re-exported from `snapfire_fsr_core::ext`.

* `pub struct Ambient { pub locale: String, pub now: i128, pub catalogs: Option<Arc<Catalogs>>, pub files: Option<Arc<FileRoots>> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`, `#[non_exhaustive]`.
* `locale`: the request's locale as the application spells it, empty when none is set.
* `files`: where `file` resolves in a web host. `None` in a terminal program.
* `locale(self, locale: String) -> Self`, `now(self, now: i128) -> Self`, `catalogs(self, catalogs: Option<Arc<Catalogs>>) -> Self`, `files(self, files: Option<Arc<FileRoots>>) -> Self`
* `bcp47(&self) -> String`: `fr-FR` for `fr_FR` and `en` when empty.

### Catalogs

Message tables by locale. Re-exported from `snapfire_fsr_core::ext` with `Table` as `snapfire_fsr_ir::catalog`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`.

* `pub type Table = BTreeMap<String, String>`
* `from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Catalogs`: every locale's table merged over the `default` locale's.
* `is_empty(&self) -> bool`
* `default_tag(&self) -> &str`
* `rows(&self) -> Vec<(String, usize)>`: each locale with how many keys its own table held.
* `table(&self, tag: &str) -> Option<&Arc<Table>>`: the merged table for `tag`. The default locale's when `tag` has none.
* `json(&self, tag: &str) -> Option<Arc<str>>`: `table` as JSON.
* `lookup(&self, tag: &str, key: &str) -> Option<&str>`

### Argument readers

Re-exported in `snapfire_fsr_ir::ext` from `snapfire_fsr_core::ext`. Each takes the extension's name for its error and is `Internal` naming it on a wrong type.

* `number`: an argument by index as `f64`, from `Int`, `UInt`, `F32` or `F64`.
* `text`: an argument by index as a string.
* `text_opt`: an optional string argument.
* `option`: a field of an optional options object.

### The standard library

Every member takes its arguments positionally and answers a `Value`. Numbers arrive as `F64`, `Int` or `UInt`. Every date is UTC and every instant is milliseconds since the epoch.

Every `locale` argument is a BCP 47 tag. Without one the ambient locale formats. The locale is `Ambient::bcp47`. A tag ICU4X cannot parse falls back to `en`.

A `file` path resolves against `Ambient.files`, confined to its roots. Without one it resolves against the working directory.

| Member | Reach | Arguments | Answer |
| --- | --- | --- | --- |
| `intl.number` | render | `n`, `{ style?, minimumFractionDigits?, maximumFractionDigits?, minimumIntegerDigits?, useGrouping?, signDisplay?, locale? }?` | what `Intl.NumberFormat` writes: grouped for the locale, half away from zero to at most three fraction digits by default (none for `style: "percent"`, which multiplies by 100 and writes the locale's percent pattern), trailing zeros dropped past the minimum; `useGrouping` `true`, `false`, `auto`, `always` or `min2`; `signDisplay` `auto`, `always`, `exceptZero`, `negative` or `never`; `NaN`, `∞` and `-∞` as JavaScript prints them |
| `intl.currency` | render | `n`, `code`, `{ display?, minimumFractionDigits?, maximumFractionDigits?, locale? }?` | the amount with the ISO code and the currency's own fraction digits, `USD 1,234.50`, `1.234,50 EUR`; `display` `symbol` or `narrowSymbol` writes the symbol CLDR gives the locale, `$1,234.50`, `1.234,50 €`; one fraction bound given moves the other only as far as it must; a code that is not three letters is `Internal` |
| `intl.date` | render | `when` (milliseconds or an ISO 8601 string), `style?` (`short`, `medium` by default, `long`, `full`) or `{ style?, timeStyle?, weekday?, year?, month?, day?, locale? }` | the calendar date in UTC at that `dateStyle` or at the parts: a text `month` or `weekday` (`long`, `short`, `narrow`) sets the width and `year` and `day` are `numeric`; a numeric month is `Internal`. `timeStyle` `short` or `medium` writes the time of day in UTC, after the date when `style` is given, with an ordinary space before the day period |
| `intl.plural` | render | `n`, `{ type?, locale? }?` | the category `Intl.PluralRules` selects, `zero`, `one`, `two`, `few`, `many` or `other`, cardinal unless `type` is `ordinal` |
| `intl.relative` | render | `value`, `unit`, `{ numeric?, style?, locale? }?` | the phrase `Intl.RelativeTimeFormat` writes: `unit` `second` to `year`, singular or plural; `numeric` `always` or `auto`; `style` `long`, `short` or `narrow` |
| `intl.localeTag` | render | | the ambient locale as BCP 47, `fr-FR` for `fr_FR`, `en` when none is set |
| `intl.list` | render | `items, { type?, style?, locale? }` | the strings joined as `Intl.ListFormat` joins them: `type` `conjunction` (the default), `disjunction` or `unit`, `style` `long` (the default), `short` or `narrow` |
| `intl.compare` | render | `a`, `b`, `{ sensitivity?, numeric?, ignorePunctuation?, caseFirst?, usage?, locale? }?` | the number -1, 0 or 1 as `new Intl.Collator(locale, options).compare(a, b)` answers: `sensitivity` `base`, `accent`, `case` or `variant` (the default), `numeric` ordering runs of digits by value, `ignorePunctuation` skipping punctuation and spaces and `caseFirst` `upper`, `lower` or `false`; the locale's own defaults where an option is not given |
| `intl.localTime` | body | `when`, `locale?` | `toLocaleTimeString(locale)` in this machine's time zone, on the clock CLDR gives the locale's region; without a locale the one `LC_ALL`, `LC_MESSAGES` or `LANG` names, else `en-US`; `Invalid Date` for an invalid instant |
| `intl.localDate` | body | `when`, `locale?`, `withTime?` | `toLocaleDateString(locale)` with no options in this machine's time zone, the date in the locale's calendar and CLDR's pattern for it; with `withTime`, `toLocaleString(locale)`, the time joined on by the short `atTime` pattern; `Invalid Date` for an invalid instant |
| `intl.numeric` | render | `when`, `locale?`, `mode` | the UTC fields of `when` as `toLocaleDateString` writes them with no options for `mode` `"date"`, as `toLocaleTimeString` does for `"time"` and as `toLocaleString` does for `"both"`; without a locale the request's; `Invalid Date` for an invalid instant |
| `intl.dateParts` | render | `when`, `options` | `formatToParts` of `intl.date`'s format, `{ type, value }` by ICU4X's parts |
| `intl.numberParts` | render | `text`, `style`, `options` | `formatToParts` of a number `intl.number` or `intl.currency` wrote, read back by the locale's separators |
| `intl.numericParts` | render | `when`, `locale?` | `formatToParts` of a date given only a time zone: the pieces of CLDR's `yMd` pattern as `year`, `month`, `day`, `era` and `literal` |
| `intl.listParts` | render | `items`, `options` | `formatToParts` of `intl.list`, each item an `element` and the text between them a `literal` |
| `intl.relativeParts` | render | `value`, `unit`, `options` | `formatToParts` of `intl.relative`, the number's parts carrying the singular `unit` and the text around it a `literal` |
| `intl.locale` | render | `tag`, `op` | `"canonical"`, `"maximize"` or `"minimize"` the tag; `"parse"` an `Intl.Locale`'s fields with the tag under `$tag`; `"getCalendars"`, `"getHourCycles"`, `"getNumberingSystems"`, `"getTextInfo"`, `"getTimeZones"` and `"getWeekInfo"` what those methods answer |
| `intl.supported` | render | `key` | `Intl.supportedValuesOf(key)` as ICU 78.3 lists it |
| `intl.supportedLocales` | render | `tags` | `supportedLocalesOf(tags)` of an `Intl` formatter: each tag `snapfire_i18n` holds data for, canonical and once |
| `intl.resolvedOptions` | render | `formatter`, `options` | `resolvedOptions()` of the formatter named, `DateTimeFormat`, `NumberFormat`, `Collator`, `ListFormat`, `Segmenter`, `DisplayNames`, `RelativeTimeFormat`, `PluralRules` or `DurationFormat`: the options given and the defaults node gives the rest, in node's order |
| `intl.duration` | render | `duration`, `options?` | `new Intl.DurationFormat(locale, { style }).format(duration)` |
| `intl.displayNames` | render | `code`, `options` | `new Intl.DisplayNames(locale, { type, style, languageDisplay, fallback }).of(code)` |
| `intl.timeString` | body | `when` | `toTimeString()` in this machine's time zone, `19:37:20 GMT-0400 (Eastern Daylight Time)`, the zone named in the locale `intl.systemLocale` reads; `Invalid Date` for an invalid instant |
| `intl.dateTimeString` | body | `when` | `toString()` of a `Date` in this machine's time zone, `Tue Sep 10 2024 19:37:20 GMT-0400 (Eastern Daylight Time)`; `Invalid Date` for an invalid instant |
| `intl.upper` | render | `text`, `locales`, `fallback?` | `text.toLocaleUpperCase(locales)`, with `locales` a tag or an array whose first tag applies and `fallback` the tag for `undefined`, `null` or an empty array: Unicode's mapping, with the Turkish and Azerbaijani dotted i, Lithuanian's dot above dropped after i and j and Greek accents dropped as ICU drops them; a locale that is not a well-formed tag is `Internal` |
| `intl.lower` | render | `text`, `locales`, `fallback?` | `text.toLocaleLowerCase(locales)`, with `locales` and `fallback` as for `intl.upper`: Unicode's mapping, with the Turkish and Azerbaijani dotless i and Lithuanian's dot above kept on i and j under an accent; a locale that is not a well-formed tag is `Internal` |
| `intl.systemLocale` | body | | the locale `LC_ALL`, `LC_MESSAGES` or `LANG` names as BCP 47, else `en-US` |
| `text.slug` | render | `s` | NFD, marks dropped, lowercased, every run outside `a-z0-9` one hyphen, none at either end |
| `text.truncate` | render | `s`, `max`, `ellipsis?` (`…`) | the first `max` code points and the ellipsis when longer, else `s` |
| `text.frontMatter` | body | `source`, `{ delimiters? }?` | `{ data, content }` as gray-matter 4 reads them: the YAML between a first line holding the opening delimiter and a line holding the closing one, `content` the text after it less one line break; `delimiters` is one string or `[open, close]`, `---` by default; text that does not open with the delimiter is all `content` and `data` is `{}`; YAML that does not parse is `Internal` |
| `text.withFrontMatter` | body | `content`, `data`, `{ delimiters? }?` | `data` as YAML between the delimiter lines, then `content` ending in a line break, as gray-matter's `stringify` of a `{ content }` file writes it; empty `data` writes `content` alone |
| `time.format` | render | `when`, `pattern` | `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced in UTC, every other character kept |
| `time.add` | render | `when`, `amount`, `unit` (`ms`, `s`, `m`, `h`, `d`) | `when` plus `amount` units, `F64` |
| `time.diff` | render | `later`, `earlier`, `unit` | the difference in units, fractional, `F64` |
| `time.parse` | render | `s` | `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`, as `F64` milliseconds; `Null` for anything else |
| `time.now` | body | | `Ambient.now` as `Int` |
| `time.monotonic` | body | | milliseconds since the process first read it, as `F64`; `performance.now()` |
| `time.localOffset` | body | `when` | `getTimezoneOffset()` for that instant in this machine's time zone, minutes behind UTC, from the time zone source the `tz_compiled` or `tz_system` feature builds in; NaN for an invalid instant; `Internal` naming both features in a build with neither |
| `temporal.new` | render | `type`, `...args` | `new Temporal.<type>(...args)`: `Instant`, `PlainDate`, `PlainTime`, `PlainDateTime`, `ZonedDateTime` or `Duration`, as the `$temporal` variant |
| `temporal.static` | render | `type`, `name`, `...args` | `Temporal.<type>.<name>(...args)`: `from`, `compare`, `fromEpochMilliseconds` and `fromEpochNanoseconds` |
| `temporal.get` | render | `value`, `field` | `value.<field>`: a date's or a time's parts, a zoned value's `timeZoneId`, `offset` and epoch values, a duration's units, `sign` and `blank`; `undefined` for a field the type lacks |
| `temporal.call` | render | `value`, `method`, `...args` | `value.<method>(...args)`: `add`, `subtract`, `until`, `since`, `with`, `round`, `equals`, `total`, the conversions and `toString` with its options; `valueOf` is a `TypeError` |
| `temporal.now` | body | `name`, `timeZone?` | `Temporal.Now.<name>()` at the clock `time.now` reads, in the zone given or this machine's |

## 8. Evaluation Rules

### Reads

* `Param(name)` and `Query(name)`: `Value::Str`. `Value::Null` when absent.
* `ParamsAll` and `QueryAll`: the route's params and the request's query, each as an object.
* `Session(key)`: reads the draft, so a body sees its own earlier writes. `Value::Null` when absent.
* `Identity(path)`: walks `{ subject, claims }` from the session's identity. `Value::Null` when anonymous or when a step is missing.
* `Locale`: the request's locale tag. `Value::Null` under a context with none.
* `Locales`: `ctx.locales`, the configured tags as a `Value::Seq` of strings in `[locales]` order. Empty under a context with none.
* `Path`: the path the request matched, without its query. An empty `Value::Str` under a context with none.
* `Document`: the path of the page the document is showing. That is `RequestCtx::document` when an intercepted render set it. Otherwise it is `Path`. A render reads it from the `$document` prop. The plan spells it `(document)`.
* `Host`: the host the request named. `null` unless `[server] hosts` lists it.
* `Origin`: `document.origin` as the host checked it at boot, `Value::Str` or `Value::Null`. The plan spells it `(origin)`.
* `Address`: `ctx.address`, the navigation's own request on an intercepted render, as a map of `path`, `params` and `query`. Otherwise null.
* `Config(key)`: one `[public]` value, `ctx.config.<key>`.
* `Input`: the value passed to `run`.
* `Now`: `Value::Int` from the clock.

### Truthiness

* `Null`, `false`, integer and float zero and the empty string are false. Every other value is true, including an empty `Seq` or `Map`.
* `Coalesce` substitutes on `Null` only.
* `Logic` returns the deciding operand.

### Operators

* `Arith` on two `Int`s stays `Int`. Integer overflow and division by zero are `Internal`.
* `Add` with a `Str` on either side concatenates the other operand as `String(x)` writes it.
* `Add` with a `Seq` on either side and no `Map` concatenates both sides as text. An array is written as `join(",")` writes it, with a null or undefined item empty.
* `Arith` on any other pair of numbers, strings, booleans and `Null` reads both through `ToNumber` and yields `F64`. `Null` is 0, a boolean 0 or 1 and a string its numeric literal, empty as 0 and anything else as NaN.
* `Arith` with an array or an object operand is `Internal`.
* `Compare` orders two numbers of any representation by value, two `Str` by code unit and two `Bool` as 0 and 1.
* `Eq` and `Ne` between values of different kinds are false and true.
* An ordering between other scalars reads both through `ToNumber`, so `"2" < 10` is true and anything against NaN is false.
* A comparison involving an array or an object is `Internal`.
* `Template` concatenates the string form of each part.
* `Field` on a non-map is `Value::Null`.
* `Index` takes a `Map` with a `Str` key or a number, read under `interp::number_key`. It takes a `Seq` with an `Int`, `UInt` or integral `F64` index. An index out of range reads `Value::Null`.
* `Index` on `Null` is `Null`. Any other target is `Internal`.

### Conversions

* `Str` writes `Null` as `null`, booleans, integers and strings as themselves, integral floats without a fraction and the infinities as `Infinity` and `-Infinity`. A collection is `Internal`.
* `Num` is `Number(x)`: `F64` from integers, floats, booleans, `Null` and strings, an unparseable string as NaN. A collection is `Internal`.
* `BigInt` gives an `Int` from integers, integral floats, booleans and parseable strings. A fractional float or an unparseable string is `Invalid`.
* A number writes as `String(n)` does: the fewest digits that read back, the closest of those with a tie to the even one. From 1e21 up and below 1e-6 it uses exponent form, `1e+21` and `1.5e-7`.

### Builtins

Collection expressions:

* `Map`, `Filter`, `Find`, `FindIndex`, `Some`, `Every` and `FlatMap` apply a lambda `(item, index, array)` over a `Seq`.
* `Reduce` applies `(acc, item, index, array)` from `init`.
* The array is passed only to a lambda naming more than two parameters.
* A non-`Seq` operand is `Internal`.
* `Find` yields `Value::Null` when nothing matches.
* `FlatMap` spreads a `Seq` result one level and keeps any other.
* `Sort` merges stably. A comparator's answer is read as a number, NaN and a non-number as 0. With no comparator each item is compared as `String(x)` by UTF-16 code units, so `[10, 9, 1]` sorts to `[1, 10, 9]`.
* `Entries` yields a `Seq` of two-element `Seq` pairs in insertion order. `Keys` and `Values` likewise. All three require a `Map`.
* `Length` counts `Seq` items, `Str` UTF-16 code units or `Map` entries, as `F64`.

Strings:

* `StartsWith` and `EndsWith` take two strings and yield a `Bool`. Every string starts and ends with the empty string. `StartsWith` takes an optional third argument, the UTF-16 position to test from, clamped to the string.
* `Trim` strips JavaScript's whitespace and line terminators from both ends. A second argument `start` or `end` strips that end only. `\u{feff}` is stripped and `\u{85}` is not.
* `Split` takes a subject, a separator and an optional limit read as `ToUint32`. It yields a `Seq` of at most that many strings.
* `Split` keeps a leading or trailing separator's empty piece. A separator the subject does not hold gives one piece.
* `Split` with an empty separator splits into characters. It is `Internal` on a subject holding a character outside the Basic Multilingual Plane.
* `Split` refuses to build more than a million pieces.
* `Replace` takes a subject, a pattern and a replacement and replaces the first occurrence of a string pattern.
* `Replace`'s replacement reads `$$` as one dollar, `$&` as the match, `` $` `` as the text before it and `$'` as the text after. With a string pattern `$1` stays as written.
* `Replace` with a regular expression replaces the first match. With `g` it replaces every match. The replacement also reads `$1` to `$99` and `$<name>`, a group that did not take part as nothing.
* `ReplaceAll` replaces every occurrence of a string, `$` substitutions included.
* `ReplaceWith(subject, pattern, f)` calls `f` with the match, each group, the UTF-16 offset and the subject. It writes `String` of the answer at every match of a regular expression with `g` and at the first match otherwise.
* `Slice`, `At`, `IndexOf` and `Concat` take an array or a string. `Reverse` takes an array. `PadStart`, `PadEnd` and `Substring` take a string.
* Indices follow `ToIntegerOrInfinity`, negative counting from the end where JavaScript's do. A string's indices count UTF-16 code units. A cut through a surrogate pair is `Internal`.
* `IndexOf` over an array compares numbers by value whatever their width and anything else by equality.
* `LastIndexOf` takes an array or a string as `IndexOf` does, searching back from the end or from its third argument.
* `PadStart` and `PadEnd` refuse to build more than `Repeat`'s bound.
* `CharAt` is the UTF-16 code unit at an index as a string, empty past either end.
* `CharCodeAt` is that code unit as a number, NaN past either end.
* `CodePointAt` is the code point starting there, a lone surrogate as its unit and null past either end.
* `Radix` is `x.toString(radix)` for a radix from 2 to 36. A non-number is written as `String(x)`. A radix outside 2 to 36 is `Internal`.
* `Radix` with a radix other than 10 is `Internal` for a number with a fraction. So is a number past 2^53 in a radix that is not a power of 2.

Objects and arrays:

* `Omit` takes a `Map` and string keys and yields the map without those keys. `Null` reads as an empty map. Any other first argument is `Internal`.
* `FromEntries` takes `[key, value]` pairs, each key as `String(key)`. A later key replaces an earlier one in place.
* `Unique` keeps an array's first occurrence of each item by the equality `IndexOf` uses.
* `HasKey` is whether a map holds `String(key)` or an array has the index `key`.
* `IsArray` is `Array.isArray`.
* `WriteBack([value, last, ...], [index, ...])` is what a function that writes fields of its parameters answers. An `Apply` of it answers `value` and writes each `last` to the argument at its `index`, when that argument is a variable, a cell or a path of fields and indexes from one.
* `WriteBack` fails when the same place is passed for two written parameters. It also fails where no `Apply` takes it back, such as a callback of `Map`.
* `Uint8Array` takes an array of numbers, each truncated and taken modulo 256. It also takes bytes, kept as they are. Given a length it makes that many zeros.

JSON and text output:

* `JsonParse` is `JSON.parse(text)`. Every number is an `F64` and an object's keys keep document order. Text that is not JSON throws a `SyntaxError` a `catch` can read.
* `Json` is `JSON.stringify(value, null, indent)`. Maps keep insertion order and a non-finite number is `null`. An `Int` is `Internal`. A number indent is capped at 10 spaces and a string indent at its first 10 characters.
* `ClassNames` joins what `clsx` keeps with a space: each non-empty string and non-zero number, an array's items in turn and an object's keys whose values are truthy.
* `FormEncode` writes an object's entries, `[key, value]` pairs or a query string as `URLSearchParams` does. Letters, digits and `*-._` stay as they are, a space is `+` and every other byte is `%XX`.
* `FormGet` reads that string back: the first value under `String(key)`, null for none.

Errors, types and logging:

* `IsError(x, name)` with `name` `Error` is `x instanceof Error`: an object whose `name` is one of the error constructors' names and whose `message` is a string. With another `name` it is whether `x.name` is that name.
* `Throw` fails the render with `String(message)`.
* `TypeOf` is `typeof`, with null answered `"undefined"` and a handler or an action `"function"`.
* `IsNaN` is the global `isNaN`.
* `ParseInt` reads `String(s)` as `parseInt` does. Leading white space and a sign are skipped. A radix of 0 or none reads 10. It reads 16 after `0x`. A radix outside 2 to 36 is NaN. NaN when there are no digits.
* `ParseFloat` reads `String(s)` as `parseFloat` does. Leading white space is skipped. It reads the longest signed decimal literal at the start, with an optional fraction and exponent, or `Infinity`. NaN when there is none.
* `Log` is a `console` call. Its first argument names the method: `log`, `info`, `debug`, `warn` or `error`.
* `Log` writes the rest joined by spaces: a string as it is, a scalar as `String` makes it and an object or an array as JSON. It is a `tracing` event under `interp::CONSOLE_TARGET` at that method's level. It evaluates to null.
* `LogOf` is `Log` with the values as one array in its second argument, `console.log(...items)`.

Responses and requests:

* `Response` takes a body and an init.
* A `Str` body is sent as UTF-8 with `text/plain;charset=UTF-8` unless the init's headers name a type. A `Bytes` or `Uint8Array` body is sent as its bytes. `Null` or `Undefined` sends nothing.
* The init's `status` must lie in 200 to 599 and is 200 when absent. Its `headers` is an object of strings or an array of `[name, value]` pairs.
* `ResponseJson` writes its value as `Json` does with `application/json`.
* `ResponseRedirect` takes a path or an absolute URL and a status among 301, 302, 303, 307 and 308, 302 when absent. It sets `location`.
* `RedirectTo` is `redirect(to)`: a 303 whose target must be a path on this origin as `RedirectHandle::check` takes it, else `Invalid`.
* Each response builtin yields the `Value::Variant` that `snapfire_fsr_runtime::HttpResponse::from_value` reads.
* `RequestNew` makes the `Value::Variant` tagged `snapfire_fsr_runtime::REQUEST_TAG` holding `url`.
* `ResponseMember` reads a response's `status` or `ok`. Reading those fields off a response directly answers the same.
* `ResponseHeaderGet` reads a header, matched without regard to case. With `"has"` third it answers whether the header is there.
* `ResponseHeaderSet` answers the response with `set` replacing every header of the name, `append` adding one and `delete` removing them.

Dates:

* `DateMs` reads a number as milliseconds since the epoch and an ISO 8601 string as `time.parse` does. Anything else is NaN.
* `DatePart` takes the milliseconds and a part name in UTC: `year`, `month` from 0, `date`, `day` from Sunday as 0, `hours`, `minutes`, `seconds` or `milliseconds`. NaN for an invalid date.
* `IsoString` writes `YYYY-MM-DDTHH:mm:ss.sssZ`. It is `Internal` for an invalid date.

Math:

* `Pow`, `Sqrt`, `Trunc` and `Sign` are the `Math` functions.
* `Math` is the `Math` function its first argument names, computed by `jsmath`.
* `MinOf` and `MaxOf` take one array. They answer `Infinity` and `-Infinity` when it is empty.

Regular expressions, each over a value holding `jsregex::PATTERN` and `jsregex::FLAGS`:

* `Split` with a regular expression keeps the groups between the pieces.
* `RegexTest` is `re.test(s)` without `lastIndex`, so `g` changes nothing.
* `Match` is the first match and its groups. With `g` it is every whole match. Null for none.
* `MatchAll` is each match with its groups. It is `Internal` without `g`.
* `Search` is the UTF-16 index of the first match, -1 for none.

### Calls

* Arguments evaluate in order. An argument whose value is `Value::Null` is omitted from the `ValueMap` sent.
* The call goes through `RequestCtx::services`. A `ServiceError` becomes a `Fail` with the same kind and message.

### Session writes

* `SessionSet` with an empty path replaces the key.
* `SessionSet` with a path walks `Map` steps by `Str` key, creating maps where a step is `Null`. It walks `Seq` steps by an `Int` or integral `F64` index within bounds.
* `SessionDelete` with an empty path removes the key. With a path it removes the last step's entry from its `Map`. It does nothing where the path does not exist.
* `SessionSet` and `SessionDelete` mark the key in `written`.
* `SessionExtend` evaluates `seconds`, which must be a positive whole number, `Int` or integral `F64`. Anything else fails the body with `Internal`.
* The last `SessionExtend` the body ran is applied to the cell after the draft commits. A failed body leaves the end where it was. It marks no key and does not dirty the cell.

### Guards

* Before the body runs, its top-level statements are scanned in order. A guard whose condition reads no `Var` and contains no `Call` is evaluated then.
* The scan stops at the first `If`, `ForOf`, `While`, `Break`, `Continue`, `Try`, `SessionSet`, `SessionDelete`, `SessionExtend` or `Act`. It also stops at the first statement holding an `After`.
* Every guard also runs in sequence at its own position.
* An unknown kind name is `Internal`.

### Parallel lets

* A run of consecutive `Let` statements in one block, none of which reads a name bound earlier in the run, is evaluated together when at least two of them contain a `Call`.
* Each `Let` in the run evaluates over a snapshot of the current scope and draft.
* Any other statement is evaluated in sequence, as is any `Let` that reads an earlier name in the run.

## 9. Regular Expressions and Math

### JsRegex

`snapfire_fsr_ir::jsregex::JsRegex`: a compiled ECMAScript pattern.

* `is_match(&self, subject: &str) -> Result<bool, String>`
* `first(&self, subject: &str) -> Result<Option<Found>, String>`
* `all(&self, subject: &str) -> Result<Vec<Found>, String>`
* Backreferences, lookaround, named groups and the `u` and `v` flags are supported.
* Without `u` or `v`, a subject holding a character past U+FFFF is matched as UTF-16 code units. A match that would split a surrogate pair is an error.

### Found

One match as byte ranges into the subject.

* `whole(&self) -> Range<usize>`
* `len(&self) -> usize`: the number of groups, the whole match counted.
* `is_empty(&self) -> bool`
* `group(&self, index: usize) -> Option<Range<usize>>`: `None` for a group that did not take part.
* `named(&self, name: &str) -> Option<Range<usize>>`
* `has_names(&self) -> bool`
* `texts<'a, 's: 'a>(&'a self, subject: &'s str) -> impl Iterator<Item = Option<&'s str>> + 'a`

### jsregex functions

* `PATTERN = "$regex"` and `FLAGS = "$flags"`: the fields of a regular expression value.
* `MAX_PATTERN: usize = 1024`: the longest pattern compiled, in characters.
* `MAX_INPUT: usize = 1 << 20`: the longest subject matched, in bytes. A longer subject is `Internal`.
* `value(pattern: &str, flags: &str) -> Value`: a regular expression as a value.
* `of(value: &Value) -> Option<(&str, &str)>`: the source and flags of a regular expression value.
* `compiled(pattern: &str, flags: &str) -> Result<Arc<JsRegex>, String>`: the compiled expression, cached by source and flags. The error names what is refused.
* `compiled` refuses the `y` flag, a pattern longer than `MAX_PATTERN` and an unbounded quantifier over a group with an alternative made only of repeating atoms, such as `(a+)+` or `(\s*\w+)*`.
* `utf16_index(s: &str, byte: usize) -> usize`: a UTF-8 byte offset as a UTF-16 index.
* `expand(out: &mut String, replacement: &str, subject: &str, found: &Found)`: writes `replace`'s replacement string for one match.

### jsmath

`snapfire_fsr_ir::jsmath`: JavaScript's `Math` functions as V8 computes them. Each is `fn(f64) -> f64` except `atan2(y: f64, x: f64) -> f64`.

* `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `expm1`, `log`, `log2`, `log10`, `log1p` and `cbrt`: ports of the LLVM libc double-precision functions, along the fused multiply-add path.
* `sinh`, `cosh`, `asinh`, `acosh` and `atanh`: the fdlibm algorithms over them.
* Every function but `atan` and `atan2` is correctly rounded. `atan` and `atan2` match Chrome on ARM and may differ in the last bit from Chrome on x86-64 without FMA.
* `Math.pow` and `Math.tanh` are not provided.

## 10. Runtime Adapters

### PreparedBody

A translated body as every adapter below runs it. `Clone`. `Deref<Target = Body>`. `From<Body>` reads it with no constants.

* `new(body: Body, consts: Option<&Consts>) -> PreparedBody`: builds the body's `Program` once and its `plan_hash` with the plan's constants.
* `with_consts(body: Body, consts: Option<&ConstHashes<'_>>) -> PreparedBody`: `new` against a table every body of one plan can share.
* `compiled_from(self, compiled: &[&Native]) -> PreparedBody`: attaches the first entry whose `plan` equals the body's own hash. With none matching, the body is returned as it was.
* `with_native(self, native: NativeBody) -> PreparedBody`: attaches `native` without a hash check.
* `plan(&self) -> u64`
* `is_compiled(&self) -> bool`
* `body(&self) -> &Body`
* `run(&self, interpreter: &Interpreter, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`: runs as `Prepared::Native` when a compiled body is attached, as its `Program` when `Program::of` built one and as `Prepared::No` otherwise.

### IrSource

A body answering a data source id. Implements `snapfire_fsr_runtime::DataSource`.

* `new(id: impl Into<String>, body: impl Into<PreparedBody>) -> IrSource`
* `with_interpreter(self, interpreter: Interpreter) -> IrSource`
* `load` runs the body with no input and returns the `Value::Map` it returned as `Data`. A non-map return or a `Fail` is a `LoadError` carrying the id and the message.

### IrPaths

A translated `paths` naming the parameter sets a route prerenders. Implements `snapfire_fsr_runtime::Paths`.

* `new(source_id: impl Into<String>, body: impl Into<PreparedBody>) -> IrPaths`
* `with_interpreter(self, interpreter: Interpreter) -> IrPaths`
* `paths` runs the body with no input and reads the `Value::Seq` it returned as one `Params` per `Value::Map`, a string or a number per key.
* A non-list return, an entry that is not an object, a value of another kind or a `Fail` is a `LoadError` carrying the id and the message.

### IrStore

A module's translated `store` and `defaults` exports, either or both. Implements `snapfire_fsr_runtime::Seeds`.

* `new(source_id: impl Into<String>) -> IrStore`
* `with_store(self, store: Option<impl Into<PreparedBody>>) -> IrStore`
* `with_defaults(self, defaults: Option<impl Into<PreparedBody>>) -> IrStore`
* `with_interpreter(self, interpreter: Interpreter) -> IrStore`
* `with_keys(self, keys: Option<Vec<String>>) -> IrStore`: the plan's `store-keys`, which `keys` answers with when set.
* `seed` runs the `store` body with the segment's data as `Input` and returns the `Value::Map` it returned. With no `store` it returns an empty map.
* `defaults` does the same with the `defaults` body.
* A non-map return or a `Fail` is a `LoadError`.
* `keys` without `with_keys` is every field name a `return` of either body can hold, through branches, loops and a returned `let`.
* `keys` is `None` when a return is anything but an object of named fields, such as a spread or a computed key.

### IrAction

A body answering an action id, a route handler, the middleware or a theme. Implements `snapfire_fsr_runtime::ActionHandler`.

* `new(body: impl Into<PreparedBody>) -> IrAction`
* `with_interpreter(self, interpreter: Interpreter) -> IrAction`
* `call` runs the body with the submitted value as `Input` and returns what it returned. A `Fail` is an `ActionError` with the kind preserved.

### IrMeta

A translated `meta` describing the document from its loader's data. Implements `snapfire_fsr_runtime::Metadata`.

* `new(source_id: impl Into<String>, body: impl Into<PreparedBody>) -> IrMeta`
* `with_interpreter(self, interpreter: Interpreter) -> IrMeta`
* `describe` runs the body with the segment's data as `Input`. It must return an object. `title`, `absolute`, `description` and `head` are read from it.

### IrEvaluator

Translated components rendered in Rust. Implements `snapfire_fsr_runtime::Evaluator`.

* `new(components: impl IntoIterator<Item = (String, Component)>) -> IrEvaluator`: each component goes through `render::prepare`.
* `with_interpreter(self, interpreter: Interpreter) -> IrEvaluator`
* `covers(&self, module: &ModuleId) -> bool`: whether it holds the module.
* `components(&self) -> Arc<Components>`
* `interpreter(&self) -> &Interpreter`
* `modules(&self) -> Vec<String>`: every module id, sorted.
* `evaluate` on a module it does not hold is an `EvalError`: `not a lowered component`.
* A root component whose render is whole is a `Client` node with `rendered: true` and the rendered nodes as children.
* A root component whose render is not whole is a `Client` node with `rendered: false` and its slot regions as children.
* `evaluate` splits the markup at each `SLOT_MARK` and places a `Node::Slot` there.

## 11. The Host

`snapfire_fsr_ir::host`: the names a translated terminal page and the program that runs it share. The web host runs none of them.

### Event

What a terminal runs a handler on, the event its `Handler::event` names.

* `pub enum Event { Input, Mount, Interval(u64), Timeout(u64), Call(usize), Process { id: String, report: Report }, Effect(usize), Resume(String) }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `Input`: a key, Ink's `useInput`. The event is `{ input, key }`.
* `Mount`: the component's first render.
* `Interval(ms)`: every `ms` milliseconds while the component renders.
* `Timeout(ms)`: once, `ms` milliseconds after the component first renders.
* `Call(n)`: the function a mount effect declares in place `n`.
* `Process { id, report }`: a report from the process spawned as `id`.
* `Effect(n)`: the effect in place `n` whose dependencies changed.
* `Resume(at)`: the rest of an async function after the `await` at `at`, `file@offset`. It runs when the awaited promise settles, with `{ value, ok, carry }`.
* `name(&self) -> String`: `input`, `mount`, `interval:<ms>`, `timeout:<ms>`, `call:<n>`, `spawn:<id>:<report>`, `effect:<n>` or `resume:<at>`.
* `parse(name: &str) -> Option<Event>`: reads one back.
* `pub fn timer(repeats: bool, ms: u64) -> Event`: `Interval(ms)` when it repeats, else `Timeout(ms)`.

### Report

What a spawned process reports.

* `pub enum Report { Stdout, Stderr, Exit, Close, Error, Resize }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.
* `Stdout` and `Stderr`: a chunk of text.
* `Exit` and `Close`: `{ code, signal }`.
* `Error`: `{ message }`.
* `Resize`: `null`. Only `TERMINAL_OUTPUT` reports it.
* `name(self) -> &'static str`
* `parse(name: &str) -> Option<Report>`

### Act

What a terminal handler asks its terminal for, the action its `Stmt::Act` names.

* `pub enum Act { Stdout, Stderr, Timer, ClearTimer, Cleanup, Spawn, Stdin, Settle, Then, Listen, Unlisten, Run, Suspend, Exit, Focus, FocusNext, FocusPrevious, FocusEnable, FocusDisable }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.
* `Stdout`: a line of text written above the page, `useStdout().write`.
* `Stderr`: the same through `useStderr().write`.
* `Timer`: `{ event, ms, repeats }`, `{ id, event, carry, ms, repeats }` or `{ settle, ms }`.
* `ClearTimer`: `{ timer }`.
* `Cleanup`: `{ effect, event, carry }`.
* `Spawn`: `{ id, command, args, options, stdin }`. With `stdin` true the process's standard input is a pipe `Stdin` writes.
* `Stdin`: `{ id, text }` writes to the process's standard input. `{ id, end: true }` closes it.
* `Settle`: `{ settle, value }`.
* `Then`: `{ deferred, event, carry }`. For a call that failed as it was awaited it is `{ rejected, event, carry }`.
* `Listen`: `{ process, report, event, carry }` or `{ process, report, settle }`.
* `Unlisten`: `{ process, report, event }`.
* `Run`: `{ event, carry }`.
* `Suspend`: `{ until, settles }`, Ink's `suspendTerminal`.
* `Exit`: Ink's `exit()`.
* `Focus`: `focus(id)`. `FocusNext`, `FocusPrevious`, `FocusEnable` and `FocusDisable` take what Ink's calls pass.
* `name(self) -> &'static str`: the action's name, `$stdout`, `$timer`, `$spawn` and so on.
* `parse(name: &str) -> Option<Act>`

### Host keys

* `SETTLES = "$settles"`: a promise's `resolve` or `reject` is held under `{ "$settles": deferred, ok }`, `ok` false for `reject`.
* `ACT_REF = "$act"`: an act as a value, `{ "$act": action }`.
* `act_ref(expr: &Expr) -> Option<String>`: the act a translated value refers to, through any field of an object literal that holds one.
* `TIMER = "$timer"`: a timer a handler started, `{ "$timer": id }`.
* `PROCESS = "$process"`: a process a block spawned, `{ "$process": id }`.
* `TERMINAL_OUTPUT = "$terminal"`: the id `PROCESS` names for the terminal's own output, which reports `Resize`.

## 12. Error Handling

### Fail

Re-exported from `snapfire_fsr_core::ext`, with `FailureKind`.

* `pub struct Fail { pub kind: FailureKind, pub message: String, pub thrown: Option<Box<Value>>, pub call: Option<String> }`. `Debug`, `Clone`, `PartialEq`, `#[non_exhaustive]`. Implements `std::error::Error`.
* `Display` is `{kind}: {message}`.
* `thrown`: what a `throw` threw, which a `catch` binds. `None` for a failure the interpreter or a call raised.
* `call`: the service call that failed, `service.method`.
* `new(kind: FailureKind, message: impl Into<String>) -> Fail`
* `internal(message: impl Into<String>) -> Fail`: kind `Internal`.
* `with_call(self, call: impl Into<String>) -> Fail`
* `with_thrown(self, value: Value) -> Fail`
* A guard yields its named kind. A service error keeps its kind.
* `Num` and `BigInt` of unparseable input are `Invalid`.
* Type mismatches, unbound names, overflow and structural misuse are `Internal`.

### SexprError

* `pub struct SexprError(String)`. `Debug`, `Clone`, `PartialEq`, `Eq`, `thiserror::Error`. `Display` is the message.
* A syntax error's message starts `line <n>: `. A shape error names the form.
* `new(msg: impl Display) -> SexprError`

### ShadowRootError

Returned by `ShadowRoot::take`. Root re-export. `Debug`, `Clone`, `PartialEq`, `Eq`, `thiserror::Error`, `#[non_exhaustive]`. Each `Display` names the root `<template>` and what to write instead.

* `NoMode`: the root `<template>` has no `shadowrootmode`.
* `NotLiteral { name: String, expected: &'static str }`: `shadowrootmode` or an option is an expression and not a literal of its type.
* `Mode { mode: String }`: a `shadowrootmode` that is neither `open` nor `closed`.
* `Attribute { name: String }`: an attribute the parser would drop with the template, named as the template wrote it, such as `key` or `dangerouslySetInnerHTML`.
* `Spread`: a spread on the root `<template>`.
* `NotAlone`: a `<template shadowrootmode>` beside other root nodes or in a branch of a root `If`.
