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
* [3. Test Files](#3-test-files)
  * [lower_tests](#lower_tests)
  * [TestFile and Block](#testfile-and-block)
  * [TestCase and Mode](#testcase-and-mode)
  * [Step](#step)
  * [Assertion and Subject](#assertion-and-subject)
  * [Matcher and Pattern](#matcher-and-pattern)
  * [Mock, Answer and Binding](#mock-answer-and-binding)
  * [Target](#target)
* [4. Error Handling](#4-error-handling)
  * [LowerError](#lowererror)
  * [Residue](#residue)

## 1. Lowering

### lower_loader

* `pub fn lower_loader(file: &str, source: &str) -> Result<Body, LowerError>`
* `file` is used in diagnostics only. The module must export `load` as a function declaration or an arrow with a block body.

### lower_actions

* `pub fn lower_actions(file: &str, source: &str) -> Result<Vec<LoweredAction>, LowerError>`
* Lowers every `export const <name> = action(<function>)` or `action<T>(<function>)` in file order; other exports are skipped. The function is an arrow with a block body, a function expression or an arrow with an expression body, which lowers to a `Return` of the expression. An `action(...)` of anything else is a `Residue` naming the export. The first `Residue` in any action fails the whole call.

### lower_loader_with, lower_actions_with and lower_meta_with

* `pub fn lower_loader_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError>`
* `pub fn lower_actions_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredAction>, LowerError>`
* As `lower_loader` and `lower_actions`, with every read of a session key that has a default lowered to `Coalesce(Session(key), default)`.
* `pub fn lower_meta_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the module's exported `meta`, a function of `{ data }` whose `data` lowers to `Expr::Input`, as a block body or an arrow whose expression body becomes its `Return`. `None` when the module exports no `meta`; a `meta` that is not a function is a `Residue`.
* `pub fn lower_store_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the module's exported `store`, the same shape as `meta`.
* `pub fn lower_paths_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError>`: the module's exported `paths`, a function of the context or of nothing, returning the parameter sets a route prerenders. The context binds the way a loader's does, so `data` is not a name here. `None` when the module exports no `paths`; a `paths` that is not a function is a `Residue`. What the body may read is the app's check, not the lowerer's.

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
* `lower(&mut self, module: &str) -> Result<(), LowerError>`: lowers `path#export` and everything it renders into `components`; a module already lowered is not read again. A module met while it is still being lowered is a component that renders itself, directly or through others. It lowers to a `Tmpl::Component` naming it, which the renderer calls by id. Once the outermost call returns, every `hydrate` verdict and every entry of `pure` is brought to what the whole graph says: a component on a cycle hydrates when any component it renders inline has state or handlers. It is pure when every component on the cycle is.
* `lower_loader(&mut self, file: &str) -> Result<Body, LowerError>`, `lower_meta`, `lower_store` and `lower_paths` (`Result<Option<Body>, LowerError>`), `lower_actions` (`Result<Vec<LoweredAction>, LowerError>`), `lower_handlers` (`Result<Vec<LoweredHandler>, LowerError>`) and `lower_middleware` (`Result<Body, LowerError>`): the body lowerers over a file under the app, each following the module-level names the body calls through the same resolution a component uses, so a loader calls the helper a component calls. A name that cannot be followed is the residue the free functions give.
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
* `<X.Provider>` where `X` is a module-level `const` bound to `createContext(...)` from `react`, in the file or followed through its imports, lowers to `Tmpl::Fragment` of its children with the `value` dropped and makes the component hydrate; `X` bound to anything else is residue naming the tag. `useContext` and `X.Consumer` stay residue.
* `export default tree(Layout)` with `tree` from `@snapfire/fsr-client/react` lowers `Layout` as the default export and sets `hydrated_by` to `HydratedBy::ReactTree`, whatever the layout holds. The module must be one of `ComponentSet::layouts`; on any other module the call is residue, as is a call with anything but one identifier.
* `<Link>` from the same module lowers to an `<a>` carrying `data-sf-link` and an `aria-current` computed against `Expr::Path`. With `current="document"` written it is computed against `Expr::Document` and the anchor carries `data-sf-current="document"` beside it; `current="url"` is the default. `match` and `current` must be written out; any other value is residue.
* In a component, `<Island when="visible">` with `Island` imported from `@snapfire/fsr-client/react` places its one component child as `Tmpl::Island`; a module-level `const Lazy = island(Chart, { when })` with `island` from the same module makes `<Lazy … />` the same. `when` must be written out as `"load"`, `"visible"` or `"idle"`; `<Island>` around an element, with any other attribute or with more than one child is residue.

### Statements

* `const x = e` or `let x = e` with one identifier binding; a destructuring or an uninitialised binding is residue.
* `if (c) fail("kind", msg)`, with the call bare or in a one-statement block and no `else`, is a guard; the kind is a string literal, since it is matched at build time, and the message is any expression, a template naming the value that failed for one; any other `if`, with or without `else`, is a conditional whose branches are blocks or single statements.
* `for (const x of e) body`.
* `return e` or `return`.
* `session.key = e`, `session.key.sub = e`, `session.key[e] = e`, plus the same through `ctx.session`; `delete session.key[e]`, `delete session.key?.[e]` and `delete session.key.sub`.
* `session.extend(e)` or `ctx.session.extend(e)` as a statement is `Stmt::SessionExtend` in an action or middleware, `e` the seconds. In a loader or any other body it is residue naming the two, since a loader runs on every navigation; more or fewer than one argument is residue.
* `fail("kind", msg)` as a bare statement is a guard whose condition is `true`.
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
* `e.map(f)`, `e.filter(f)`, `e.find(f)`, `e.findIndex(f)`, `e.some(f)`, `e.every(f)`, `e.reduce(f, init)`.
* The pure builtins, each an `Expr::Builtin`: `Math.round`, `Math.floor`, `Math.ceil`, `Math.abs`, `Math.min`, `Math.max`, `Array.from({ length })`, `encodeURIComponent(e)`, `e.toFixed(n)`, `e.repeat(n)`, `e.join(sep)`, `e.trim()`, `e.toUpperCase()`, `e.toLowerCase()`, `e.includes(x)`, `e.startsWith(p)`, `e.endsWith(p)`, `e.split(sep)`, `e.replace(from, to)` and `e.toLocaleString("en-US")`. The receiver is the first argument and the call's own arguments follow it.
* `split`, `startsWith` and `endsWith` take exactly one argument and `replace` exactly two; a second argument, JavaScript's `limit` and `position`, is residue rather than one the interpreter would ignore. `split("")` written as a literal is residue naming why. A `replace` over a regular expression is residue through the regular expression literal itself.
* Any other call, `new`, an optional call, `this`, `++`, comma expressions, tagged templates, JSX, `yield` and assignment inside an expression are residue. A call to a name that is not a builtin names that name in the message.

### Extensions

* `X.m(args)` where `X` is a named import from `STD_SPECIFIER` lowers to `Expr::Ext { module: X, name: m, args }`; a member `standard_reach` does not know is residue naming it. A member whose reach is `body` on a component's render path is `LowerError::Reach` naming the line, a hard error the set never downgrades; the same call inside a handler, in a body, in middleware or in a helper lowered on its own is allowed and the refusal follows an inlined helper to the render-path call that applies it, naming both. A `render` member is a hoist candidate like a helper call.
* A module-level `export const f = native("module.member", g?)` with `native` from `STD_SPECIFIER` declares a native pair: the name must be a string literal of two non-empty parts, `LowerError::Extension` otherwise; the reach is `render` with `g` and `body` without. The set records it in `natives` and a call `f(args)` lowers to `Expr::Ext` with the same reach rules.
* Every export of a module under `EXT_DIR` is lowered by `lower_extensions`; one that does not lower is `LowerError::Extension` carrying the residue. Elsewhere a helper that does not lower is residue at its call as before.

### Hoisting

* In a component, a call to a module-level helper (`Expr::Apply`), a `render` extension call (`Expr::Ext`) and a `toLocaleString` (`Builtin::LocaleNumber`) is wrapped as `Expr::Hoist` when its inputs are props only: none of its free variables reaches a `useState`, `useStore` or `useRef` binding or a name computed from one through a `const`, a `.map` parameter or a block `const` and it reads no store key, no request and no service. A call inside a lambda body, under another hoist or with such an input stays a plain call. A `Tmpl::For` whose `over` reads state taints its parameters.
* `ComponentSet.rewrites: Vec<hoist::Rewrite>` holds one entry per component with a surviving hoist: `file`, `module`, where the reader hook goes (`hoist::Hook::Block { after }` after a block body's `{` or `Hook::Expression(range)` around an arrow's expression body), the surviving `sites` as `(id, byte range of the call)`, the keyed `placements` as `(id, element range, among children)` and the `loops` as the byte ranges of the JSX `.map` callbacks holding them. A placement is keyed when the component it places keys a hoist, a chunk or a region anywhere below it and it does not sit inside a kept chunk; the lowerer sets `Tmpl::Component.keyed` on it so the renderer extends the key path the same way. A component met in its own cycle counts as keyed. `ComponentSet::rewritten(&self) -> Vec<(String, String)>` is every such file with `hoist::apply` over its source.
* `hoist::apply(source: &str, rewrites: &[&Rewrite]) -> String` splices from the end: prepends `hoist::IMPORT`, binds `const __sfh = __sfUseHoisted("<module>")` at the hook, replaces each call with `__sfh.r(<id>, () => (<call>))`, wraps each loop callback as `__sfh.l(<callback>)`, each keyed placement as `__sfh.p(<id>, <element>)` and each chunk element as `__sfh.c(<id>, (__sfHtml) => <tag attrs dangerouslySetInnerHTML={__sfHtml} />, () => (<element>))`, braced when the element sits among JSX children. `hoist::decide(&mut Component, state: &[String]) -> Vec<u32>` is the value pass, unwrapping what does not qualify and returning the ids kept.
* Subtrees: every element with children is a candidate, marked by a `$chunk` attribute that holds its id. `hoist::chunks(&mut Component, state, pure: &HashMap<String, bool>) -> Vec<u32>` keeps the outermost ones that are static and do work, removing every other marker. A subtree is static when every expression in it is props only, no element in it carries `$bound` (a handler, a `ref` or a spread, which the lowerer marks), it is not `sf-s`, it holds no island and no slot and every component it renders is pure; it does work when it holds an interpolation, a branch, a loop, a binding, a component or a non-literal attribute, so literal markup is left to React. `ComponentSet.pure` records per module whether it is pure: no state, static all the way down. `hoist::static_tree(&Tmpl, pure) -> bool` is the static test with no state at all. `Rewrite.chunks` carries `(id, element range, opening tag range, among children)`.

### Handlers

* Every `on*` attribute of an element is lowered as a handler when it can be, whether or not the component is ever placed in server mode: an arrow, a function declared in the component or a `const` holding one, `useCallback` included or `() => void f()` calling one. The body is `const`s, which bind, calls to state setters, `setX(expr)` or `setX((prev) => expr)` with `prev` reading `X`, plus calls to actions, `save(input)` or `void save(input)` where `save` is a module-level `const save = action("id")` with `action` imported from `CLIENT_SOURCE`, which lower to `Stmt::Act { action: id, input }` in the order written, the input an empty object when the call has no argument; `e.preventDefault()` and `e.stopPropagation()` are dropped; the event parameter reads as `$event`, so `e.target.value` is `Field(Field(Var("$event"), "target"), "value")`. The handler returns an object of the state it set, empty when it only called actions. An `if`, with `else` or `else if`, holds the same statements: a key set under a branch is `Ternary(reach, value, prior)`, where `reach` is the branch's condition joined by `And` to the conditions around it and `prior` is what the patch already held for the key or `Var(key)`, the state as it stands; an action called under a branch is `Stmt::If { cond: reach, then: [Act], else: [] }`; a `const` declared under a branch is hoisted as `Let` under a name of its own, `rest$1`, its value `Ternary(reach, init, Null)`; a bare `return` in a branch makes the statements after the `if` reachable only where that branch did not run. A handler that neither sets state nor calls an action, that loops or that calls anything else leaves `$unlowered` on the element with the line and the reason and is otherwise the browser's.
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

## 3. Test Files

`snapfire_fsr_lower::testing`: reads a `*.test.ts` into the cases `fsr test` replays through the interpreter.

### lower_tests

* `pub fn lower_tests(file: &str, source: &str) -> Result<TestFile, LowerError>`

Lowers the test file at `file`, relative to the app. The file holds imports, `const` fixtures, mock functions, `let` bindings, hooks, `describe` blocks and tests; anything else is a `LowerError::Residue` naming its line. A test imports the loader, its `meta` or `store`, an action, a route handler or the middleware by path or alias and its helpers from `@snapfire/fsr/testing`. An `only` anywhere in the file is resolved here, so every case it does not cover carries `Mode::Skip`.

### TestFile and Block

* `pub struct TestFile { pub file: String, pub blocks: Vec<Block>, pub tests: Vec<TestCase> }`
* `TestFile::chain(&self, block: usize) -> Vec<usize>`: the blocks around `block`, outermost first, block 0 being the file.
* `TestFile::full_name(&self, case: &TestCase) -> String`: the case's name under its blocks' names, joined with ` > `.
* `pub struct Block { pub name: String, pub parent: Option<usize>, pub each: Option<Each>, pub before_all: Vec<(usize, Step)>, pub after_all: Vec<(usize, Step)>, pub before_each: Vec<(usize, Step)>, pub after_each: Vec<(usize, Step)> }`

A `describe` or the file itself, each hook as its steps with their lines. A mock function declared at the top of a file or a block is a `Step::Fn` in its `before_each`, so every test starts it with no calls.

### TestCase and Mode

* `pub struct TestCase { pub name: String, pub line: usize, pub block: usize, pub mode: Mode, pub each: Option<Each>, pub steps: Vec<(usize, Step)> }`
* `pub enum Mode { Run, Skip, Todo }`
* `pub struct Each { pub table: Expr, pub params: Vec<Binding> }`

`name` is the pattern an `each` fills. `table` is evaluated when the file runs: an array row is spread over `params` and any other row binds the one parameter. A template table lowers to an array of objects keyed by its headings.

### Step

* `pub enum Step { Mock { name, mock }, Run { binding, target, ctx, input }, Let { name, value }, Assert(Assertion), Fn { name, answer: Option<Answer> }, Answer { name, answer, once }, Clear { name, reset } }`

`Mock` is a `ctx(...)` bound or assigned to a name. `Let` is a local `const` or an assignment to a `let`. `Fn` is `fn(impl)`, `vi.fn(impl)` or `jest.fn(impl)`. `Answer` is `mockReturnValue`, `mockResolvedValue`, `mockRejectedValue`, `mockImplementation` and their `Once` forms. `Clear` is `mockClear` or `mockReset`; an empty name is every mock function, which `vi.clearAllMocks()` asks for.

### Assertion and Subject

* `pub enum Assertion { Ok(Expr), Equal(Expr, Expr), Rejects { target, ctx, input, kind: Option<String> }, Expect { subject: Subject, not: bool, matcher: Matcher, message: Option<String> } }`
* `pub enum Subject { Value(Expr), Settled { target, ctx, input, rejects: bool }, Mock(String) }`

`Ok`, `Equal` and `Rejects` are `assert.ok`, `assert.equal` and `assert.rejects`. `assert.match(actual, pattern, message?)` lowers to an `Expect` with `Matcher::Match`. `Expect` is `expect(subject, message?)` with `.not` and a matcher; the message must be a string literal. A subject under `.resolves` or `.rejects` is a run, settled when the step runs. Its failure reads as `{ kind, message }`. A call matcher reads a mock function by name.

### Matcher and Pattern

* `pub enum Matcher { Be, Equal, StrictEqual, Truthy, Falsy, Null, Undefined, Defined, NaN, GreaterThan, GreaterThanOrEqual, LessThan, LessThanOrEqual, CloseTo, Contain, ContainEqual, Length, Property, Match, MatchObject, TypeOf, OneOf, Throw, Called, CalledOnce, CalledTimes, CalledWith, CalledExactlyOnceWith, LastCalledWith, NthCalledWith, Returned, ReturnedTimes, ReturnedWith, LastReturnedWith }`, each carrying the expressions its arguments lowered to
* `Matcher::name(&self) -> &'static str`: the name a test writes, `toBe` for `Be`. `Matcher::takes_expected(&self) -> bool` and `Matcher::reads_calls(&self) -> bool`.
* `pub enum Pattern { Text(Expr), Regex { source: String, flags: String } }`
* `pub const EXPECT_MARK: &str = "$sf.expect"`

Each matcher's older names lower to the same variant: `toBeCalled` is `Called`, `toThrowError` is `Throw`. Expected values lower like any value, except that `expect.any`, `anything`, `objectContaining`, `arrayContaining`, `stringContaining`, `stringMatching` and `closeTo`, with their `expect.not` forms, lower to objects whose `EXPECT_MARK` field names the helper, which the runner reads as a matcher. A regular expression literal is kept as its source and flags for `toMatch`, `toThrow` and `stringMatching`, the only places the lowering takes one. A matcher for markup is refused, since a body test has none.

### Mock, Answer and Binding

* `pub struct Mock { pub session, pub services: Vec<(String, String, Expr)>, pub mock_fns: Vec<(String, String, String)>, pub input, pub params, pub query, pub identity, pub locale, pub path, pub host, pub config }`
* `pub enum Answer { Returns(Expr), Calls(Expr), Fails(Expr) }`
* `pub enum Binding { Name(String), Fields(Vec<(String, String)>) }`

A service method that names a mock function or a `let` is in `mock_fns` as `(service, method, name)` rather than in `services`, which holds a lambda or a value as a lambda of no parameters. `Returns` answers a value whatever the arguments, `Calls` applies a lambda to them and `Fails` fails the call with the kind its value names when that is `{ kind, message }`.

### Target

* `pub enum Target { Loader { file }, Meta { file }, Store { file }, Paths { file }, Action { file, export }, Handler { file, export }, Middleware { file } }`; `Paths` comes from `paths` imported from a `page.loader` and a `paths()` call with no argument runs against the ctx bound above it.

What a run names: a loader module, its `meta` or its `store`, an action export, a route handler's method or the middleware, each path relative to the app.

## 4. Error Handling

### LowerError

* `Parse { file: String, message: String }`, with the parser's line, column and message in `message`.
* `MissingExport { file: String, export: String }`.
* `Residue(Residue)`, transparent in `Display`.
* `Reach(Residue)`: a `body` extension on a component's render path; `Display` is the residue followed by why it is refused.
* `Extension(Residue)`: an export under `ext/` that does not lower or a `native` declaration the build cannot read; `Display` is the residue followed by the rule.

### Residue

* `pub struct Residue { pub file: String, pub line: usize, pub column: usize, pub message: String, pub hint: Option<String>, pub via: Vec<Placement> }`
* `Display` is `{file}:{line}:{column}: {message}`, then an indented `reached through <chain>` when `via` is not empty, then the indented `hint` when there is one.
* `line` and `column` are one-based and point at the construct, not at the statement that contains it.
* `hint` names the rewrite that does the same thing in the IR.
* `via` is how the module the build asked for reaches the file this residue is in, outermost placement first, empty when they are the same file. A component set re-raising a child's residue records the placement with `Residue::placed_at`, so one unlowerable leaf tells every page above it which tag to follow. `Residue::chain(&self) -> String` prints it as one line.

### Placement

* `pub struct Placement { pub file: String, pub line: usize, pub column: usize, pub tag: String }`, the file holding a `<Tag />`, the one-based position of the tag and the name as written.
* `Display` is `<{tag}> {file}:{line}:{column}`.
