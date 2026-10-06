# API Reference: snapfire_fsr_ir

The lowered form of a loader or action body and the interpreter that runs it over the value model.

## Contents

* [1. The Tree](#1-the-tree)
  * [Body](#body)
  * [Stmt](#stmt)
  * [Expr](#expr)
  * [Tmpl](#tmpl)
  * [Component](#component)
  * [Owner](#owner)
  * [ShadowRoot](#shadowroot)
  * [ScopedStyle](#scopedstyle)
  * [ShadowMode](#shadowmode)
  * [Entry](#entry)
  * [Lit](#lit)
  * [ArithOp](#arithop)
  * [CompareOp](#compareop)
  * [LogicOp](#logicop)
* [2. The JSON Form](#2-the-json-form)
  * [from_json](#from_json)
  * [to_json](#to_json)
* [3. The S-Expression Form](#3-the-s-expression-form)
  * [Sx](#sx)
  * [parse](#parse)
  * [print](#print)
  * [the IR conversions](#the-ir-conversions)
  * [SexprError](#sexprerror)
* [4. The Interpreter](#4-the-interpreter)
  * [Interpreter](#interpreter)
  * [Outcome](#outcome)
  * [Clock](#clock)
  * [Extensions](#extensions)
  * [Reach](#reach)
  * [Ambient](#ambient)
  * [The Standard Library](#the-standard-library)
  * [Catalogs](#catalogs)
  * [Frameworks](#frameworks)
  * [ReactMajor](#reactmajor)
  * [VueMajor](#vuemajor)
* [5. Evaluation Rules](#5-evaluation-rules)
  * [Reads](#reads)
  * [Truthiness](#truthiness)
  * [Operators](#operators)
  * [Conversions](#conversions)
  * [Builtins](#builtins)
  * [Calls](#calls)
  * [Session writes](#session-writes)
  * [Guards](#guards)
  * [Parallel lets](#parallel-lets)
* [6. Runtime Adapters](#6-runtime-adapters)
  * [IrSource](#irsource)
  * [IrPaths](#irpaths)
  * [IrStore](#irstore)
  * [IrAction](#iraction)
* [7. Error Handling](#7-error-handling)
  * [Fail](#fail)
  * [ParseError](#parseerror)
  * [ShadowRootError](#shadowrooterror)

## 1. The Tree

### Body

The statements of one loader or action, in order.

* `pub type Body = Vec<Stmt>`

### Stmt

One statement. Derives `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`; serialised externally tagged in `snake_case`.

* `Let { name: String, expr: Expr }` binds a name for the rest of the enclosing block.
* `Set { name: String, expr: Expr }` gives the nearest binding of `name` a new value, so a write inside a branch or a loop outlives it. A name nothing has bound is `Internal`.
* `If { cond: Expr, then: Body, else: Body }`; `else` is omitted from JSON when empty.
* `ForOf { name: String, over: Expr, body: Body }`; `over` must evaluate to `Value::Seq`.
* `Return(Expr)` ends the body, from any depth.
* `Guard { cond: Expr, kind: String, message: String }` fails the body with `kind` when `cond` is truthy; `kind` is a `FailureKind` name.
* `SessionSet { key: String, path: Vec<Expr>, value: Expr }`; `path` is omitted from JSON when empty.
* `SessionDelete { key: String, path: Vec<Expr> }`; `path` is omitted from JSON when empty.
* `SessionExtend { seconds: Expr }` moves the session's end to `seconds` from now once the body commits. An action, a route handler or middleware holds one; the lowerer refuses it in a loader.
* `Act { action: String, input: Expr }` asks for the action `action` to be dispatched with `input`. Only a handler holds one: `island_step` evaluates the input and collects the pair in `Stepped::acts` for the host to dispatch; a body reaching one fails with `Internal`.
* `Expr(Expr)` evaluates for effect and discards the value.

### Expr

One expression. Derives `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`; serialised externally tagged in `snake_case`.

* `Param(String)`, `Query(String)`, `Session(String)`, `Identity(Vec<String>)`, `Locale`, `Path`, `Document`, `Origin`, `Input`, `Now`, `Var(String)`.
* `Context(String)`: the value the nearest provider of a context holds around the render point, `Value::Null` under none. A `Tmpl::Let` whose name is `render::CONTEXT_PREFIX` and the context id is a provider; nested components see it and an island does not, since the browser mounts each island in a root of its own.
* A Vue placement's children may hold `Tmpl::Let`s named `render::SLOT_CONTENT_PREFIX` and a slot: the caller's content for that slot, its value the condition it is given under. A `Tmpl::Let` named `render::SLOT_OUT_PREFIX` and a slot is the child's `<slot>`: its value the props it hands that content (bound to `render::SLOT_PROPS`) and its body the fallback written when the caller gave none. Vue's fragment anchors wrap either.
* `Lit(Lit)`.
* `Object(Vec<Entry>)` takes `Field`, `Computed` and `Spread` entries; `Array(Vec<Entry>)` takes `Item` and `Spread` entries. A wrong entry kind is `Internal`.
* `Field(Box<Expr>, String)`, `Index(Box<Expr>, Box<Expr>)`.
* `Arith(ArithOp, Box<Expr>, Box<Expr>)`, `Compare(CompareOp, Box<Expr>, Box<Expr>)`, `Logic(LogicOp, Box<Expr>, Box<Expr>)`, `Not(Box<Expr>)`, `Coalesce(Box<Expr>, Box<Expr>)`, `Ternary(Box<Expr>, Box<Expr>, Box<Expr>)`, `Template(Vec<Expr>)`.
* `Call { service: String, method: String, args: Vec<(String, Expr)> }`; `args` defaults to empty in JSON.
* `Lambda { params: Vec<String>, body: Box<Expr> }`; evaluating a lambda as a value is `Internal`.
* `Map(over, f)`, `Filter(over, f)`, `Reduce(over, init, f)`, `Find(over, f)`, `FindIndex(over, f)`, `Some(over, f)`, `Every(over, f)`, `FlatMap(over, f)`, all `Box<Expr>`; `f` must be a `Lambda`. Each call passes the item and its index (`Reduce` the accumulator before them) plus the array last when the lambda names more than two parameters.
* `Sort(over, f)`: a sorted copy, stable, by the comparator lambda `f` or, when `f` is `Lit::Null`, by `String(x)` in UTF-16 code units.
* `Entries(Box<Expr>)`, `Keys(Box<Expr>)`, `Values(Box<Expr>)`, `Length(Box<Expr>)`.
* `Str(Box<Expr>)`, `Num(Box<Expr>)`, `BigInt(Box<Expr>)`.
* `Ext { module: String, name: String, args: Vec<Expr> }`: an extension call, `intl.number(n)`, answered by the interpreter's `Extensions` under `module.name` with the arguments evaluated and the request's locale and clock as `Ambient`. A name the registry lacks is `Internal`, ``extension `x.y` is not registered``. `has_call` is false for a standard member and true for any other, so an application's pair may sit on the async path. `Expr::ext(module, name, args)` builds one.
* `Hoist { id: u32, expr: Box<Expr> }`: a render-path expression whose inputs are props only. Evaluates as `expr`; under a render the value is also recorded in the environment's `Hoists` under `id`, so the browser reads it instead of computing it. `id` is unique within the component's module. In a body it is `expr` and nothing more.
* `Handler(u32)`: the rendering component's handler at that index as a value, which a prop holding a function carries. It evaluates to `render::handler_value(path, index)`, a `Value::Variant` tagged `render::HANDLER_VALUE` whose payload is the token `<index>` or `<path>/<index>` for the component rendered at `path`; `render::handler_token(&Value) -> Option<&str>` reads it back. An `$on:<event>` attribute holding one prints that token in server mode, so the step runs the handler of the component that passed it. Any other attribute and text write nothing for it. `RenderedIsland::mount_props` leaves it out, since a function does not cross into an island. Outside a render it is an error. The plan writes `(handler 2)`.
* `Expr::lit_str(s: impl Into<String>) -> Expr`, `Expr::lit_int(n: impl Into<i128>) -> Expr`, `Expr::var(name: impl Into<String>) -> Expr`.
* `Expr::field(self, name: impl Into<String>) -> Expr`, `Expr::index(self, key: Expr) -> Expr`.
* `Expr::call(service, method, args: Vec<(&str, Expr)>) -> Expr`.
* `Expr::lambda(params: &[&str], body: Expr) -> Expr`.
* `Expr::object(entries: Vec<(&str, Expr)>) -> Expr` builds `Field` entries only.
* `Expr::free_vars(&self, out: &mut Vec<String>)` appends every `Var` name read and not bound by an enclosing lambda, without duplicates.
* `Expr::visit(&self, f: &mut dyn FnMut(&Expr))` calls `f` on the expression and every expression beneath it, in tree order.
* `Expr::has_call(&self) -> bool` is true when any `Call` appears in the tree.
* `Expr::reads_request(&self) -> bool` is true when a `Param`, `Query`, `Session`, `Identity`, `Input` or `Now` appears in the tree. `Locale`, `Path`, `Document` and `Origin` are not counted: a prerendered route has one of each, so a body reading only them renders once per locale rather than once per request.
* `body_reads_request(body: &Body) -> bool` is true when any statement reads the request or writes the session.
* `body_params_read(body: &Body) -> Vec<String>` is every route parameter the body reads, by name, without duplicates.

### Tmpl

A lowered component's tree: `Text`, `Expr`, `Element { tag, attrs, children }`, `Fragment`, `If`, `For`, `Let`, `Component { module, props, children, id, keyed }`, `Slot` and `Island { module, props, children, when, mode }`, a component placed as its own island with `when` its hydration timing, `"load"`, `"visible"` or `"idle"`, when the use named one and `mode` `Some("server")` for an island whose events round-trip to the server, absent for the browser mode every island is in otherwise.

### Component

* `pub struct Component { pub body: Body, pub render: Tmpl, pub state: Vec<String>, pub stores: Vec<(String, String)>, pub handlers: Vec<Handler>, pub owner: Owner, pub shadow: Option<ShadowRoot>, pub scope: Option<ScopedStyle> }`; `Component::new(owner: Owner, body: Body, render: Tmpl)` has no state, no handlers and no shadow root.
* `stores` are the `useStore` bindings among `state`, each with the store key it reads; `Component::store_key(&self, name) -> Option<&str>`. The plan writes them as `(stores (count "cart/count") ...)` and the JSON form as `"stores": [["count", "cart/count"]]`. A store binding is never island state: it reads the render's store every time.
* `owner` is what renders the component in the browser, if anything does; see [Owner](#owner). The plan writes it as `(owner fsr)`, `(owner react)` or `(owner vue)` and the JSON form as `"owner": "fsr"`. A plan older than format 5 reads as before: a component with no section is `React`, `(static)` is `Fsr`, `(tree)` is `React` and `(vue)` is `Vue`. In the JSON form a component with neither `owner` nor `hydrate` is `React`, `"hydrate": false` is `Fsr`, `true` and `"tree"` are `React` and `"vue"` is `Vue`. The renderer takes its markup rules from it; see [Frameworks](#frameworks).
* `shadow` is the shadow root an element template declares, read from its root `<template>` by `ShadowRoot::take` at build time. It is `None` for every other component and for an element template whose root is anything else, which the renderer wraps in an open root. See [ShadowRoot](#shadowroot).
* `scope` is a Vue component's scoped style; see [ScopedStyle](#scopedstyle). `None` for every other component.
* `state` names the body `let`s the browser can change, the `useState` and `useStore` bindings in order. `handlers` are the component's event handlers as bodies, for an island in server mode, each `Handler { event, body }` running with `$props`, `$state` and `$event` bound after the body's `let`s and returning an object whose keys are the state names it sets, with any `Act` it holds collected for the host to dispatch. An element binds one through an attribute `$on:<event>` holding the handler's index; an element whose handler did not lower carries `$unlowered` with the line and the reason; an element's React `key` is kept as `$key`; an element's `dangerouslySetInnerHTML` is kept as `$html` holding the `__html` expression; a placement of a custom element whose template sits under `elements/` carries `$shadow` holding the template's module. `render::HANDLER_ATTR`, `render::UNLOWERED_ATTR`, `render::KEY_ATTR`, `render::RAW_ATTR` and `render::SHADOW_ATTR` name them.

### Owner

What renders a component in the browser.

* `pub enum Owner { Fsr, React, Vue }`; derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, lowercase in the JSON form.
* `Fsr` is composition: the server renders it and nothing renders it again. A template with no state, no handlers and no context provider of its own is `Fsr`. A page or a layout holding any of them is split by the build so that it is `Fsr` too, unless the report names it under `kept`. `React` is a TSX component with state, handlers or a context provider, which the React adapter hydrates. `Vue` is a `.vue` file the build lowered, which the Vue adapter hydrates.
* `Owner::as_str(self) -> &'static str`: `fsr`, `react` or `vue`. `Owner::of(word: &str) -> Option<Owner>` reads one back.
* `Owner::hydrates(self) -> bool`: true for every owner but `Fsr`.

### ShadowRoot

The declarative shadow root the server writes around an element template.

* `pub struct ShadowRoot { pub mode: ShadowMode, pub delegates_focus: bool, pub clonable: bool, pub serializable: bool }`; derives `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`. The default is an open root with no options.
* `ShadowRoot::take(render: &mut Tmpl) -> Result<Option<ShadowRoot>, ShadowRootError>` reads an element template's root `<template>` as its shadow root and leaves that template's children as the render. The root is the render itself or the only child of a root `Fragment`. `Ok(None)` when the root is anything else. A `<template shadowrootmode>` deeper in the tree is markup and is left alone.
* `shadowrootmode` must be the literal string `open` or `closed`. `shadowrootdelegatesfocus`, `shadowrootclonable` and `shadowrootserializable` must be literal booleans, a bare attribute being `true`. Any other attribute is refused. So are a spread, a root `<template>` without `shadowrootmode` and a `<template shadowrootmode>` beside other root nodes or in a branch of a root `If`. See [ShadowRootError](#shadowrooterror).
* The JSON form is `{"mode": "closed", "delegates_focus": true}` with the false options left out. The plan writes `(shadow closed delegatesfocus)` after `(owner ...)`, naming each option that is true.

### ScopedStyle

A Vue component's scoped style.

* `pub struct ScopedStyle { pub id: String, pub slotted: bool }`; derives `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`. `id` is the `data-v-<hash>` attribute its rules select on. `slotted` is true when one of them is `:slotted()`.
* `ScopedStyle::slotted_attr(&self) -> String` is `<id>-s`, which the renderer writes on every element of the content the component's slots render when `slotted` holds. A `.vue` child placed in that content takes `id` on its root. Under `slotted` it takes `<id>-s` there too.
* The plan writes `(scope "data-v-1a2b" slotted)` after `(owner ...)` and `(shadow ...)`, `slotted` only when true. The JSON form is `"scope": {"id": "data-v-1a2b", "slotted": true}` with a false `slotted` left out.

### ShadowMode

* `pub enum ShadowMode { Open, Closed }`, `Open` by default and lowercase in the JSON form; derives `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`.
* `ShadowMode::of(mode: &str) -> Option<ShadowMode>` reads `open` or `closed`. `as_str(self) -> &'static str` writes them.

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

* `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Pow`.

### CompareOp

* `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`.

### LogicOp

* `And`, `Or`.

## 2. The JSON Form

### from_json

* `pub fn ast::from_json(text: &str) -> Result<Body, ParseError>`

### to_json

* `pub fn ast::to_json(body: &Body) -> String`, pretty printed.

## 3. The S-Expression Form

`snapfire_fsr_ir::sexpr` is the IR half of what a `plan.sexp` carries. The manifest half is `snapfire_fsr_plan::sexpr`.

### Sx

* `pub enum Sx { Sym(String), Str(String), List(Vec<Sx>), Interp(Box<Sx>) }`: one node of the syntax. A symbol and a string are distinct terms, so `nil` is the null literal and `"nil"` the string. `Interp` is `{expr}`, an expression where a template child is expected.

### parse

* `pub fn parse(src: &str) -> Result<Vec<Sx>, SexprError>`: every top-level form. `;` runs to the end of the line, `"..."` is a string and `|...|` a symbol whose bare spelling would not lex. `\n`, `\r` and `\t` are those characters; any other escape is the character after it, whole, so `\é` is `é`. A syntax error names the line it is on.

### print

* `pub fn print(forms: &[Sx]) -> String`: one form per line, each wrapped at 100 columns and indented by two.

### the IR conversions

Each is a pair and every pair round-trips: `parse(print(to(x))) == to(x)` and `from(to(x)) == x`.

* `expr_to_sx(&Expr) -> Sx` and `expr_from_sx(&Sx) -> Result<Expr, SexprError>`: a string literal is a quoted term, a variable a bare symbol, so `(class "wide")` is a field holding a literal.
* `tmpl_to_sx(&Tmpl) -> Sx` and `tmpl_from_sx(&Sx) -> Result<Tmpl, SexprError>`
* `stmt_to_sx(&Stmt) -> Sx` and `stmt_from_sx(&Sx) -> Result<Stmt, SexprError>`
* `component_to_sx(&Component) -> Sx` and `component_from_sx(&Sx) -> Result<Component, SexprError>`
* `component_sections(&Component) -> Vec<Sx>` and `component_from_sections(&[Sx]) -> Result<Component, SexprError>`: the sections without the `component` head, for a plan that puts the module id between the two.

### SexprError

* `pub struct SexprError`: what the text says it is, with the line for a syntax error and the form for a shape error.
* `SexprError::new(msg: impl Display) -> Self`

## 4. The Interpreter

### Interpreter

Runs a body. `Clone`; the default carries the system clock and the standard library.

* `Interpreter::default() -> Interpreter`
* `Interpreter::with_clock(clock: Arc<dyn Clock>) -> Interpreter`
* `Interpreter::with_extensions(self, extensions: Arc<Extensions>) -> Interpreter`: answers `Expr::Ext` from `extensions` in place of the standard library alone; `Interpreter::extensions(&self) -> &Arc<Extensions>` reads it back.
* `Interpreter::with_catalogs(self, catalogs: Option<Arc<Catalogs>>) -> Interpreter`: the message catalogs every `Ambient` carries; none by default. `Interpreter::catalogs(&self) -> Option<&Arc<Catalogs>>`.
* `Interpreter::render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: renders a lowered component with `props` bound as `$props`, byte for byte what the server renderer of the framework hydrating each component writes (see [Frameworks](#frameworks)), as `Rendered { html, islands, hoisted, whole }`. A `Tmpl::Island` renders its component apart, with the caller's children on the slot stack like a `Component` and leaves `ISLAND_MARK`, its index in `islands` and a NUL in `html` where it sits; `RenderedIsland { module, props, when, body }` holds the evaluated props and the island's own `Rendered`. A root `Slot` with no caller leaves `ROOT_SLOT`. Outside an island's body and outside a component that `hydrates`, a `Tmpl::Component` whose component hydrates renders as an island with no timing, since only its framework can render it again. A `Tmpl::Component` naming a module `library` does not hold is there an island whose body is empty. Inside one, a module `library` does not hold writes nothing and the island or root around it is not the server's: `whole` is false, the island holding it is written with an empty body and mounts fresh. `IrEvaluator` emits a root component whose render is not whole as a `Client` node with no `ssr`. An island's children render inside `render::CHILDREN_OPEN`, `<sf-s data-sf-children>`, closed after them, wherever its component places its `children`, which the mounter hands the component as its `children` without rendering them; nothing they hoist is kept and an island among them keys under the caller that wrote them. A component the server has no body for, a `.vue` file the build could not read among them, has that region as its whole body; with no children its body is empty. A `React` or `Vue` island whose render did not place its children this render writes its children after its markup inside `render::CHILDREN_HELD_OPEN`, `<template data-sf-children>`, closed as a template: inert to the parser and the scan, read by the mounter. They render in the caller's scope, as placed children do. `bind::rendered_nodes(&Rendered) -> Vec<Node>` turns the markup into nodes: raw pieces, `Node::Slot("content")` at `ROOT_SLOT` and, at an island, `Node::raw("<sf-s data-sf-island[ data-sf-when=\"…\"]>")`, a `Node::Client` whose `ssr` is the island's body and `Node::raw("</sf-s>")`. Synchronous: a component body holds no service call, so nothing here suspends. An expression with no `Call` in it is evaluated the same way wherever it appears; only an expression that calls a service goes through the async path.
* `Interpreter::render_island(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: `render_module` for an island placed with no children, as a page places one: a `Slot` it renders writes an empty `render::CHILDREN_OPEN` region where a root component's leaves `ROOT_SLOT`. `fsr test`'s `render` takes this for a component that `hydrates`.
* `Interpreter::render_module(&self, module: &str, component: &Component, props: &ValueMap, library: &Components) -> Result<Rendered, Fail>`: `render` for the component under `module`, which keys its hoisted values; `render` is this with an empty module. A `Tmpl::Component` or `Tmpl::Island` is a call into `library` by module id, so a component may render itself; components nested deeper than `render::MAX_CALLS`, 96 in a debug build and 512 in a release build, fail the render with an internal `Fail` naming the module rather than overflowing the thread's stack. `Rendered.hoisted: ValueMap` holds every `Expr::Hoist` value the markup took, keyed `<module>|<id>` or `<module>|<id>@<i>.<j>` under `For` iterations, the callers' loops first, so a component placed from a loop keys below the iteration that placed it. A `Tmpl::Component` with `keyed` set adds `c<id>` to the path for what the component renders, so two placements of one component key apart; the caller's children render under the caller's module and path, where the browser builds them. `interp::Step` is one entry of the path, `Iteration(usize)` or `Placement(u32)`; a key recorded twice with different values is removed rather than left wrong. An island starts its own table: `RenderedIsland.body.hoisted` and `RenderedIsland::mount_props(&self) -> ValueMap` is its props plus that table under `HOISTED_PROP`, `"$h"`, when it is not empty. `IrEvaluator` adds the same key to the root node's props. `interp::Hoists { module, path, table }` is the recorder: `key(id)` spells the key and `record(id, &value)` applies the collision rule. An element whose attributes carry `render::CHUNK_ATTR`, `$chunk`, with an integer id renders its children into a buffer of their own and records that markup as a string under the id before writing it; an attribute whose name starts with `$` is never printed, which also covers the lowerer's `$bound` mark. An element carrying `render::RAW_ATTR`, `$html`, writes that string to the document unescaped and renders no children, the way React refuses to have both; a null writes nothing.
* `HandlerRef { path: String, index: usize }` names the handler a step runs: `index` into the handlers of the component at `path`, which is empty for the island's own component (`HandlerRef::own(index)`) and otherwise the address a component rendered inside it carries, `c1` for a keyed placement or `0.c2` for one inside a loop, the `Hoists::path_key` of the render that reached it. `HandlerRef::parse` reads a token as an element binds it, `2` or `c1/2`; anything else is `None`.
* `Interpreter::island_step(&self, module, component, props, state, handler: Option<HandlerRef>, event: &Value, library) -> Result<Stepped, Fail>`: one round trip of an island in server mode. The body's `let`s run with `state` standing in for the component's state bindings, the handler at `handler` runs with `$props`, `$state` and `$event` bound and the object it returns is merged into the state for the keys the component names, a key that is a store binding written to the render's store instead and answered in `Stepped.store`, then the component renders from that state in server mode: `$on:` markers print as `data-sf-on="click:0 change:1"` and `$key` as `data-sf-key`, neither of which prints in a browser-mode render. `None` for `handler` renders as is. A component rendered inside the island through a keyed placement has an address, its `path_key`: its state bindings ride in the same map as `<address>/<name>`, its markers print as `<address>/<index>`, the address of the component that owns the handler wherever inside it the element sits, so an element in one of its loops never takes the iteration's address and a `HandlerRef` naming it is found by rendering once to that path, which gives its handler the props the island's render gave it; `NotFound` when the render reaches no such path or the component there has no such index. The answered `state` is what the render consumed, so an instance the render no longer places leaves no entry behind. `Stepped { state, rendered, acts }`: `acts` is every `Act` the handler ran, in order, as `(action id, input)` with the input evaluated where the statement stood, so it reads the props and the state as they were at that point; the interpreter dispatches nothing itself. A missing handler index is `Internal`.
* A `Tmpl::Island` in server mode renders its component the same way and `RenderedIsland { mode, state, reads, rendered_store, .. }` carries the mode, the values the state `let`s took, the store values the island was rendered from (the keys it reads that the store held, which `mount_props` writes as `$sv` on a browser-mode island) and the store keys the island reads, `render::store_keys(component, library)`: every `Expr::Store` in it and in the components it renders inline, sorted. `mount_props` adds the state under `render::STATE_PROP` (`$s`) and the keys under `render::READS_PROP` (`$sk`) and `rendered_nodes` writes `data-sf-mode="server"` on the region.
* An island whose `store_keys` include one of the pending keys its props carry under `snapfire_fsr_runtime::PENDING_PROP` is held: its component is not rendered while its children are. `RenderedIsland::awaits` lists those keys, which `mount_props` writes as `render::AWAITS_PROP` (`$aw`) in place of `$sv`. A root a framework hydrates is held the same way by `IrEvaluator`: a `Node::Client` with its slot regions and `$aw`.
* `render::ROOT_SLOT`: what a root component's own `Slot` writes into the markup, since it has no caller. `IrEvaluator` splits the markup there and emits a `Client` node whose `children` carry the pieces around a `Node::Slot("content")`, which is how a layout places its page.
* `Interpreter::run(&self, body: &Body, ctx: &RequestCtx, input: Option<Value>) -> impl Future<Output = Result<Outcome, Fail>>`. `input` is `None` for a loader; a body reads `Expr::Input` as `Value::Null` then. Session writes go to a draft copied from `ctx.session` at entry and are committed to the cell, key by key, only on success. A `SessionExtend` is applied to the cell after the draft, on success only.

### Outcome

* `pub struct Outcome { pub value: Value, pub written: Vec<String>, pub extended: Option<u64> }`
* `value` is `Value::Null` when the body ends without a `return`.
* `written` lists every session key set or deleted, in first-touch order, already committed.
* `extended` is the seconds the last `SessionExtend` named, already applied to the cell through `SessionCell::extend`; `None` when the body ran none.

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
* `ext::number`, `ext::text`, `ext::text_opt` and `ext::option` read an argument by index for an implementation: a number (`Int`, `UInt`, `F32` or `F64` as `f64`), a string, an optional string and a field of an optional options object, each `Internal` naming the extension on a wrong type. They are `snapfire_fsr_core::ext`'s, re-exported here.

### Reach

Re-exported from `snapfire_fsr_core::ext`. `STANDARD` and `standard_reach` are this crate's.

* `pub enum Reach { Render, Body }`, `Copy`, `Eq`; `as_str` is `render` or `body`.
* `Render`: pure, both sides, callable from every site. `Body`: server only, callable from a body, a handler and middleware; the lowerer refuses it on a component's render path.
* `pub const STANDARD: &[(&str, &str, Reach)]`: module, member and reach of every standard member; `standard_reach(module, name) -> Option<Reach>` looks one up. The registry `Extensions::standard` builds and the lowerer's checks are both taken from it.

### Ambient

Re-exported from `snapfire_fsr_core::ext`.

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

Message tables by locale, `catalog::Catalogs`, re-exported from `snapfire_fsr_core::ext` with `Table`. `Clone`, `Default`, `Eq`. `pub type Table = BTreeMap<String, String>`.

* `Catalogs::from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Catalogs`: holds every locale's table merged over the default locale's, so a key a locale lacks reads as the default's and each merged table as JSON.
* `is_empty(&self) -> bool`; `default_tag(&self) -> &str`; `rows(&self) -> Vec<(String, usize)>`: each locale with how many keys its own table held.
* `table(&self, tag: &str) -> Option<&Arc<Table>>` and `json(&self, tag: &str) -> Option<Arc<str>>`: the merged table for `tag`, the default locale's when `tag` has none, `None` when neither exists.
* `lookup(&self, tag: &str, key: &str) -> Option<&str>`.

### Frameworks

The frameworks an application vendors, each at the major whose server markup the renderer writes.

* `pub struct Frameworks { pub react: Option<ReactMajor>, pub vue: Option<VueMajor> }`; `Default` vendors nothing. Defined in `render` and re-exported at the root.
* `Interpreter::with_frameworks(self, frameworks: Frameworks) -> Interpreter` and `Interpreter::frameworks(&self) -> Frameworks`.
* A component a vendored framework hydrates renders under that framework's rules at the vendored major. A component hydrated by a framework the application does not vendor renders as plain markup. An `Fsr` component keeps its caller's rules. At the top of a render those are plain markup.

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

* A head tag React 19 hoists is one of two kinds. A `<title>` or a `<meta>` without `itemProp` is hoisted. A `<link>` is hoisted when it has a `rel`, a non-empty `href` and no load or error handler; a stylesheet `<link>` also needs a `precedence` and no `disabled`.
* Under Vue's rules, checked against `@vue/server-renderer` 3.5, markup is what `ssrRender` writes: text and attributes escape `"`, `&`, `'`, `<` and `>`; adjacent text runs are joined with no separator comment; an interpolation is `toDisplayString`, a boolean its word, `null` nothing, an array or an object its two-space JSON; a void element closes with `>`; attribute names are written as given; `class` is always written, normalised from a string, an array or an object of truthy keys; `style` is always written, a string as it stands and an object or an array as `name:value;` pairs with camelCase hyphenated and a number bare; a boolean attribute is bare when truthy or the empty string; `true` on any other attribute is bare, a string or a number is valued and anything else absent. A multi-root component is wrapped in `<!--[-->` and `<!--]-->`, so is every `v-for` list and each item that is not one element, a branch that is not one element and every slot; a `v-if` chain with no branch taken writes `<!---->`. Nothing hoists into the head.
* `className` on a custom element is written `class` under every set of rules.
* `render::prepare` bakes a literal open tag only when every set of rules prints it alike. It never bakes a `<title>`, `<meta>` or `<link>`.
* An element carrying `render::SHADOW_ATTR`, `$shadow`, holding a module id writes that component after its open tag under plain markup, with the element's other attributes as its props. The component's `shadow` gives the wrapping `<template>`: its `shadowrootmode` and each option that is true as a bare attribute. With no `shadow` the wrapper is `<template shadowrootmode="open">`. Nothing rendered inside it is recorded among an island's hoisted values, since the island's browser half never renders a shadow root. Its children follow. A non-scalar attribute on such an element reaches the template and is not written on the host.

### ReactMajor

* `pub enum ReactMajor { V18, V19 }`; `Default` is `V18`. Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`.
* `ReactMajor::of(version: &str) -> Option<ReactMajor>`: the major of a version such as `18.3.1` or `19.3.0`; `None` for any other major.
* `ReactMajor::number(self) -> u32`
* `ReactMajor::ALL: [ReactMajor; 2]`, oldest first.

### VueMajor

* `pub enum VueMajor { V3 }`; `Default` is `V3`. Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`.
* `VueMajor::of(version: &str) -> Option<VueMajor>`: the major of a version such as `3.5.13`; `None` for any other major.
* `VueMajor::number(self) -> u32`
* `VueMajor::ALL: [VueMajor; 1]`

## 5. Evaluation Rules

### Reads

* `Param(name)` and `Query(name)` are `Value::Str` or `Value::Null` when absent.
* `Locale` is the request's locale tag, `Value::Null` under a context with none; `Path` is the path the request matched, `Value::Str` and empty under a context with none. `Document` is the path of the page the document is showing, `RequestCtx::document` when an intercepted render set it and `Path` otherwise; the renderer reads it off the `$document` prop as it reads `Path` off `$path`. The plan spells it `(document)`. `Origin` is `document.origin` as the host checked it at boot, `Value::Str` or `Value::Null`; it is one value for the whole deployment, so it is constant the way `Config` is rather than per-request the way `Host` is. The plan spells it `(origin)`.
* `Session(key)` reads the draft, so a body sees its own earlier writes; `Value::Null` when absent.
* `Identity(path)` walks `{ subject, claims }` from the session's identity; `Value::Null` when anonymous or when a step is missing.
* `Input` is the value passed to `run`; `Now` is `Value::Int` from the clock.
* `Var(name)` is the innermost binding; an unbound name is `Internal`.

### Truthiness

`Null`, `false`, integer and float zero and the empty string are false. Every other value, including an empty `Seq` or `Map`, is true. `Coalesce` substitutes on `Null` only. `Logic` returns the deciding operand, as JavaScript does.

### Operators

* `Arith` on two `Int` stays `Int`; integer overflow and division by zero are `Internal`. `Add` with a `Str` on either side concatenates the other operand as `String(x)` writes it. Any other pair of numbers, strings, booleans and `Null` is read through JavaScript's `ToNumber` and yields `F64`: `Null` is 0, a boolean 0 or 1 and a string its numeric literal, empty as 0 and anything else as NaN. An array or an object operand is `Internal`.
* `Compare` orders two numbers of any representation by value, two `Str` by code unit and two `Bool` as 0 and 1. `Eq` and `Ne` between values of different kinds are false and true, as `===` and `!==` are. An ordering between other scalars reads both through `ToNumber`, so `"2" < 10` is true and anything against NaN is false. A comparison involving an array or an object is `Internal`.
* `Template` concatenates the string form of each part, per `Str`.
* `Field` on a non-map is `Value::Null`. `Index` accepts a `Map` with a `Str` key or a `Seq` with an `Int`, `UInt` or integral `F64` index, out of range reads `Value::Null`; `Index` on `Null` is `Null`; other targets are `Internal`.

### Conversions

* `Str` renders `Null` as `null`, booleans, integers and strings as themselves, integral floats without a fraction and the infinities as `Infinity` and `-Infinity`; a collection is `Internal`.
* `Num` is JavaScript's `Number(x)`: `F64` from integers, floats, booleans, `Null` and strings, an unparseable string as NaN; a collection is `Internal`.
* `BigInt` produces `Int` from integers, integral floats, booleans and parseable strings; a fractional float or an unparseable string is `Invalid`.

### Builtins

* `Map`, `Filter`, `Find`, `FindIndex`, `Some`, `Every` and `FlatMap` apply a lambda `(item, index, array)` over a `Seq`; `Reduce` applies `(acc, item, index, array)` from `init`. The array is passed only to a lambda naming more than two parameters. A non-`Seq` operand is `Internal`. `Find` yields `Value::Null` when nothing matches. `FlatMap` spreads a `Seq` result one level and keeps any other.
* `Sort` merges stably; a comparator's answer is read as a number, NaN and a non-number as 0. With no comparator each item is compared as `String(x)` by UTF-16 code units, so `[10, 9, 1]` sorts to `[1, 10, 9]` as it does in JavaScript.
* `Entries` yields a `Seq` of two-element `Seq` pairs in insertion order; `Keys` and `Values` likewise; all three require a `Map`.
* `Length` counts `Seq` items, `Str` characters or `Map` entries, as `F64`, since a TypeScript `number` is a float and a `bigint` is an `Int`.
* `Builtin::Omit` takes a `Map` and string keys and yields the map without those keys, the rest of a destructuring; `Null` reads as an empty map and any other first argument is `Internal`.
* `Builtin::StartsWith` and `Builtin::EndsWith` take two strings and yield a `Bool`. Every string starts with and ends with the empty one.
* `Builtin::Split` takes a subject and a separator and yields a `Seq` of strings. A separator the subject does not hold gives one piece, a leading or trailing separator keeps its empty piece; an empty separator is `Internal`: JavaScript splits one into UTF-16 code units, which the value model holds no half of. It refuses to build more than a million pieces, for the same reason `Repeat` and `Range` have bounds.
* `Builtin::Replace` takes a subject, a string pattern and a replacement, yielding the subject with the **first** occurrence replaced, as JavaScript's `String.prototype.replace` does with a string pattern. The replacement carries JavaScript's substitutions: `$$` is one dollar, `$&` the match, `` $` `` the text before it and `$'` the text after. A string pattern has no capture groups, so `$1` stays the two characters it is written as. Given a regular expression as its pattern it replaces the first match (every match with `g`) and the replacement also reads `$1` to `$99` and `$<name>`, a group that did not take part as nothing.
* `Builtin::Slice`, `At`, `IndexOf` and `Concat` take an array or a string; `Reverse` an array; `PadStart`, `PadEnd` and `Substring` a string. Indices follow JavaScript's `ToIntegerOrInfinity`, negative counting from the end where JavaScript's does. A string's indices count UTF-16 code units. A cut through a surrogate pair is `Internal`, since the value model holds no half of one. `IndexOf` over an array compares numbers by value whatever their width and anything else by equality. `PadStart` and `PadEnd` refuse to build more than `Repeat`'s bound.
* `Builtin::Json` is `JSON.stringify(value, null, indent)`: maps in insertion order, a non-finite number as `null`, an `Int`, which only a `bigint` produces, `Internal` as JavaScript throws, an indent of a number capped at 10 spaces or a string's first 10 characters.
* A regular expression is a map holding `jsregex::PATTERN` and `jsregex::FLAGS`. `jsregex::translate` rewrites its source for the `regex` crate so `\d`, `\w` and `\b` are ASCII and `.` stops at every line terminator without `s`. It refuses a backreference, lookaround and the `y` and `v` flags; `jsregex::compiled` caches what it builds. `Split` with one keeps the groups between the pieces as JavaScript does. `RegexTest` is `re.test(s)` without `lastIndex`, so `g` changes nothing. `Match` is the first match and its groups (every whole match with `g`) or null for none. `MatchAll` is each match with its groups and is `Internal` without `g`. `Search` is the UTF-16 index of the first match, -1 for none. `ReplaceAll` replaces every occurrence of a string, `$` substitutions included.
* `ReplaceWith(subject, pattern, f)` calls `f` with the match, each group, the UTF-16 offset and the subject. It writes `String` of what `f` returns at every match of a regular expression with `g` and at the first match otherwise.
* `Builtin::DateMs` reads a number as milliseconds since the epoch and an ISO 8601 string as `time.parse` does, anything else as NaN. `DatePart` takes the milliseconds and a part name (`year`, `month` from 0, `date`, `day` from Sunday as 0, `hours`, `minutes`, `seconds` or `milliseconds`) in UTC, NaN for an invalid date. `IsoString` writes `YYYY-MM-DDTHH:mm:ss.sssZ` and is `Internal` for an invalid date, as JavaScript throws.
* `Builtin::FromEntries` takes `[key, value]` pairs, each key as `String(key)`, a later key replacing an earlier one in place. `Unique` keeps an array's first occurrence of each item by the equality `IndexOf` uses. `HasKey` is whether a map holds `String(key)`. `FormEncode` writes an object's entries, `[key, value]` pairs or a query string as `URLSearchParams` does: letters, digits and `*-._` as they are, a space as `+`, every other byte `%XX`.
* `Builtin::Pow`, `Sqrt`, `Trunc` and `Sign` are the `Math` functions; `ArithOp::Pow` is `**`, an `Int` power of two `Int`s as a `bigint` computes it. `MinOf` and `MaxOf` take one array, `Infinity` and `-Infinity` when it is empty.

### Calls

* Arguments evaluate in order; an argument whose value is `Value::Null` is omitted from the `ValueMap` sent.
* The call goes through `RequestCtx::services`. A `ServiceError` becomes a `Fail` with the same kind and message.

### Session writes

* `SessionSet` with an empty path replaces the key. With a path it walks `Map` steps by `Str` key, creating maps where a step is `Null`, plus `Seq` steps by `Int` or integral `F64` index within bounds.
* `SessionDelete` with an empty path removes the key; with a path it removes the last step's entry from its `Map` and is a no-op where the path does not exist.
* Both mark the key in `written`.
* `SessionExtend` evaluates `seconds`, which must be a positive whole number, `Int` or an integral `F64`, else the body fails with `Internal`. The last one the body ran is applied to the cell after the draft commits, so a failed body leaves the end where it was. It marks no key and does not dirty the cell.

### Guards

* Before the body runs, top-level guards are scanned in order. A guard whose condition reads no `Var` and contains no `Call` is evaluated immediately. The scan stops at the first `If`, `ForOf`, `SessionSet`, `SessionDelete`, `SessionExtend` or `Act`.
* Every guard also runs in sequence at its own position.
* An unknown kind name is `Internal`.

### Parallel lets

* A run of consecutive `Let` statements in one block, none of which reads a name bound earlier in the run, is evaluated together when at least two of them contain a `Call`. Each evaluates over a snapshot of the current scope and draft, which is safe because writes are statements, never expressions.
* Any other statement is evaluated in sequence, as is any `Let` that reads an earlier name in the run.

## 6. Runtime Adapters

### IrSource

A body answering a data source id. Implements `snapfire_fsr_runtime::DataSource`.

* `IrSource::new(id: impl Into<String>, body: Body) -> IrSource`
* `IrSource::with_interpreter(self, interpreter: Interpreter) -> IrSource`
* `load` runs the body with no input and returns the `Value::Map` it returned as `Data`. A non-map return or a `Fail` is a `LoadError` carrying the id and the message.

### IrPaths

A lowered `paths` naming the parameter sets a route prerenders. Implements `snapfire_fsr_runtime::Paths`.

* `IrPaths::new(source_id: impl Into<String>, body: Body) -> IrPaths`
* `IrPaths::with_interpreter(self, interpreter: Interpreter) -> IrPaths`
* `paths` runs the body with no input and reads the `Value::Seq` it returned as one `Params` per `Value::Map`, a string or a number per key. A non-list return, an entry that is not an object, a value of another kind or a `Fail` is a `LoadError` carrying the id and the message.

### IrStore

A lowered `store` export. Implements `snapfire_fsr_runtime::Seeds`.

* `IrStore::new(source_id: impl Into<String>, body: Body) -> IrStore`
* `IrStore::with_interpreter(self, interpreter: Interpreter) -> IrStore`
* `seed` runs the body with the segment's data as `Input` and returns the `Value::Map` it returned. A non-map return or a `Fail` is a `LoadError`.
* `keys` is every field name a `return` of the body can hold, through branches, loops and a returned `let`: `None` when a return is anything but an object of named fields, a spread or a computed key among them, since its keys are then the data's.

### IrAction

A body answering an action id. Implements `snapfire_fsr_runtime::ActionHandler`.

* `IrAction::new(body: Body) -> IrAction`
* `IrAction::with_interpreter(self, interpreter: Interpreter) -> IrAction`
* `call` runs the body with the submitted value as `Input` and returns what it returned. A `Fail` is an `ActionError` with the kind preserved.

## 7. Error Handling

### Fail

Re-exported from `snapfire_fsr_core::ext`, with `FailureKind`.

* `pub struct Fail { pub kind: FailureKind, pub message: String }`; derives `Debug`, `Clone`, `PartialEq`, implements `std::error::Error` and `Display` as `{kind}: {message}`.
* `Fail::new(kind: FailureKind, message: impl Into<String>) -> Fail`; `Fail::internal(message: impl Into<String>) -> Fail`, kind `Internal`.
* A guard yields its named kind. A service error keeps its kind. `Num` and `BigInt` of unparseable input are `Invalid`. Type mismatches, unbound names, overflow and structural misuse are `Internal`.

### ParseError

* `pub struct ParseError(serde_json::Error)`, returned by `from_json`; `Display` is `malformed IR: {inner}`.

### ShadowRootError

Returned by `ShadowRoot::take`. Derives `Debug`, `Clone`, `PartialEq`, `Eq` and implements `std::error::Error`; each `Display` names the root `<template>` and what to write instead.

* `NoMode`: the root `<template>` has no `shadowrootmode`.
* `NotLiteral { name: String, expected: &'static str }`: `shadowrootmode` or an option is an expression and not a literal of its type.
* `Mode { mode: String }`: a `shadowrootmode` that is neither `open` nor `closed`.
* `Attribute { name: String }`: an attribute the parser would drop with the template, named as the template wrote it, so `key` and `dangerouslySetInnerHTML` rather than their markers.
* `Spread`: a spread on the root `<template>`.
* `NotAlone`: a `<template shadowrootmode>` beside other root nodes or in a branch of a root `If`.
