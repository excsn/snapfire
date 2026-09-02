# API Reference: snapfire_fsr_cli

The `fsr` binary and the library build it fronts: route discovery, the contract, lowering, the plan file and the generated TypeScript.

## Contents

* [1. The Binary](#1-the-binary)
  * [fsr build](#fsr-build)
  * [fsr check](#fsr-check)
* [2. The Build](#2-the-build)
  * [Options](#options)
  * [build](#build)
  * [Built](#built)
  * [write](#write)
  * [Report](#report)
* [3. Discovery Rules](#3-discovery-rules)
  * [Clients](#clients)
  * [Schemas](#schemas)
  * [Routes](#routes)
  * [Ids](#ids)
  * [Modules](#modules)
  * [Plan shape](#plan-shape)
  * [Generated files](#generated-files)
* [4. Inference](#4-inference)
  * [Ts](#ts)
  * [Inferer](#inferer)
* [5. Error Handling](#5-error-handling)
  * [BuildError](#builderror)

## 1. The Binary

### fsr build

* `fsr build <app dir> [--shell <module id>] [--slot <name>]`
* Runs the build, prints the report to stdout, writes `<app dir>/plan.json`, `generated/contract.json`, `generated/services.d.ts` and `generated/fsr.ts`, then prints `wrote <path>` for each.
* Exit 0 on success, 1 on any `BuildError`, 2 on a usage error.

### fsr check

* `fsr check <app dir> [--shell <module id>] [--slot <name>]`
* Runs the build and prints the report; writes nothing. Same exit codes.

## 2. The Build

### Options

* `pub struct Options { pub shell: String, pub slot: String, pub mounter_module: String, pub mounter: String }`
* `Default` is `shell#document`, `content`, `@snapfire/fsr-client/react` and `reactMounter`.

### build

* `pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError>`
* Imports `app/clients`, reads `app/schemas`, validates the contract, walks `app/routes`, lowers every `loader.ts` and `actions.ts` and returns everything without writing. The first error in any file fails the whole build.

### Built

* `pub struct Built { pub manifest: Manifest, pub contract: Contract, pub report: Report, pub files: Vec<(String, String)> }`
* `files` pairs a path relative to the app directory with its content: `plan.json`, `generated/contract.json`, `generated/services.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, in that order.

### write

* `pub fn write(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError>`
* Writes every entry of `built.files` under `app`, creating `generated/` as needed. Returns the paths written.

### Report

* `pub struct Report { pub routes: Vec<(String, String)>, pub sources: Vec<(String, String)>, pub actions: Vec<(String, String)>, pub services: Vec<(String, String)>, pub schemas: Vec<(String, String)> }`
* `routes` pairs a pattern with its directory relative to `app`; `sources` and `actions` pair an id with the module that lowered to it; `services` pairs a service with its document; `schemas` pairs a type with its file.
* `Display` prints the five sections in that order, source and action rows labelled `lowered`, service rows `http`.

## 3. Discovery Rules

### Clients

* Every `app/clients/<name>.openapi.json`, sorted by name, is imported with `snapfire_fsr_service::import` as service `<name>`. Its types and services are merged into one contract; a type name that two documents both define is `DuplicateType`.

### Schemas

* Every `app/schemas/*.ts`, sorted by name, is read with `snapfire_fsr_lower::read_schema`. Each exported interface or string-literal union becomes a contract type; a name declared twice is `DuplicateType`.
* The type named `Session` is imported into `generated/fsr.ts` from its file and types `ctx.session`; without one, `session` is `Record<string, unknown>`. An `export const defaults` in that file is read with `read_session_defaults` and folded into every lowered session read.
* After both, `Contract::validate` runs; an unresolved reference is `BuildError::Contract`.

### Routes

* A directory under `routes/` is a route when it contains `page.tsx` or `page.ts`. Other directories contribute path segments only.
* A segment is a name of ASCII letters, digits, `_` and `-`, `[name]` for a parameter or `[...name]` for a catch-all. Anything else is `BuildError::Segment`.
* `index` as the first segment is the root. `index` deeper in a path is a literal segment.
* Routes are sorted by pattern before ids are assigned.

### Ids

* Source id: the static segments joined with `.`; `index` for the root. Parameter segments contribute nothing.
* Action id: `<source id>.<export>` for each export `lower_actions` returns.

### Modules

* Page: `<route dir>/page.tsx#default`, with the directory relative to `app`.
* Error: `routes/error.tsx#default` (or `.ts`) when present, applied to every page; a route's own `error.tsx` takes precedence for that route.
* Loading: `<route dir>/loading.tsx#default` when present; the node is marked deferred with it as the fallback.
* Loader and actions modules are named by their relative paths in the source and action rows.

### Plan shape

* Every route is a two-node tree: node 0 is the shell module, node 1 is the page in `Options::slot`, carrying the source id, the error module and the fallback.
* Sources and actions are emitted with `RowOwner::Lowered` and their bodies. No other owner is produced.
* An action whose `action<T>` names a type the contract lacks is `UnknownInput`; an action row carries `input` when it names one.

### Generated files

* `generated/contract.json` is `Contract::to_json` of the merged contract.
* `generated/services.d.ts` is `snapfire_fsr_service::typescript::declarations` of it.
* `generated/islands.ts` imports `registerIsland` and the mounter and exports `registerIslands()`, one call per module: the routes-level error module, then each page, its error and its loading module, each loading `../<path>.js` relative to `generated/`.
* `generated/client.ts` imports `action as call` from `@snapfire/fsr-client`, prints every contract type in client flavour, one `export type <Id>Props` per route from `infer::Inferer::returns` over its loader (`{}` without one) and `export const actions`, nested by the dots of each action id, each `call("<id>") as unknown as (input: <Input>) => Promise<<returns>>`.
* `generated/fsr.ts` is what the app maps `@snapfire/fsr` to in `tsconfig.json` `paths`; it imports the base package as `@snapfire/fsr-authoring`, re-exports `fail` and `Services`, imports `Session`, declares `Routes` with one key per pattern whose value has a `string` field per parameter, `Ctx<P extends keyof Routes = keyof Routes>` with `params`, `query`, `session`, `identity`, `services` and `now`, `ActionCtx<Input, P>` and an `action<Input, Out>` wrapper over `@snapfire/fsr`'s.

## 4. Inference

### Ts

* `pub enum infer::Ts { Str, Num, Big, Bool, Null, Unknown, Named(String), List(Box<Ts>), Map(Box<Ts>), Tuple(Vec<Ts>), Record(Vec<(String, Ts)>), Union(Vec<Ts>), Inter(Vec<Ts>) }`
* `Ts::print(&self, flavour: Flavour) -> String`; `Big` is `bigint` on the server and `bigint | number` on the client; a union or big inside a list or an intersection is parenthesised.

### Inferer

* `pub struct infer::Inferer<'a> { pub contract: &'a Contract, pub session: Option<&'a str>, pub input: Option<&'a str> }`
* `Inferer::returns(&self, body: &Body) -> Ts`: the union of every `return`, `Null` when none.
* `Inferer::expr(&self, expr: &Expr, env: &[(String, Ts)]) -> Ts`. Reads type by their root, a call by its method's return, a session key by the `Session` record, `map` by its lambda's body over the element, `filter` by its operand, `Object.entries` as `[string, V][]`, an object literal as a record intersected with its spreads, a coalesce against an empty object or array as its left side. Anything else is `Unknown`, which absorbs a union it joins.

## 5. Error Handling

### BuildError

* `Io(PathBuf, std::io::Error)`
* `NoRoutes(PathBuf)`, when `app/routes` is not a directory.
* `Segment { path: PathBuf, name: String }`
* `Lower(LowerError)`, transparent; see `snapfire_fsr_lower`.
* `Import { document: String, error: ImportError }`
* `DuplicateType { name: String, first: String, second: String }`
* `Contract(ContractError)`, from `Contract::validate`.
* `UnknownInput { action: String, name: String }`
