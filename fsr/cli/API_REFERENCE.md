# API Reference: snapfire_fsr_cli

The `fsr` binary and the library build it fronts: route discovery, the contract, lowering, the plan file and the generated TypeScript.

## Contents

* [1. The Binary](#1-the-binary)
  * [fsr add](#fsr-add)
  * [fsr types](#fsr-types)
  * [fsr build](#fsr-build)
  * [fsr check](#fsr-check)
  * [fsr serve](#fsr-serve)
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
* [5. Vendoring and Declarations](#5-vendoring-and-declarations)
  * [Layout](#layout)
  * [Spec](#spec)
  * [add](#add)
  * [fetch](#fetch)
  * [tsconfig](#tsconfig)
  * [Manifests](#manifests)
  * [Ts](#ts)
  * [Inferer](#inferer)
* [6. Error Handling](#6-error-handling)
  * [BuildError](#builderror)

## 1. The Binary

### fsr build

* `fsr build <app dir> [--shell <module id>] [--slot <name>]`
* Runs the build, prints the report to stdout, writes `<app dir>/generated/plan.json`, `generated/contracts/<client>.json` per document and `generated/contracts/schemas.json`, `generated/services.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json` and `tsconfig.build.json`, then prints `wrote <path>` for each.
* Exit 0 on success, 1 on any `BuildError`, 2 on a usage error.

### fsr check

* `fsr check <app dir> [--shell <module id>] [--slot <name>]`
* Runs the build and prints the report; writes nothing. Same exit codes.

### fsr serve

* `fsr serve <app dir> [--listen <addr>]`
* `serve::run`: the stock host, `snapfire_fsr_host::Host`, over the configuration `serve::project_root` finds, the directory beside the app when it holds `config/`, `app.toml` or `app.yaml`, else the app itself; prints the host's report and listens on `--listen` or the configured `server.listen` until the process ends. Refuses a configuration whose `[app] dir` is not the app given. Exit 1 on a `BuildError::Serve`.
* `serve::host_for(app: &Path) -> Result<Host, BuildError>` is the same host without the listener.
* `fsr dev <app dir>` runs this in place of `cargo run` when no `Cargo.toml` is beside the app, watching `config/`, `app.toml` and `app.yaml` there instead of `src/`.

### fsr add

* `fsr add <app dir> <name@version[/subpath]>... [--external <name,...>]`
* `vendor::add` over the specs; prints `added <specifier> <file> <bytes>` per entry or `xwpm add <spec>` per delegated call. Same exit codes; a spec without a version exits 2.

### fsr types

* `fsr types <app dir> [--refresh]`
* `types::fetch`; prints `ran <command>` for each delegated xwpm command, `types <package> <from> <version>` per fetch, `kept <package>` and `missing <package> <why>`. Exit 0 with missing packages, 1 on a `BuildError`.

## 2. The Build

### Options

* `pub struct Options { pub shell: String, pub slot: String, pub mounter_module: String, pub mounter: String }`
* `Default` is `shell#document`, `content`, `@snapfire/fsr-client/react` and `reactMounter`.

### build

* `pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError>`
* Imports `app/clients`, reads `app/schemas`, validates the contract, walks `app/routes`, lowers every `loader.ts` and `actions.ts` and returns everything without writing. The first error in any file fails the whole build.

### Built

* `pub struct Built { pub manifest: Manifest, pub contract: Contract, pub report: Report, pub files: Vec<(String, String)> }`
* `files` pairs a path relative to the app directory with its content: `generated/plan.json`, `generated/contracts/<client>.json` per document in name order, `generated/contracts/schemas.json`, `generated/services.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json`, `tsconfig.build.json`, in that order.

### write

* `pub fn write(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError>`
* Removes every `*.json` under `generated/contracts/`, then writes every entry of `built.files` under `app`, creating directories as needed. Returns the paths written.

### Report

* `pub struct Report { pub routes: Vec<(String, String)>, pub sources: Vec<(String, String)>, pub actions: Vec<(String, String)>, pub services: Vec<(String, String)>, pub schemas: Vec<(String, String)>, pub types: Vec<(String, String)> }`
* `routes` pairs a pattern with its directory relative to `app`; `sources` and `actions` pair an id with the module that lowered to it; `services` pairs a service with its document; `schemas` pairs a type with its file; `types` pairs a package with `types::status`'s row.
* `Display` prints the six sections in that order, source and action rows labelled `lowered`, service rows `http` or `grpc` by their document's extension.

## 3. Discovery Rules

### Clients

* Every `app/clients/<name>.openapi.json`, sorted by name, is imported with `snapfire_fsr_service::import` as service `<name>`. Its types and services are merged into one contract; a type name that two documents both define is `DuplicateType`.
* Every `clients/*.proto`, in name order after the OpenAPI documents, is imported with `snapfire_fsr_service::import_proto` under its file stem the same way. `CONTRACTS_DIR` and `PLAN_FILE` name where the build writes.

### Schemas

* Every `app/schemas/*.ts`, sorted by name, is read with `snapfire_fsr_lower::read_schema`. Each exported interface or string-literal union becomes a contract type; a name declared twice is `DuplicateType`.
* The type named `Session` is imported into `generated/fsr.ts` from its file and types `ctx.session`; without one, `session` is `Record<string, unknown>`. An `export const defaults` in that file is read with `read_session_defaults` and folded into every lowered session read.
* After both, `Contract::validate` runs; an unresolved reference is `BuildError::Contract`.

### Routes

* A directory under `routes/` is a route when it contains `page.tsx` or `page.ts` and a handler route when it contains `route.ts`. One holding both is `BuildError::PageAndRoute`. Other directories contribute path segments only.
* A segment is a name of ASCII letters, digits, `_` and `-`, `[name]` for a parameter or `[...name]` for a catch-all. Anything else is `BuildError::Segment`.
* `index` as the first segment is the root. `index` deeper in a path is a literal segment.
* Routes are sorted by pattern before ids are assigned.

### Ids

* Source id: the static segments joined with `.`; `index` for the root. Parameter segments contribute nothing.
* Action id: `<source id>.<export>` for each export `lower_actions` returns.
* Handler id: `<route id>.<METHOD>` for each export of `route.ts` named `GET`, `POST`, `PUT`, `PATCH` or `DELETE`; the row also carries the method and the pattern. An `action<T>` export names `T` as its input, which must be a schema type or the build fails with `UnknownHandlerInput`.

### Modules

* Page: `<route dir>/page.tsx#default`, with the directory relative to `app`.
* Error: `routes/error.tsx#default` (or `.ts`) when present, applied to every page; a route's own `error.tsx` takes precedence for that route.
* Loading: `<route dir>/loading.tsx#default` when present; the node is marked deferred with it as the fallback.
* Not found: `routes/not-found.tsx#default` (or `.ts`) when present, the page for a path no route matches.
* Loader, actions and route modules are named by their relative paths in the source, action and handler rows.

### Plan shape

* Every route is a two-node tree: node 0 is the shell module, node 1 is the page in `Options::slot`, carrying the source id, the error module and the fallback.
* `not_found` is the same two-node tree around the not-found module with the routes-level error module and no source, present only when the module is; the host renders it with status 404 and `params.path` set to the path asked for.
* Sources and actions are emitted with `RowOwner::Lowered` and their bodies. No other owner is produced.
* An action whose `action<T>` names a type the contract lacks is `UnknownInput`; an action row carries `input` when it names one.

### Generated files

* `generated/contracts/<client>.json` is `Contract::to_json` of that document's import, types and service; `generated/contracts/schemas.json` holds the schema types. `CONTRACTS_DIR` names the directory. The build merges them with `Contract::merge` for `services.d.ts`, `client.ts` and validation, so a type two documents define fails the build naming the second; `write` empties the directory of `*.json` before writing so a removed client leaves nothing behind.
* `generated/fsr.ts` declares `Routes` with one key per page pattern and per handler pattern, so `Ctx<"/api/cart">` types a handler's parameters.
* `generated/services.d.ts` is `snapfire_fsr_service::typescript::declarations` of it.
* `generated/islands.ts` imports `registerIsland` and the mounter and exports `registerIslands()`, one call per module: the routes-level error module, the not-found module, then each page, its error and its loading module, each loading `../<path>.js` relative to `generated/`.
* `generated/client.ts` imports `action as call` from `@snapfire/fsr-client`, prints every contract type in client flavour, one `export type <Id>Props` per route from `infer::Inferer::returns` over its loader (`{}` without one) and `export const actions`, nested by the dots of each action id, each `call("<id>") as unknown as (input: <Input>) => Promise<<returns>>`.
* `tsconfig.json` is `types::tsconfig`; `tsconfig.build.json` is `types::tsconfig_build`.
* `generated/fsr.ts` is what the generated `tsconfig.json` maps `@snapfire/fsr` to; it imports the base package as `@snapfire/fsr-authoring`, re-exports `fail` and `Services`, imports `Session`, declares `Routes` with one key per pattern whose value has a `string` field per parameter, `Ctx<P extends keyof Routes = keyof Routes>` with `params`, `query`, `session`, `identity`, `services` and `now`, `ActionCtx<Input, P>` and an `action<Input, Out>` wrapper over `@snapfire/fsr`'s.

## 4. Inference

### Ts

* `pub enum infer::Ts { Str, Num, Big, Bool, Null, Unknown, Named(String), List(Box<Ts>), Map(Box<Ts>), Tuple(Vec<Ts>), Record(Vec<(String, Ts)>), Union(Vec<Ts>), Inter(Vec<Ts>) }`
* `Ts::print(&self, flavour: Flavour) -> String`; `Big` is `bigint` on the server and `bigint | number` on the client; a union or big inside a list or an intersection is parenthesised.

### Inferer

* `pub struct infer::Inferer<'a> { pub contract: &'a Contract, pub session: Option<&'a str>, pub input: Option<&'a str> }`
* `Inferer::returns(&self, body: &Body) -> Ts`: the union of every `return`, `Null` when none.
* `Inferer::expr(&self, expr: &Expr, env: &[(String, Ts)]) -> Ts`. Reads type by their root, a call by its method's return, a session key by the `Session` record, `map` by its lambda's body over the element, `filter` by its operand, `Object.entries` as `[string, V][]`, an object literal as a record intersected with its spreads, a coalesce against an empty object or array as its left side. Anything else is `Unknown`, which absorbs a union it joins.

## 5. Vendoring and Declarations

### Layout

* `pub struct xwpm::Layout { pub vendor: String, pub base: String, pub importmap: String, pub types: String, pub xwpm: bool }`, paths relative to the app directory.
* `Layout::of(app: &Path) -> Result<Layout, BuildError>`: the defaults `vendor`, `/static/js/vendor`, `importmap.json`, `types` and `xwpm: false`; with `<app>/xwpm.wmf` present, its root records with `xwpm: true`.
* `Layout::from_wmf(text: &str) -> Result<Layout, String>`: root records `vendor`, `base`, `importmap` and `types` override the defaults; other records are ignored; sections are skipped; a root line that is not `key = value` is an error naming its line.
* `xwpm::run(app: &Path, args: &[&str]) -> Result<(), BuildError>`: runs `xwpm` in the app directory; `Xwpm` when it cannot start or exits non-zero.
* `xwpm::XWPM_FILE` is `xwpm.wmf`.

### Spec

* `pub struct vendor::Spec { pub package: String, pub version: String, pub subpath: Option<String> }`
* `Spec::parse(raw: &str) -> Result<Spec, BuildError>`: `name@version`, `name@version/subpath`, `@scope/name@version[/subpath]`; anything else is `Spec`.
* `Spec::specifier(&self) -> String`: `package` or `package/subpath`, the import map key.

### add

* `pub fn vendor::add(app: &Path, specs: &[Spec], externals: &[String]) -> Result<AddReport, BuildError>`
* Under the default layout, per spec: `GET https://esm.sh/<package>@<version>[/<subpath>]?target=es2022&bundle[&external=<externals>]`, follows every absolute path the stub names, writes each file by its base name under `<vendor>/<package>/`, rewrites same-package absolute imports to `./<name>` and fails with `Dependency` on any other; the file behind the stub's `export *` becomes the entry, written to the import map as `<base>/<package>/<name>` and to the vendor manifest. Under xwpm: `xwpm add <package>@<version>` once per distinct package and version.
* `pub struct AddReport { pub added: Vec<(String, String, usize)>, pub delegated: Vec<String> }`: specifier, file relative to the vendor directory and bytes; or the xwpm invocations run.
* `vendor::read_import_map`, `vendor::write_import_map`, `vendor::import_map_packages(app, &layout)`: the `imports` table whole and its bare keys as package names (`react/jsx-runtime` is `react`, `@a/b/c` is `@a/b`).
* `vendor::package_of(specifier: &str) -> String`.
* `vendor::ESM_HOST` is `https://esm.sh`.

### fetch

* `pub fn types::fetch(app: &Path, refresh: bool) -> Result<TypesReport, BuildError>`
* The queue is `@snapfire/fsr-authoring`, `@snapfire/fsr-client`, then `import_map_packages`. A package whose directory exists is kept unless `refresh`; the fsr packages are written from declarations embedded in the binary; any other `@snapfire/*` is `missing`. Under xwpm, `xwpm restore` and `xwpm types` run first and every other package is `missing` with that reason. Otherwise, the npm registry: the abbreviated packument chooses the highest release sharing the vendored major, else `latest`; the version document's `types` or `typings` names the entry and its tarball's `.d.ts`, `.d.mts`, `.d.cts` and `package.json` files are unpacked under `<types>/<package>/`; without one, `@types/<name>` (`@scope/name` as `@types/scope__name`) the same way with `index.d.ts` as the entry; its `dependencies` are queued under their package names. A package with neither is `missing`.
* `pub struct TypesReport { pub fetched: Vec<(String, String, String)>, pub kept: Vec<String>, pub missing: Vec<(String, String)>, pub delegated: Vec<String> }`: package, version and source; kept packages; package and reason; xwpm commands run.
* `types::definitely_typed(package) -> String`, `types::is_ambient(entry: &str) -> bool` (contains `declare module "` or `declare module '`).
* `types::present(app, &layout) -> Result<Vec<(String, TypedPackage)>, BuildError>`: every package directory under the types directory, scoped ones included, with the manifest's record or `index.d.ts` as the entry when it has none.
* `types::status(app) -> Result<Vec<(String, String)>, BuildError>`: the report rows for the fsr packages and every import map package: `<types>/<package>  <from> <version>`, `<types>/<package>` when unrecorded or `missing; run fsr types`.
* `types::NPM_REGISTRY` is `https://registry.npmjs.org`.

### tsconfig

* `pub fn types::tsconfig(app: &Path) -> Result<String, BuildError>`: `target` es2022, `module` esnext, `moduleResolution` bundler, `jsx` react-jsx, `strict`, `noEmit`, `skipLibCheck`; `paths` with `@snapfire/fsr` to `./generated/fsr`, then per present package `<name>` to `./<types>/<name>/<entry>` unless ambient and `<name>/*` to `./<types>/<name>/*`; `include` of `src/**/*`, `routes/**/*`, `schemas/**/*`, `generated/**/*` and each ambient entry.
* `pub fn types::tsconfig_build() -> String`: `target` es2022, `outDir` dist, `rootDir` `.`, `sourceMap`, `jsx` react-jsx; `include` of `src/**/*`, `routes/**/*.tsx`, `generated/islands.ts` and `generated/client.ts`.

### Manifests

* `pub struct vendor::VendorManifest { pub packages: BTreeMap<String, VendoredPackage> }`, `vendor::VendoredPackage { pub version: String, pub externals: Vec<String>, pub entries: BTreeMap<String, String> }`, entries from specifier to file relative to the vendor directory; read and written as `<vendor>/.fsr-vendor.json`.
* `pub struct types::TypesManifest { pub packages: BTreeMap<String, TypedPackage> }`, `types::TypedPackage { pub version: String, pub from: String, pub entry: String, pub ambient: bool }`; read and written as `<types>/.fsr-types.json`.
* Both: `read(app, &layout)`, `write(&self, app, &layout)`; a missing file reads as empty.

## 6. Error Handling

### BuildError

* `Io(PathBuf, std::io::Error)`
* `NoRoutes(PathBuf)`, when `app/routes` is not a directory.
* `Segment { path: PathBuf, name: String }`
* `Lower(LowerError)`, transparent; see `snapfire_fsr_lower`.
* `Import { document: String, error: ImportError }`
* `DuplicateType { name: String, first: String, second: String }`
* `Contract(ContractError)`, from `Contract::validate`.
* `UnknownInput { action: String, name: String }`
* `Spec(String)`, an `fsr add` argument that is not `name@version[/subpath]`.
* `Http(String, String)`, the URL and the failure.
* `Manifest(PathBuf, String)`, a vendor manifest, types manifest, import map or `xwpm.wmf` that did not parse.
* `Serve(String)`, the stock host refusing to build or the listener failing, from `fsr serve`.
* `Dependency { package: String, wants: String }`, a vendored module importing a package outside its bundle.
* `Xwpm(String)`, an `xwpm` command that could not start or failed.
