# API Reference: snapfire_fsr_lower

The recogniser that lowers a TypeScript loader or actions module to the IR.

## Contents

* [1. Lowering](#1-lowering)
  * [lower_loader](#lower_loader)
  * [lower_actions](#lower_actions)
  * [lower_loader_with, lower_actions_with and lower_meta_with](#lower_loader_with-lower_actions_with-and-lower_meta_with)
  * [SessionDefaults](#sessiondefaults)
  * [LoweredAction](#loweredaction)
  * [read_schema](#read_schema)
  * [read_session_defaults](#read_session_defaults)
  * [SchemaType](#schematype)
  * [ComponentSet](#componentset)
  * [ALIASES, STD_SPECIFIER and EXT_DIR](#aliases-std_specifier-and-ext_dir)
* [2. The Recognised Language](#2-the-recognised-language)
  * [Modules](#modules)
  * [Statements](#statements)
  * [Expressions](#expressions)
  * [Extensions](#extensions)
  * [Hoisting](#hoisting)
  * [Handlers](#handlers)
  * [Lambdas](#lambdas)
  * [Schemas](#schemas)
* [3. Error Handling](#3-error-handling)
  * [LowerError](#lowererror)
  * [Residue](#residue)

## 1. Lowering

### lower_loader

* `pub fn lower_loader(file: &str, source: &str) -> Result<Body, LowerError>`
* `file` is used in diagnostics only. The module must export `load` as a function declaration or an arrow with a block body.

### lower_actions

* `pub fn lower_actions(file: &str, source: &str) -> Result<Vec<LoweredAction>, LowerError>`
* Lowers every `export const <name> = action(<function>)` or `action<T>(<function>)` in file order; other exports are skipped. The function is an arrow with a block body, an arrow with an expression body, which lowers to a `Return` of the expression, or a function expression. An `action(...)` of anything else is a `Residue` naming the export. The first `Residue` in any action fails the whole call.

### lower_loader_with, lower_actions_with and lower_meta_with

* `pub fn lower_loader_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError>`
* `pub fn lower_actions_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredAction>, LowerError>`
* As `lower_loader` and `lower_actions`, with every read of a session key that has a default lowered to `Coalesce(Session(key), default)`.
* `pub fn lower_meta_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the module's exported `meta`, a function of `{ data }` whose `data` lowers to `Expr::Input`, as a block body or an arrow whose expression body becomes its `Return`. `None` when the module exports no `meta`; a `meta` that is not a function is a `Residue`.

### SessionDefaults

* `pub type SessionDefaults = Vec<(String, Expr)>`, one literal expression per session key.

### LoweredAction

* `pub struct LoweredAction { pub export: String, pub input: Option<String>, pub body: Body }`
* `input` is the identifier of the first type argument when it is a plain type reference, otherwise `None`.

### read_schema

* `pub fn read_schema(file: &str, source: &str) -> Result<Vec<SchemaType>, LowerError>`
* Reads every `export interface` and `export type X = "a" | "b"` in file order; non-exported declarations and everything else are skipped.

### read_session_defaults

* `pub fn read_session_defaults(file: &str, source: &str) -> Result<SessionDefaults, LowerError>`
* Reads `export const defaults = { key: literal, ... }`; each value is lowered as an expression with no context, so anything beyond literals, object and array literals is residue. Empty when the module declares no `defaults`.

### SchemaType

* `pub struct SchemaType { pub name: String, pub def: TypeDef }`, with `TypeDef` from `snapfire_fsr_service`.

### ComponentSet

The cursor over one application: parsed files, lowered components and the resolution that follows imports. `component::ComponentSet`.

* `ComponentSet::new(app: &Path) -> ComponentSet`; `with_defaults(self, defaults: SessionDefaults) -> ComponentSet`: the session defaults every body lowers with.
* `lower(&mut self, module: &str) -> Result<(), LowerError>`: lowers `path#export` and everything it renders into `components`; a module already lowered is not read again.
* `lower_loader(&mut self, file: &str) -> Result<Body, LowerError>`, `lower_meta` and `lower_store` (`Result<Option<Body>, LowerError>`), `lower_actions` (`Result<Vec<LoweredAction>, LowerError>`), `lower_handlers` (`Result<Vec<LoweredHandler>, LowerError>`) and `lower_middleware` (`Result<Body, LowerError>`): the body lowerers over a file under the app, each following the module-level names the body calls through the same resolution a component uses, so a loader calls the helper a component calls. A name that cannot be followed is the residue the free functions give.
* `lower_extensions(&mut self, file: &str) -> Result<Vec<(String, String)>, LowerError>`: lowers every export of a module under `ext/`, `(file#export, kind)` with the kind `lowered`, `native render` or `native body`; an export that does not lower is `LowerError::Extension`.
* `natives: Vec<(String, Reach)>`: every native pair declared so far, `module.member` and reach. `remaining: Vec<(String, String)>`: per lowered module, `file:line:column` of each render-path call candidate that was not hoisted and sits under no hoisted call, which the browser still makes.
* `components`, `layouts`, `slots`, `rewrites`, `pure` and `rewritten` as the hoisting section says.

### ALIASES, STD_SPECIFIER and EXT_DIR

* `pub const ALIASES: &[(&str, &str)]`: `@app/`, `@routes/`, `@src/`, `@schemas/`, `@generated/` and `@ext/` with the app directory each stands for; `resolve_specifier(from, specifier) -> Option<String>` expands one or joins a relative specifier to `from`'s directory, `None` for a bare specifier.
* `pub const STD_SPECIFIER: &str`, `@snapfire/fsr-client/std`: the module whose members lower to `Expr::Ext` and whose `native` declares a pair.
* `pub const EXT_DIR: &str`, `ext`: the directory whose modules are extensions.

## 2. The Recognised Language

### Modules

* The context parameter is the first parameter of the body: an identifier or an object pattern whose keys are `params`, `query`, `session`, `services`, `identity`, `input` or `now`, each optionally renamed with `key: local`. Any other key, a nested pattern or a rest element is residue.
* Imports, type declarations and non-action exports are ignored.
* In a component, `<Island when="visible">` with `Island` imported from `@snapfire/fsr-client/react` places its one component child as `Tmpl::Island`; a module-level `const Lazy = island(Chart, { when })` with `island` from the same module makes `<Lazy … />` the same. `when` must be written out as `"load"`, `"visible"` or `"idle"`; `<Island>` around an element, with any other attribute or with more than one child is residue.

### Statements

* `const x = e` or `let x = e` with one identifier binding; a destructuring or an uninitialised binding is residue.
* `if (c) fail("kind", "msg")`, with the call bare or in a one-statement block and no `else`, is a guard; any other `if`, with or without `else`, is a conditional whose branches are blocks or single statements.
* `for (const x of e) body`.
* `return e` or `return`.
* `session.key = e`, `session.key.sub = e`, `session.key[e] = e`, plus the same through `ctx.session`; `delete session.key[e]`, `delete session.key?.[e]` and `delete session.key.sub`.
* `fail("kind", "msg")` as a bare statement is a guard whose condition is `true`.
* Any other expression statement, typically an awaited call, is `Stmt::Expr`.
* `try`, `throw`, `while`, `for`, `for...in`, `switch`, nested functions, classes, `break`, `continue`, labels and bare blocks are residue.

### Expressions

* Parentheses, `await`, `as`, `!` non-null, `satisfies`, type assertions and optional chaining on a member are transparent, since a missing field already reads as `null`.
* Reads: `params.x`, `query.x`, `session.x`, `identity.x`, `identity.x.y`, `input`, `now`, plus the same under `ctx.`. A session read whose key has a default becomes `Coalesce(Session, default)`. A root used as a whole is residue, as is a computed key on a root.
* Literals: strings, numbers as `Lit::Float`, bigints as `Lit::Int`, booleans, `null`, `undefined` as `Lit::Null`. Regular expressions are residue.
* Template literals, object literals with shorthand, `key: value`, `[computed]: value` and spread entries, array literals with spread.
* `+ - * / %`, `=== !== == != < <= > >=`, `&& || ??`, `!`, unary minus and the conditional operator. Other operators, including `**`, bitwise operators, `in` and `instanceof`, are residue.
* `a.b` and `a["b"]` are field reads; `a[e]` is an index; `a.length` is `Expr::Length`.
* `String(e)`, `Number(e)`, `BigInt(e)`; `Object.entries(e)`, `Object.keys(e)`, `Object.values(e)`.
* `await services.<s>.<m>(args)` and `ctx.services.<s>.<m>(args)`, where `args` is absent or one object literal with shorthand or `key: value` entries. Argument values that lower to `null` are omitted at run time.
* `e.map(f)`, `e.filter(f)`, `e.find(f)`, `e.some(f)`, `e.every(f)`, `e.reduce(f, init)`.
* Any other call, `new`, an optional call, `this`, `++`, comma expressions, tagged templates, JSX, `yield` and assignment inside an expression are residue. A call to a name that is not a builtin names that name in the message.

### Extensions

* `X.m(args)` where `X` is a named import from `STD_SPECIFIER` lowers to `Expr::Ext { module: X, name: m, args }`; a member `standard_reach` does not know is residue naming it. A member whose reach is `body` on a component's render path is `LowerError::Reach` naming the line, a hard error the set never downgrades; the same call inside a handler, in a body, in middleware or in a helper lowered on its own is allowed, and the refusal follows an inlined helper to the render-path call that applies it, naming both. A `render` member is a hoist candidate like a helper call.
* A module-level `export const f = native("module.member", g?)` with `native` from `STD_SPECIFIER` declares a native pair: the name must be a string literal of two non-empty parts, `LowerError::Extension` otherwise; the reach is `render` with `g` and `body` without. The set records it in `natives` and a call `f(args)` lowers to `Expr::Ext` with the same reach rules.
* Every export of a module under `EXT_DIR` is lowered by `lower_extensions`; one that does not lower is `LowerError::Extension` carrying the residue. Elsewhere a helper that does not lower is residue at its call as before.

### Hoisting

* In a component, a call to a module-level helper (`Expr::Apply`), a `render` extension call (`Expr::Ext`) and a `toLocaleString` (`Builtin::LocaleNumber`) is wrapped as `Expr::Hoist` when its inputs are props only: none of its free variables reaches a `useState`, `useStore` or `useRef` binding or a name computed from one through a `const`, a `.map` parameter or a block `const`, and it reads no store key, no request and no service. A call inside a lambda body, under another hoist, or with such an input stays a plain call. A `Tmpl::For` whose `over` reads state taints its parameters.
* `ComponentSet.rewrites: Vec<hoist::Rewrite>` holds one entry per component with a surviving hoist: `file`, `module`, where the reader hook goes (`hoist::Hook::Block { after }` after a block body's `{`, or `Hook::Expression(range)` around an arrow's expression body), the surviving `sites` as `(id, byte range of the call)` and the `loops` as the byte ranges of the JSX `.map` callbacks holding them. `ComponentSet::rewritten(&self) -> Vec<(String, String)>` is every such file with `hoist::apply` over its source.
* `hoist::apply(source: &str, rewrites: &[&Rewrite]) -> String` splices from the end: prepends `hoist::IMPORT`, binds `const __sfh = __sfUseHoisted("<module>")` at the hook, replaces each call with `__sfh.r(<id>, () => (<call>))`, wraps each loop callback as `__sfh.l(<callback>)` and each chunk element as `__sfh.c(<id>, (__sfHtml) => <tag attrs dangerouslySetInnerHTML={__sfHtml} />, () => (<element>))`, braced when the element sits among JSX children. `hoist::decide(&mut Component, state: &[String]) -> Vec<u32>` is the value pass, unwrapping what does not qualify and returning the ids kept.
* Subtrees: every element with children is a candidate, marked by a `$chunk` attribute holding its id, and `hoist::chunks(&mut Component, state, pure: &HashMap<String, bool>) -> Vec<u32>` keeps the outermost ones that are static and do work, removing every other marker. A subtree is static when every expression in it is props only, no element in it carries `$bound` (a handler, a `ref` or a spread, which the lowerer marks), it is not `sf-s`, it holds no island and no slot and every component it renders is pure; it does work when it holds an interpolation, a branch, a loop, a binding, a component or a non-literal attribute, so literal markup is left to React. `ComponentSet.pure` records per module whether it is pure: no state, static all the way down. `hoist::static_tree(&Tmpl, pure) -> bool` is the static test with no state at all. `Rewrite.chunks` carries `(id, element range, opening tag range, among children)`.

### Handlers

* Every `on*` attribute of an element is lowered as a handler when it can be, whether or not the component is ever placed in server mode: an arrow, a function declared in the component or a `const` holding one, `useCallback` included, or `() => void f()` calling one. The body is `const`s, which bind, and calls to state setters, `setX(expr)` or `setX((prev) => expr)` with `prev` reading `X`; `e.preventDefault()` and `e.stopPropagation()` are dropped; the event parameter reads as `$event`, so `e.target.value` is `Field(Field(Var("$event"), "target"), "value")`. The handler returns an object of the state it set. A handler that sets no state, branches, calls an action or anything else leaves `$unlowered` on the element with the line and the reason and is otherwise the browser's.
* `<Island mode="server">` and `island(C, { mode: "server" })` set `Tmpl::Island.mode`; `"browser"` is the default and spells as none; anything else is residue. `Component.state` lists the `useState` and `useStore` bindings and `Component.handlers` the handlers lowered, indexed by the `$on:` attributes.

### Lambdas

* An arrow function in a builtin position. Parameters are identifiers, array patterns of identifiers or object patterns of shorthand identifiers; a pattern parameter is named `$<index>` and each element reads as an index or field of it.
* The body is one expression or a block whose only statement is a `return`.
* An arrow anywhere else is residue.

### Schemas

* An interface becomes `TypeDef::Record`; `extends`, type parameters, methods, index signatures and computed keys are residue.
* Field types: `string` is `Str`, `number` is `F64`, `bigint` is `I64`, `boolean` is `Bool`, `null` and `undefined` are `Null`; `T[]` and `Array<T>` are `List`; `Record<string, T>` is `Map`; `Uint8Array` is `Bytes` and the other typed arrays are `Array(kind)`; a bare name is `Named`.
* `T | null`, `T | undefined` and a `?` field are `Optional(T)`; a union of two real types, an inline object type, a literal outside a named union and a generic reference are residue.
* A type alias must be a union of string literals and becomes `TypeDef::Union` of unit variants.

## 3. Error Handling

### LowerError

* `Parse { file: String, message: String }`, with the parser's line, column and message in `message`.
* `MissingExport { file: String, export: String }`.
* `Residue(Residue)`, transparent in `Display`.
* `Reach(Residue)`: a `body` extension on a component's render path; `Display` is the residue followed by why it is refused.
* `Extension(Residue)`: an export under `ext/` that does not lower, or a `native` declaration the build cannot read; `Display` is the residue followed by the rule.

### Residue

* `pub struct Residue { pub file: String, pub line: usize, pub column: usize, pub message: String }`
* `Display` is `{file}:{line}:{column}: {message}`.
* `line` and `column` are one-based and point at the construct, not at the statement that contains it.
