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
  * [emit](#emit)
  * [Report](#report)
* [3. Discovery Rules](#3-discovery-rules)
  * [Clients](#clients)
  * [Schemas](#schemas)
  * [Extensions](#extensions)
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

* `fsr build <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>]`
* Runs the build, prints the report to stdout, writes `<app dir>/generated/plan.json`, `generated/contracts/<client>.json` per document and `generated/contracts/schemas.json`, `generated/services.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json` and `tsconfig.build.json`, prints `wrote <path>` for each, then bundles the browser modules into `<app dir>/dist/` with `snapfirec`.
* The bundle follows the generation because it compiles the island registry the generation writes. `--public-path` defaults to `/static/js/app`, or `<at>/static/js/app` for a site; `--snapfirec` defaults to `$SNAPFIREC`, else beside this binary, else `PATH`.
* Exit 0 on success, 1 on any `BuildError`, 2 on a usage error.

### fsr check

* `fsr check <app dir> [--shell <module id>] [--slot <name>]`
* Runs the build and prints the report; writes nothing. Same exit codes.

### fsr serve

* `fsr serve <app dir> [--listen <addr>]`
* `serve::run`: the stock host, `snapfire_fsr_host::Host`, over the configuration `serve::project_root` finds, the directory beside the app when it holds `config/`, `app.toml` or `app.yaml`, else the app itself; prints the host's report and listens on `--listen` or the configured `server.listen` until the process ends. Refuses a configuration whose `[app] dir` is not the app given. Exit 1 on a `BuildError::Serve`.
* `serve::host_for(app: &Path) -> Result<Host, BuildError>` is the same host without the listener.
* `serve::prerender(app: &Path, out: Option<&Path>) -> Result<Vec<(String, PathBuf)>, BuildError>` builds that host and calls `Host::prerender` with `out`, else `server.prerender`, else `dist/prerender` under the app, one rendering per configured locale; `fsr prerender` prints what it wrote, or that nothing qualifies.
* `fsr dev <app dir>` runs this in place of `cargo run` when no `Cargo.toml` is beside the app, watching `config/`, `app.toml` and `app.yaml` there instead of `src/`. The host it builds carries a reloader that rereads the project, so a change to the generated files is `POST /__fsr/reload` to `server.listen`, printing the report it answers with, and the process restarts only when the reload is refused, a changed `[session]` for one; after a rebundle alone it posts `/__fsr/changed`, best effort, so open development documents refresh.

### Sites

* Every command reads the `[site]` section of the configuration beside the app, `site_beside`, and builds with it; `fsr dev` then serves the bundle under `<at>/static/js/app`.
* `fsr serve` mounts every site the shell's `[sites]` table names, `snapfire_fsr_sites::mount_all`, sets a reloader that mounts them again and watches the table, `snapfire_fsr_sites::watch`, on `SIGHUP` and on `sites.poll`.

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

### SiteOptions

* `pub struct SiteOptions { pub name: String, pub at: String, pub shell: Option<PathBuf> }`: the `[site]` section as the build reads it; `prefix()` is `<name>:`.
* `Options::beside(app: &Path) -> Options`: the defaults with `site` from the configuration beside `app` when one names that app directory. `site_beside(app: &Path) -> Option<SiteOptions>` is that lookup alone. `Options::prefix()` is the prefix on every emitted id, empty without a site.
* `unprefixed(service: &str) -> &str`: a service name without its site prefix, which is what a test mocks it by.

### ShellContract

* `pub struct ShellContract { pub version: u32, pub store: BTreeMap<String, String>, pub imports: BTreeMap<String, String>, pub fsr: String }`: `generated/shell.json`. `SHELL_CONTRACT_VERSION` is 1.
* `ShellContract::read(path: &Path) -> Result<ShellContract, BuildError>`: refuses a version this fsr does not read.
* `declarations(&self) -> String`: `generated/shell.d.ts`, `ShellStore` and `ShellImport`.

### build

* `pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError>`
* With `options.site` set, after every generated TypeScript is written unprefixed, the plan file is `Manifest::namespaced`, every contract file `Contract::namespaced`, the islands registry registers `<name>:<module>` and the generated call sites call `<name>:<action id>`; with `site.shell` set, `generated/shell.d.ts` is written from the shell contract. Without a site, `generated/shell.json` is written: every store key a `store` export seeds, typed by inferring the loader's return and then the store body, the app's import map and the fsr version.
* Imports `app/clients`, reads `app/schemas`, validates the contract, walks `app/routes`, lowers every `page.loader.ts` and `actions.ts` and returns everything without writing. The first error in any file fails the whole build.

### Built

* `pub struct Built { pub manifest: Manifest, pub contract: Contract, pub report: Report, pub files: Vec<(String, String)> }`
* `files` pairs a path relative to the app directory with its content: `generated/plan.json`, `generated/contracts/<client>.json` per document in name order, `generated/contracts/schemas.json`, `generated/services.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json`, `tsconfig.build.json`, in that order.

### write

* `pub fn write(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError>`
* Removes every `*.json` under `generated/contracts/` and the whole `.fsr-bundle/` directory, then writes every entry of `built.files` under `app`, creating directories as needed. Returns the paths written.

### write_overlay

* `pub fn write_overlay(app: &Path, built: &Built) -> Result<(), BuildError>`
* Removes `.fsr-bundle/` and writes only the entries of `built.files` under it: the sources the build rewrote for the browser. `write` includes them; this is for a caller that compiles without writing the rest, which is what `fsr test` does.
* `dev::BUNDLE_OVERLAY` is the directory's name, `.fsr-bundle`.

### emit

* `pub fn emit(app: &Path, options: DevOptions) -> Result<Emitted, BuildError>`
* `pub struct Emitted { pub built: Built, pub written: Vec<PathBuf> }`
* `build`, then `write`, then `snapfirec` over `tsconfig.build.json` into `<app>/dist` with `options.public_path`, the layout's import map and `--overlay .fsr-bundle` when the build wrote one, so a rewritten source is compiled in place of its original at the same path. The order is load-bearing: the bundle compiles the island registry the generation writes.
* The whole artifact a host reads, and what a `build.rs` calls. `build` and `write` alone leave `dist/` at whatever the last bundle wrote, which the host cannot distinguish from a current one.
* `Dev` naming the compiler when it cannot start, or its exit status when it fails.

### Report

* `shell: Option<(String, usize, usize, Vec<String>)>`: for a site built against a shell contract, its path, its store key and import counts and the site's import map entries that differ; `Display` prints a `shell` row and a second naming the differences.

* `pub struct Report { pub routes: Vec<(String, String)>, pub layouts: Vec<(String, String)>, pub slots: Vec<(String, String)>, pub intercepts: Vec<(String, String)>, pub sources: Vec<(String, String)>, pub actions: Vec<(String, String)>, pub handlers: Vec<(String, String)>, pub middleware: Option<String>, pub components: Vec<(String, String, String)>, pub hoisted: Vec<(String, usize)>, pub services: Vec<(String, String)>, pub schemas: Vec<(String, String)>, pub types: Vec<(String, String)> }`
* `hoisted` gives a lowered component's module, prefixed for a site, how many of its render-path calls and how many of its static subtrees the server computes for the browser; `Display` prints them as `hoisted` rows after the components, `4 values, 8 subtrees`.
* `islands: Vec<(String, usize)>` names each component placed as an island in server mode, prefixed for a site, with how many handlers it answers; `Display` prints them as `islands` rows labelled `server`, before `hoisted`.
* `extensions: Vec<(String, String)>` pairs each export under `ext/`, `file#name`, with `lowered`, `native render` or `native body`; `browser: Vec<(String, String)>` pairs a lowered module, prefixed for a site, with `file:line:column` of each render-path call that stays in the browser after hoisting. `Display` prints `extensions` rows, then `browser` rows, before `hoisted`.
* `routes` pairs a pattern with its directory relative to `app`; `layouts` pairs the pattern a layout wraps with its module; `slots` pairs a parallel slot's source id with its page module; `intercepts` pairs `<pattern> into <slot>` with the `page.<slot>.tsx` module; `sources` and `actions` pair an id with the module that lowered to it; `services` pairs a service with its document; `schemas` pairs a type with its file; `types` pairs a package with `types::status`'s row.
* `Display` prints the six sections in that order, source and action rows labelled `lowered`, service rows `http` or `grpc` by their document's extension.

## 3. Discovery Rules

### Clients

* Every `app/clients/<name>.openapi.json`, sorted by name, is imported with `snapfire_fsr_service::import` as service `<name>`. Its types and services are merged into one contract; a type name that two documents both define is `DuplicateType`.
* Every `clients/*.proto`, in name order after the OpenAPI documents, is imported with `snapfire_fsr_service::import_proto` under its file stem the same way. `CONTRACTS_DIR` and `PLAN_FILE` name where the build writes.

### Schemas

* Every `app/schemas/*.ts`, sorted by name, is read with `snapfire_fsr_lower::read_schema`. Each exported interface or string-literal union becomes a contract type; a name declared twice is `DuplicateType`.
* The type named `Session` is imported into `generated/fsr.ts` from its file and types `ctx.session`; without one, `session` is `Record<string, unknown>`. An `export const defaults` in that file is read with `read_session_defaults` and folded into every lowered session read.
* After both, `Contract::validate` runs; an unresolved reference is `BuildError::Contract`.

### Extensions

* Every `app/ext/*.ts`, sorted by name, is lowered with `ComponentSet::lower_extensions` before any route, so a native pair is declared before a body or a component calls it; each export is a `Report.extensions` row and one that does not lower fails the build with the lowerer's `Extension` error. `@ext/<name>` reaches the module from anywhere under the app; `ext/**/*` is in every generated tsconfig.
* Loaders, metas, stores, actions, handlers and middleware are lowered through the same `ComponentSet`, so a body follows the imports it calls; a name the build cannot follow is the residue the lowerer gives, at the line.
* A `body` extension on a component's render path fails the build with the lowerer's `Reach` error, never a `client` row.

### Routes

* A directory under `routes/` is a route when it contains `page.tsx` or `page.ts` and a handler route when it contains `route.ts`. A `layout.tsx` in any directory on the way from `routes/` to a route wraps that route's page, outermost first. One holding both is `BuildError::PageAndRoute`. Other directories contribute path segments only.
* `slots/<name>/` beside a `layout.tsx` is a parallel slot of that layout, a child in the slot `<name>` of every route under it, with `page.tsx`, `page.loader.ts`, `loading.tsx` and `error.tsx` read the way a route's are and the source id `layout.<name>` (`<layout id>.<name>` for a nested layout). It is not a route: `slots/` elsewhere is `BuildError::SlotsWithoutLayout`, a slot without `page.tsx` is `SlotWithoutPage` and one with a page or handler directory beneath it is `SlotRoute`. A layout also declares every slot its template places with `<Slot name>`.
* `page.<slot>.tsx` beside a route's `page.tsx` is an intercept: an entry under the route's pattern in the manifest's `intercepts`, holding the layouts down to the nearest one declaring `<slot>`, that layout with the variant as its `<slot>` child and its page and every other slot in `keep`, and each layout above with its own slots in `keep`. The variant shares the route's source and error module and streams behind `loading.<slot>.tsx` alone. A route with several variants has one entry each, in file order. A slot no layout above declares is `SlotUndeclared`.
* Node ids are assigned in tree order per plan, the shell at 0.
* A segment is a name of ASCII letters, digits, `_` and `-`, `[name]` for a parameter or `[...name]` for a catch-all. Anything else is `BuildError::Segment`.
* `index` as the first segment is the root. `index` deeper in a path is a literal segment.
* Routes are sorted by pattern before ids are assigned.

### Ids

* Source id: every segment joined with `.`; `index` for the root. A parameter contributes `$<name>`, a catch-all `$<name>` too, so `routes/product/[id]` is `product.$id` and `routes/docs/[...rest]` is `docs.$rest`. The marker is what makes an id injective: a directory name is alphanumerics, `_` and `-` only, so no static segment can produce a `$` part and a route can never share an id with its parameterised child.
* Action id: `<source id>.<export>` for each export `lower_actions` returns.
* Layout id: `layout` for `routes/layout.tsx`, `<segments joined with .>.layout` deeper, parameters marked the same way; it names the layout's loader as a source.
* Two rows deriving one id stop the build with `ClaimedId`, naming the kind, the id and both files. Route, source, action, handler and props-type names are each checked. The marker keeps ids apart but `props_name` drops it, so `routes/a/x` beside `routes/a/[x]` builds two distinct ids and one type name, and is refused on that.
* A component placed as an island in server mode stops the build with `ServerIsland { module, reason }` when one of its handlers did not lower, naming the placing module, the line and why, or when a component it renders has state or handlers of its own, naming it.
* Handler id: `<route id>.<METHOD>` for each export of `route.ts` named `GET`, `POST`, `PUT`, `PATCH` or `DELETE`; the row also carries the method and the pattern. An `action<T>` export names `T` as its input, which must be a schema type or the build fails with `UnknownHandlerInput`.

### Modules

* Page: `<route dir>/page.tsx#default`, with the directory relative to `app`.
* Error: `routes/error.tsx#default` (or `.ts`) when present, applied to every page; a route's own `error.tsx` takes precedence for that route.
* Loading: `<route dir>/loading.tsx#default` when present; the node is marked deferred with it as the fallback.
* Not found: `routes/not-found.tsx#default` (or `.ts`) when present, the page for a path no route matches.
* Layout: `<dir>/layout.tsx#default`, its loader `<dir>/layout.loader.ts` as the source row under the layout id, once however many routes it wraps.
* Loader, actions and route modules are named by their relative paths in the source, action and handler rows.
* Middleware: `middleware.ts` at the top of the app, when present, lowered as `Manifest.middleware`. Its exported `middleware` reads `request` (`method` and `path`), which reaches it as the input. It returns nothing or an object naming `redirect`, `rewrite`, `status`, `body` or `headers`.

### Plan shape

* Every route is a chain: node 0 is the shell module; each wrapping layout follows in `Options::slot` for the shell and `content` for a layout, ids counting up from 1, carrying the layout's source and the routes-level error module; the page comes last with its source, its error module and its fallback. A route with no layout is the two-node tree of before.
* `not_found` is the same chain around the not-found module, inside the root layout when there is one, with the routes-level error module and no source, present only when the module is; the host renders it with status 404 and `params.path` set to the path asked for.
* Sources and actions are emitted with `RowOwner::Lowered` and their bodies. No other owner is produced.
* An action whose `action<T>` names a type the contract lacks is `UnknownInput`; an action row carries `input` when it names one.

### Generated files

* `generated/contracts/<client>.json` is `Contract::to_json` of that document's import, types and service; `generated/contracts/schemas.json` holds the schema types. `CONTRACTS_DIR` names the directory. The build merges them with `Contract::merge` for `services.d.ts`, `client.ts` and validation, so a type two documents define fails the build naming the second; `write` empties the directory of `*.json` before writing so a removed client leaves nothing behind.
* `generated/fsr.ts` declares `Routes` with one key per page pattern and per handler pattern, so `Ctx<"/api/cart">` types a handler's parameters, plus `RequestLine`, `MiddlewareCtx` and `MiddlewareResult` for `middleware.ts`, and `Meta`, `MetaCtx<Data>` and `DataOf<typeof load>` for a loader module's `meta`. A loader module's exported `meta` is lowered beside `load` into the source row's `meta`, and its exported `store` into the row's `store`, both functions of the data `load` returned. `generated/client.ts` types a layout's props from its loader the way it types a page's: `LayoutProps` for the root, `AccountLayoutProps` for `routes/account/layout.tsx`.
* `generated/services.d.ts` is `snapfire_fsr_service::typescript::declarations` of it.
* `generated/islands.ts` imports `registerIsland` and the mounter and exports `registerIslands()`, one call per module, each with `mount` and `patch` from `Options::mounter_module`: the routes-level error module, the not-found module, each layout, then each page, its error and its loading module, then every component a lowered component places as an island, its loader picking the named export, each loading `../<path>.js` relative to `generated/`.
* `generated/client.ts` imports `action as call` from `@snapfire/fsr-client`, prints every contract type in client flavour, one `export type <Id>Props` per route from `infer::Inferer::returns` over its loader (`{}` without one) and `export const actions`, nested by the dots of each action id, each `call("<id>") as unknown as (input: <Input>) => Promise<<returns>>`.
* `tsconfig.json` is `types::tsconfig`; `tsconfig.build.json` is `types::tsconfig_build`. Both include `ext/**/*` beside `src/**/*`.
* `.fsr-bundle/<path>` is the browser copy of every lowered component module with a hoist: the source with `hoist::apply` over it, which snapfirec reads through `--overlay` in place of the original. Not for the editor and not for `fsr test`'s Rust side; the plan carries the same decisions as `Expr::Hoist`.
* `generated/fsr.ts` is what the generated `tsconfig.json` maps `@snapfire/fsr` to; it imports the base package as `@snapfire/fsr-authoring`, re-exports `fail` and `Services`, imports `Session`, declares `Routes` with one key per pattern whose value has a `string` field per parameter, `Ctx<P extends keyof Routes = keyof Routes>` with `params`, `query`, `session`, `identity`, `locale`, `services` and `now`, `ActionCtx<Input, P>` and an `action<Input, Out>` wrapper over `@snapfire/fsr`'s.

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
* `SlotsWithoutLayout(PathBuf)`, `SlotWithoutPage(PathBuf)`, `SlotRoute(PathBuf)` and `SlotUndeclared { path: PathBuf, file: String, slot: String }`, from the slot and variant rules above.
* `Spec(String)`, an `fsr add` argument that is not `name@version[/subpath]`.
* `Http(String, String)`, the URL and the failure.
* `Manifest(PathBuf, String)`, a vendor manifest, types manifest, import map or `xwpm.wmf` that did not parse.
* `Serve(String)`, the stock host refusing to build or the listener failing, from `fsr serve`.
* `Dependency { package: String, wants: String }`, a vendored module importing a package outside its bundle.
* `Xwpm(String)`, an `xwpm` command that could not start or failed.
