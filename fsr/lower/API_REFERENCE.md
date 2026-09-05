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
* [2. The Recognised Language](#2-the-recognised-language)
  * [Modules](#modules)
  * [Statements](#statements)
  * [Expressions](#expressions)
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
* Lowers every `export const <name> = action(<arrow>)` or `action<T>(<arrow>)` in file order; other exports are skipped. The arrow must have a block body. The first `Residue` in any action fails the whole call.

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

### Residue

* `pub struct Residue { pub file: String, pub line: usize, pub column: usize, pub message: String }`
* `Display` is `{file}:{line}:{column}: {message}`.
* `line` and `column` are one-based and point at the construct, not at the statement that contains it.
