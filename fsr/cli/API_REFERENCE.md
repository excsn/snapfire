# API Reference: snapfire_fsr_cli

The `fsr` binary and the library build it fronts: route discovery, the contract, lowering, the plan file and the generated TypeScript.

## Contents

* [1. The Binary](#1-the-binary)
  * [fsr new](#fsr-new)
  * [fsr use](#fsr-use)
  * [fsr add](#fsr-add)
  * [fsr types](#fsr-types)
  * [fsr build](#fsr-build)
  * [fsr check](#fsr-check)
  * [fsr doctor](#fsr-doctor)
  * [fsr bundle](#fsr-bundle)
  * [fsr serve](#fsr-serve)
  * [Sites](#sites)
* [2. The Build](#2-the-build)
  * [Options](#options)
  * [ShellContract](#shellcontract)
  * [SiteOptions](#siteoptions)
  * [build](#build)
  * [Built](#built)
  * [write](#write)
  * [write_overlay](#write_overlay)
  * [emit](#emit)
  * [Report](#report)
* [3. Discovery Rules](#3-discovery-rules)
  * [Clients](#clients)
  * [Schemas](#schemas)
  * [Extensions](#extensions)
  * [Elements](#elements)
  * [Routes](#routes)
  * [Ids](#ids)
  * [Modules](#modules)
  * [Plan shape](#plan-shape)
  * [Generated files](#generated-files)
* [4. Inference](#4-inference)
* [5. Typechecking](#5-typechecking)
  * [Typecheck](#typecheck)
  * [Checked](#checked)
  * [spawn, finish, run](#spawn-finish-run)
  * [find_checker](#find_checker)
  * [record](#record)
* [6. Vendoring and Declarations](#6-vendoring-and-declarations)
  * [Layout](#layout)
  * [Spec](#spec)
  * [add](#add)
  * [Direction](#direction)
  * [adopt](#adopt)
  * [fetch](#fetch)
  * [tsconfig](#tsconfig)
  * [Manifests](#manifests)
  * [Ts](#ts)
  * [Inferer](#inferer)
* [7. Error Handling](#7-error-handling)
  * [BuildError](#builderror)

## 1. The Binary

Every command parses its arguments with clap, so each takes `--help` and a flag given without its value is an error rather than a swallowed argument. A usage error exits 2.

### fsr new

* `fsr new <project dir> [--with <direction>]... [--no-fetch] [--shell] [--site --at <path> [--name <name>] [--into <shell dir>]]`
* Writes `config/app.toml`, `.gitignore` and the smallest application the stock host serves under `app/`: an import map naming the client, `/std` and `/store` and nothing else, an entry module, a root layout, an index page and its loader, a not-found page, an error page and a stylesheet. The application is bare: it vendors no framework, its layout imports `Link` and `Children` from `@snapfire/fsr-authoring/template` and its pages render with nothing in the serving path. Refuses a directory that already holds `app/` or `config/`.
* `--with <direction>`, repeatable, runs `direction::adopt` over the written project for each name in order, so `fsr new shop --with react --with htmx` ends with the map lines, the vendor tree, the declarations and the generated files `fsr use` would have produced one at a time. A direction the table does not hold is refused before anything is written. The template follows the directions where a file of its own is concerned: with `htmx` the entry module imports htmx and calls `bindHtmx` after `enableNavigation`. Every other direction changes nothing in the template, since the layout imports its placements from `@snapfire/fsr-authoring/template`, which types under React as well as under the dialect.
* Then, unless `--no-fetch`: writes the declarations for every import-map package into `app/types/` and runs the generation, which writes `app/generated/` and both tsconfigs. That last step is what makes the scaffold resolve in an editor without a build first, since the routes import `@snapfire/fsr` and `@generated/client`, neither of which the template carries. The browser bundle is not built; `fsr dev` writes `dist/`.
* `--no-fetch` writes the template and the directions' map lines alone and names `fsr add` for what the directions vendor, `fsr types` and `fsr build` as the steps to run.
* `--shell` gives the configuration a `[sites]` table. `--site` gives it a `[site]` section and needs `--at`; `--into` writes both halves of the mount through `sites::link` instead. The two are refused together.
* Prints `wrote <path>` per file, `added <specifier> <file> <bytes>` per vendored module, `types <package> <from> <version>` per declaration set, `note <text>` on stderr for a step that failed without stopping the scaffold and `next <command>` for what to run. Exit 0 on success, 1 on a `BuildError`.

### fsr use

* `fsr use <app dir> <direction>... [--no-fetch] [--example]`
* `direction::adopt` over the names, in order. Gives an existing application a client direction: `react`, `vue`, `elements`, `htmx` or `tera`. Running it again changes nothing. Prints `mapped <specifier> <url>` per import map line written, `present <specifier>` per line already there, `added <specifier> <file> <bytes>` per vendored module, `shell <specifier> <url>` per package taken from the site's shell, `kept <specifier>` per package the vendor manifest already records at the pinned version, `xwpm add <spec>` per delegated call, `types <package> <from> <version>` per declaration set, `wrote <path>` per example and generated file, `note <text>` on stderr for a step that failed without stopping the command, `edit <file>: <line>` for what the application changes by hand and `next <command>` for what to run. Exit 0 on success, 1 on a `BuildError`.
* `--example` writes one component per direction and prints where to place it. A file already there is refused before anything is written.
* `--no-fetch` writes the map lines and the examples alone and names `fsr add`, `fsr types` and `fsr build` as the steps to run.

### fsr build

* `fsr build <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>] [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]`
* Runs the build, prints the report to stdout, writes `<app dir>/generated/plan.sexp`, `generated/contracts/<client>.json` per document and `generated/contracts/schemas.json`, `generated/native.d.ts`, `generated/services.d.ts`, `generated/elements.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json` and `tsconfig.build.json`, prints `wrote <path>` for each, then bundles the browser modules into `<app dir>/dist/` with `snapfirec`.
* The bundle follows the generation because it compiles the island registry the generation writes. `--public-path` defaults to `/static/js/app` or `<at>/static/js/app` for a site; `--snapfirec` defaults to `$SNAPFIREC`, else beside this binary, else `PATH`.
* Exit 0 on success, 1 on any `BuildError`, 2 on a usage error.
* The typecheck prints one `typecheck <row>` line, a `recorded` line when it wrote the version into the configuration and nothing at all when no checker is installed beyond a note on stderr.

### fsr check

* `fsr check <app dir> [--shell <module id>] [--slot <name>] [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]`
* Runs the build and prints the report; writes nothing, so the typecheck reads whichever `tsconfig.json` is on disk. Same exit codes, plus 1 when a diagnostic is an error.

### fsr doctor

* `fsr doctor <app dir>`
* `doctor::run(app: &Path) -> Result<Report, DoctorError>`: reads the configuration and the plan the way the host would, runs every check and returns what it found. Needs nothing running. Exit 0 when clean, 1 when anything was found, 2 when the configuration or the plan could not be read.
* `Report { findings: Vec<Finding>, clean: Vec<&'static str> }`, `Report::is_clean`; `Display` prints one finding per pair of lines, the fact then the remedy, followed by a count.
* `Finding { check: &'static str, what: String, remedy: String }`. `check` is the short name a report can be grepped for.
* The checks, each answering from what a build already computed: `canonical`, `[document] origin` unset while the deployment names hosts or prerenders; `ctx.host`, a body reading `ctx.host` against an empty `[server] hosts`; `ctx.config`, a body reading a `ctx.config` key `[public]` does not declare; `locales`, a supported locale with no catalog under `locales/`; `stale`, a plan missing or older than `routes/`, `src/`, `clients/` or `schemas/`; `vendor`, an import map naming a package with nothing under `vendor/`; `render`, `[server] render` set to `islands` on a plan with no island; `statics`, a `[[static]]` root with no directory; `shadow`, a static route swallowing a route pattern; `bearer`, a client carrying a token with no `[auth]` provider to write one; `cache.tags`, a tag dropped by a call and cached by none; `links`, a literal `href` matching no route, static root or mounted site, skipped for a site; `tree`, a required file a deploy tree would carry that the project does not hold; a configuration `snapfire_fsr_sites::layout` refuses; `sites`, a `name@version` mount pinning no hash, an artifact missing a part it ships or missing its plan, a site plan older than its routes and artifacts under the root no mount names.
* Reports only. A condition the host refuses to start over stays a boot error rather than moving here. `fsr bundle` calls this before it writes, so a deploy that ends in a bundle needs no separate step.

### fsr bundle

* `fsr bundle <app dir> [--out <dir>] [--no-doctor]`
* `bundle::run(app: &Path, out: &Path) -> Result<Bundled, BuildError>`: the deploy tree, `dist/` beside the project by default. The layout is `snapfire_fsr_sites::layout`: configuration under `config/`, everything the host reads at boot under `app/`, every static root under `serve/<route>/` for a web server to point at and a generated `config/bundle.toml` naming the paths that moved. Every destination is derived from what a file is rather than from where it sat in the project, so no configured path is joined onto `out` and nothing can be written outside it.
* Runs `doctor::run` over the project before writing anything and returns `BuildError::Doctor(Report)` on a finding, which carries the findings rather than a summary so the caller prints the remedies. Nothing is written when it refuses.
* `bundle::run_checked(app, out, check)` is the same with the check optional, which is `--no-doctor`.
* `Bundled { out, served: Vec<(String, String)>, read: Vec<String>, files: usize, bytes: u64, beside: Vec<&'static str> }`. `served` is each route with the directory under `serve/` that answers it; `read` is the placements that had something to place, tree-relative. `bundle::SERVE` is `"serve"`, the one directory a web server is pointed at.

### fsr serve

* `fsr serve <app dir> [--listen <addr>]`
* `serve::run`: the stock host, `snapfire_fsr_host::Host`, over the configuration `serve::project_root` finds, the directory beside the app when it holds `config/`, `app.toml` or `app.yaml`, else the app itself; prints the host's report and listens on `--listen` or the configured `server.listen` until the process ends. Refuses a configuration whose `[app] dir` is not the app given. Exit 1 on a `BuildError::Serve`.
* `serve::run` installs the trace collector (`snapfire_fsr_host::trace::install`) and passes it to the builder when `config.dev()`, so `GET /__fsr/traces` answers with what the last requests did; the banner's second line names the endpoint. A host that is not a development one collects nothing.
* `serve::host_for(app: &Path) -> Result<Host, BuildError>` is the same host without the listener or a collector: `fsr prerender` builds through it.
* `serve::prerender(app: &Path, out: Option<&Path>) -> Result<Vec<(String, PathBuf)>, BuildError>` builds that host and calls `Host::prerender` with `out`, else `server.prerender`, else `dist/prerender` under the app, one rendering per configured locale; `fsr prerender` prints what it wrote or that nothing qualifies.
* `fsr dev <app dir>` runs this in place of `cargo run` when no `Cargo.toml` is beside the app, watching `config/`, `app.toml` and `app.yaml` there instead of `src/`. The host it builds carries a reloader that rereads the project, so a change to the generated files is `POST /__fsr/reload` to `server.listen`, printing the report it answers with and the process restarts only when the reload is refused, a changed `[session]` for one; after a rebundle alone it posts `/__fsr/changed`, best effort, so open development documents refresh. A reload waits up to ten seconds for a server that is still starting to listen.
* `dev::run(app: &Path, options: DevOptions) -> Result<(), BuildError>`: the loop. Builds the shell and every site its `[sites]` table mounts from a path, each with its own `snapfirec --driven` child told which paths changed, then `cargo build --message-format=json-render-diagnostics` when a `Cargo.toml` is beside the shell, watching what the project's build scripts declared with `rerun-if-changed` and spawning the one binary the build reported, else `cargo run`. Changed paths are classified per application by prefix, with `generated/`, `dist/`, `types/`, both tsconfigs, `tests/`, `*.test.ts` and `.fsr-*` under any of them ignored; anything under no application is the project. A file whose content hashes as it did the last time the loop looked is not a change. Events already queued when the loop comes back for them are one batch; three such batches in a row that still call for a rebuild stop the loop until an event arrives while it waits. Each rebuild prints `dev: changed <paths>`.
* `dev::OWNS_BUILD`: `"FSR_DEV_OWNS_BUILD"`, set in the environment of the cargo commands `dev::run` spawns. `dev::owns_build() -> bool` reads it, for a build script that would otherwise call `emit`.

### Sites

* Every command reads the `[site]` section of the configuration beside the app (`site_beside`) and builds with it; `fsr dev` then serves the bundle under `<at>/static/js/app`.
* `fsr serve` mounts every site the shell's `[sites]` table names, `snapfire_fsr_sites::mount_all`, sets a reloader that mounts them again and watches the table, `snapfire_fsr_sites::watch`, on `SIGHUP` and on `sites.poll`.
* `fsr sites install <shell dir> <archive> [--as <name>] [--keep <n>] [--no-pin]`: `sites::install(shell, archive, name, keep, pin) -> Result<Installation, BuildError>`. Unpacks into the cache, then writes `hash` into the mount that names that exact version. `Installation { installed: snapfire_fsr_sites::Installed, pinned: Option<Pinned> }`; `pinned` is `None` when nothing is mounted at that version yet or `--no-pin` was given.
* `fsr sites list <shell dir> [--host <url>]... [--header "K: V"]...`: with no host this is the table from disk. With hosts it is `sites::compare(shell, hosts, headers) -> Result<Vec<Compared>, BuildError>`, every row beside what each instance answers on `GET /__fsr/sites`, marked `ok`, `lags` or `absent`. `Compared { row: Row, against: Vec<(String, Option<Mounted>)> }` with `Compared::agrees()`. Exit 1 when any instance disagrees.
* `fsr sites reload [<shell dir>] [--host <url>]... [--header "K: V"]... [--all]`: `sites::reload(hosts, headers, all) -> Result<Vec<Reloaded>, BuildError>`, `POST /__fsr/sites/reload` on each instance in turn. Stops at the first refusal unless `--all`, since a `409` says what was published is bad. `Reloaded { host, sites: Option<Vec<Mounted>>, refused: Option<String> }` with `Reloaded::ok()`. Exit 1 when any refused.
* `sites::hosts_for(shell, given)` is where to ask: every `--host`, else the shell's `server.listen`. `sites::header("Name: Value")` parses one header and refuses anything else. `$FSR_SITES_HEADER` joins the given ones. Both commands forward credentials and create none: `/__fsr/` is not guarded by the framework.
* `sites::mounted(host, headers) -> Result<Instance, BuildError>` is one instance on its own. A `404` from either route is an error saying the host was built without the `sites_reload` feature or installed no sites mounter, rather than a wrong URL.
* `fsr sites pin <shell dir> [<name>]`: `sites::pin(shell, only) -> Result<Vec<Pinned>, BuildError>`. Hashes what each mount resolves to and writes `hash` into `[sites.<name>]`, replacing the line when it is there. `Pinned { name, hash, was: Option<String>, artifact }` with `Pinned::moved()` for whether the file changed. Only a `name@version` artifact is pinned; a path mount is a working tree and is skipped, so pinning a table of those returns empty. Errors on a name the table does not mount.

### fsr add

* `fsr add <app dir> <name@version[/subpath]>... [--external <name,...>]`
* `vendor::add` over the specs; prints `added <specifier> <file> <bytes>` per entry, `remapped <specifier> <url>` per entry moved to the layout's base or `xwpm add <spec>` per delegated call. Same exit codes; a spec without a version exits 2.

### fsr types

* `fsr types <app dir> [--refresh]`
* `types::fetch`; prints `ran <command>` for each delegated xwpm command, `types <package> <from> <version>` per fetch, `kept <package>`, `missing <package> <why>` and `wrote <path>` for `<types>/foreign.d.ts` when a source is foreign and for `tsconfig.json`, which it writes from what it fetched. Exit 0 with missing packages, 1 on a `BuildError`.

## 2. The Build

### Options

* `pub struct Options { pub site: Option<SiteOptions>, pub shell: String, pub slot: String }`
* `Default` is no site, `shell#document` and `content`.

### SiteOptions

* `pub struct SiteOptions { pub name: String, pub at: String, pub shell: Option<PathBuf> }`: the `[site]` section as the build reads it; `prefix()` is `<name>:` and `under(path)` is `at` joined with a path.
* `BUNDLE_BASE` is `/static/js/app` and `VENDOR_BASE` is `/static/js/vendor`. `bundle_base(site: Option<&SiteOptions>) -> String` and `vendor_base(site: Option<&SiteOptions>) -> String` put each under the site's prefix, which is where a mount keeps a static root. `fsr dev`, `fsr test` and `Layout::of_site` take their paths from them.
* `Options::beside(app: &Path) -> Options`: the defaults with `site` from the configuration beside `app` when one names that app directory. `site_beside(app: &Path) -> Option<SiteOptions>` is that lookup alone. `Options::prefix()` is the prefix on every emitted id, empty without a site.
* `unprefixed(service: &str) -> &str`: a service name without its site prefix, which is what a test mocks it by.

### ShellContract

* `pub struct ShellContract { pub version: u32, pub store: BTreeMap<String, String>, pub imports: BTreeMap<String, String>, pub frameworks: BTreeMap<String, String>, pub fsr: String }`: `generated/shell.json`. `frameworks` is the exact version the shell vendors of every framework package a client adapter imports, `react`, `react-dom` and `vue`, read from its vendor manifest: what a site built against it renders under and what a site's specs fetch development builds at. Every field but `version` defaults, so a contract written before `frameworks` existed still reads. `SHELL_CONTRACT_VERSION` is 1.
* `ShellContract::read(path: &Path) -> Result<ShellContract, BuildError>`: refuses a version this fsr does not read.
* `declarations(&self) -> String`: `generated/shell.d.ts`, `ShellStore` and `ShellImport`.

### build

* `pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError>`
* With `options.site` set, after every generated TypeScript is written unprefixed, the plan file is `Manifest::namespaced`, every contract file `Contract::namespaced`, the islands registry registers `<name>:<module>` and the generated call sites call `<name>:<action id>`; with `site.shell` set, `generated/shell.d.ts` is written from the shell contract. Without a site, `generated/shell.json` is written: every store key a `store` export seeds, typed by inferring the loader's return and then the store body, the app's import map, the version it vendors of every framework package a client adapter imports and the fsr version.
* Imports `app/clients`, reads `app/schemas`, validates the contract, walks `app/routes`, lowers every `page.loader.ts` and `actions.ts` and returns everything without writing. The first error in any file fails the whole build.

### Built

* `pub struct Built { pub manifest: Manifest, pub contract: Contract, pub report: Report, pub files: Vec<(String, String)>, pub defaults: SessionDefaults, pub browser_routes: Vec<String> }`
* `browser_routes` are the route modules the browser mounts, as files relative to the app: every route module that is not `static`, which is all of `routes/` a bundle compiles.
* `files` pairs a path relative to the app directory with its content: `generated/plan.sexp`, `generated/contracts/<client>.json` per document in name order, `generated/contracts/schemas.json`, `generated/native.d.ts`, `generated/services.d.ts`, `generated/elements.d.ts`, `generated/fsr.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json`, `tsconfig.build.json`, in that order, then `<types>/foreign.d.ts` when a source or a placement is a component in a language the build does not read, declaring `*.<ext>` for the typechecker; `write` removes a `generated/foreign.d.ts` left by an earlier build.
* `generated/native.d.ts` is read off the Rust rather than the contract: `native::read` walks the crate's `src/`, the sibling of the app directory, with `syn` and takes every `#[native]` `impl` block's `pub` methods plus the structs they name. It reads rather than expands, so `build.rs` can run it before the crate compiles. A method the reader saw as `fn` is typed as its value and an `async fn` as a promise; a Rust type outside the value model reads as `unknown`.

### write

* `pub fn write(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError>`
* Removes every `*.json` under `generated/contracts/` and the whole `.fsr-bundle/` directory, then writes every entry of `built.files` under `app`, creating directories as needed. Returns the paths written.

### write_generated

* `pub fn write_generated(app: &Path, built: &Built) -> Result<(), BuildError>`
* Writes only the `generated/` entries of `built.files`, which is what the browser half of a test compiles against, so a run sees the build it was given rather than the last `fsr build`'s. `fsr test` calls it before compiling.

### write_overlay

* `pub fn write_overlay(app: &Path, built: &Built) -> Result<(), BuildError>`
* Removes `.fsr-bundle/` and writes only the entries of `built.files` under it: the sources the build rewrote for the browser. `write` includes them; this is for a caller that compiles without writing the rest, which is what `fsr test` does.
* `dev::BUNDLE_OVERLAY` is the directory's name, `.fsr-bundle`.

### emit

* `pub fn emit(app: &Path, options: DevOptions) -> Result<Emitted, BuildError>`
* `pub struct Emitted { pub built: Built, pub written: Vec<PathBuf>, pub checked: Option<typecheck::Checked> }`
* `build`, then `write`, then `snapfirec` over `tsconfig.build.json` into `<app>/dist` with `options.public_path`, the layout's import map and `--overlay .fsr-bundle` when the build wrote one, so a rewritten source is compiled in place of its original at the same path. The order is load-bearing: the bundle compiles the island registry the generation writes.
* The whole artifact a host reads and what a `build.rs` calls when `dev::owns_build()` is false. `build` and `write` alone leave `dist/` at whatever the last bundle wrote, which the host cannot distinguish from a current one.
* The typecheck runs beside the bundle rather than after it, since neither reads the other's output and `Emitted::checked` carries what it found. `BuildError::Typecheck` when a diagnostic is an error, carrying the row and every diagnostic.
* `Dev` naming the compiler when it cannot start or its exit status when it fails.

### Report

* `shell: Option<(String, usize, usize, Vec<String>)>`: for a site built against a shell contract, its path, its store key and import counts and the site's import map entries that differ; `Display` prints a `shell` row and a second naming the differences.

* `pub struct Report { pub routes: Vec<(String, String)>, pub layouts: Vec<(String, String)>, pub slots: Vec<(String, String)>, pub intercepts: Vec<(String, String)>, pub sources: Vec<(String, String)>, pub actions: Vec<(String, String)>, pub handlers: Vec<(String, String)>, pub middleware: Option<String>, pub components: Vec<(String, String, String)>, pub hoisted: Vec<(String, usize)>, pub services: Vec<(String, String)>, pub schemas: Vec<(String, String)>, pub types: Vec<(String, String)> }`
* `components` rows are module, owner and detail: owner `lowered` or `client`; for `client`, the detail is the residue's `file:line:column`; for `lowered`, the detail is `static` when the template has no state, no handlers and no component inline that has them, so nothing mounts it; otherwise it is empty.
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
* Loaders, metas, stores, paths, actions, handlers and middleware are lowered through the same `ComponentSet`, so a body follows the imports it calls; a name the build cannot follow is the residue the lowerer gives, at the line.
* A `body` extension on a component's render path fails the build with the lowerer's `Reach` error, never a `client` row.

### Elements

* Every `app/elements/<tag>.tsx`, sorted by name, is a custom element's shadow template: its default export renders with the element's attributes as its props. The file name is the tag; one that is not lowercase, starting with a letter and holding a hyphen is `BuildError::ElementName`.
* Each template is lowered before any route and must be static. State or a handler is `BuildError::ElementTemplate`, since an element's behaviour is its class's. It is lowered without hoisting, so it has no `Report.hoisted` row and no rewrite under `.fsr-bundle/`.
* A template whose root is a `<template shadowrootmode>` declares the element's shadow root. `ShadowRoot::take` reads it into the component's `shadow` and the template's children become its render, so the plan carries `(shadow closed delegatesfocus)` in place of the element. A root template it refuses is `BuildError::ElementTemplate` with the refusal as the reason.
* A placement of that tag anywhere carries `render::SHADOW_ATTR` naming the template's module. The server writes the template inside the element as a declarative shadow root before its light children. The wrapper is `<template shadowrootmode="open">` unless the template declares its own root. A non-scalar attribute reaches the template without being written on the host.
* `elements/**/*` is in the generated `tsconfig.json`, so the templates typecheck. Nothing under `elements/` is compiled for the browser.
* `generated/elements.d.ts` is `types::element_declarations`: each template's tag typed as the template's props over the host's attributes, plus any other attribute. With `react` in the import map it augments `React.JSX.IntrinsicElements` and adds a `` `${string}-${string}` `` entry for a tag with no template, typed as the host's attributes less every name a template declares, plus any other attribute. Without it, it fills `ElementTemplates` in `@snapfire/fsr-authoring/template`. The dialect's `JSX.IntrinsicElements` is `Intrinsic & ElementTemplates`, so a template's tag takes its type from its entry alone.

### Routes

* A directory under `routes/` is a route when it contains `page.tsx`, `page.ts` or, with the `tera` feature, `page.tera` and a handler route when it contains `route.ts`. A `layout.tsx` or `layout.tera` in any directory on the way from `routes/` to a route wraps that route's page, outermost first. One holding both a page and a handler is `BuildError::PageAndRoute`; one holding two page files or two layout files is `PageAndTemplate` naming both; a template in an fsr built without the feature is `TemplateFeature`, so a template never becomes a silent 404. A template's module is `routes/<dir>/page.tera#default`: nothing is lowered for it, no component row is written, it is never bundled and the report lists it under `rendered` as `template`. Its `page.loader.ts` and `actions.ts` lower as beside a `page.tsx` and its context is the loader's returned object. Every `island(` call in a template whose `module` is a string literal is bundled, registered and, for server mode, lowered the way a TSX placement's is; one whose `module` is not a literal is `TemplateIsland` naming the line. A `page.<slot>.tsx` variant under a `layout.tera` is refused as undeclared, since the build cannot read which slots a template places; a `slots/<name>/` directory beside it is placed by `slot(name="<name>")` in the template. An `actions.ts` is read beside a page or in a slot; one in a directory with no page is `BuildError::ActionsWithoutPage`, since an action is named for its page's route id. Other directories contribute path segments only.
* `slots/<name>/` beside a `layout.tsx` is a parallel slot of that layout, a child in the slot `<name>` of every route under it, with `page.tsx`, `page.loader.ts`, `loading.tsx` and `error.tsx` read the way a route's are and the source id `layout.<name>` (`<layout id>.<name>` for a nested layout). It is not a route: `slots/` elsewhere is `BuildError::SlotsWithoutLayout`, a slot without `page.tsx` is `SlotWithoutPage` and one with a page or handler directory beneath it is `SlotRoute`. A layout also declares every slot its template places with `<Slot name>`.
* `page.<slot>.tsx` beside a route's `page.tsx` is an intercept: an entry under the route's pattern in the manifest's `intercepts`, holding the layouts down to the nearest one declaring `<slot>`. That layout carries the variant as its `<slot>` child, with its page and every other slot in `keep`; each layout above it carries its own slots in `keep`. The variant shares the route's source and error module and streams behind `loading.<slot>.tsx` alone. A route with several variants has one entry each, in file order. A slot no layout above declares is `SlotUndeclared`.
* Node ids are assigned in tree order per plan, the shell at 0.
* A segment is a name of ASCII letters, digits, `_` and `-`, `[name]` for a parameter or `[...name]` for a catch-all. Anything else is `BuildError::Segment`.
* `index` as the first segment is the root. `index` deeper in a path is a literal segment.
* Routes are sorted by pattern before ids are assigned.

### Ids

* Source id: every segment joined with `.`; `index` for the root. A parameter contributes `$<name>`, a catch-all `$<name>` too, so `routes/product/[id]` is `product.$id` and `routes/docs/[...rest]` is `docs.$rest`. The marker is what makes an id injective: a directory name is alphanumerics, `_` and `-` only, so no static segment can produce a `$` part and a route can never share an id with its parameterised child.
* Action id: `<source id>.<export>` for each export `lower_actions` returns.
* Layout id: `layout` for `routes/layout.tsx`, `<segments joined with .>.layout` deeper, parameters marked the same way; it names the layout's loader as a source.
* Two rows deriving one id stop the build with `ClaimedId`, naming the kind, the id and both files. Route, source, action, handler and props-type names are each checked. The marker keeps ids apart but `props_name` drops it, so `routes/a/x` beside `routes/a/[x]` builds two distinct ids and one type name and is refused on that.
* A component placed as an island in server mode stops the build with `ServerIsland { module, reason }` in two cases: a handler that did not lower, naming the placing module, the line and why; or a component it renders that has state or handlers of its own, naming that component.
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
* `frameworks` holds the exact version of every package a client adapter imports: `react`, `react-dom` and `vue`. A site takes each version from the shell contract's `frameworks`, since the shell's import map overrides the site's and the browser loads one copy; every other application takes it from `vendor/.fsr-vendor.json`. The shell contract carries this same map, so the two cannot drift. A site vendoring a version its shell does not serve is `FrameworkShellMismatch`. An import map serving a framework package with no version recorded anywhere is `FrameworkShellUnrecorded` when the shell is what serves it, `FrameworkUnrecorded` otherwise. A React major other than 18 or 19 is `ReactMajor`.

### Generated files

* `generated/contracts/<client>.json` is `Contract::to_json` of that document's import, types and service; `generated/contracts/schemas.json` holds the schema types. `CONTRACTS_DIR` names the directory. The build merges them with `Contract::merge` for `services.d.ts`, `client.ts` and validation, so a type two documents define fails the build naming the second; `write` empties the directory of `*.json` before writing so a removed client leaves nothing behind.
* `generated/fsr.ts` declares `Routes` with one key per page pattern and per handler pattern, so `Ctx<"/api/cart">` types a handler's parameters, It also declares `RequestLine`, `MiddlewareCtx` and `MiddlewareResult` for `middleware.ts`, then `Meta`, `MetaCtx<Data>` and `DataOf<typeof load>` for a loader module's `meta`. `Config` has one field per `[public]` key typed from its value and is what `ctx.config` is. A loader module's exported `meta` is lowered beside `load` into the source row's `meta` and its exported `store` into the row's `store`, both functions of the data `load` returned. `generated/client.ts` types a layout's props from its loader the way it types a page's: `LayoutProps` for the root, `AccountLayoutProps` for `routes/account/layout.tsx`.
* `generated/services.d.ts` is `snapfire_fsr_service::typescript::declarations` of it.
* `generated/islands.ts` imports `registerIsland` and each adapter a registered module needs and exports `registerIslands()`, one call per module, each with `mount`, `patch` and `unmount` from the adapter its extension names (`@snapfire/fsr-client/vue` for `.vue`, `@snapfire/fsr-client/react` for a module the build lowers): the routes-level error module, the not-found module, each layout, then each page, its error and its loading module, then every component a lowered component places as an island, its loader picking the named export, each loading `../<path>.js` relative to `generated/`.
* An island whose extension a plugin claims (`snapfire_compiler_wire::EXTENSIONS`) but no adapter mounts is `BuildError::NoAdapter`; one whose extension nothing claims is `BuildError::UnknownComponent`. With an import map beside the app, each adapter module the registry imports and the specifiers that adapter imports (`react` and `react-dom/client` for React, `vue` for Vue) must resolve in it or, for a site, in the shell contract's `imports`. A trailing-slash key and a scope both count. Otherwise the build fails with `BuildError::IslandImports`. An app with no import map is not checked.
* `generated/client.ts` imports `action as call` from `@snapfire/fsr-client`, prints every contract type in client flavour, one `export type <Id>Props` per route from `infer::Inferer::returns` over its loader (`{}` without one) and `export const actions`, nested by the dots of each action id, each `call("<id>") as unknown as (input: <Input>) => Promise<<returns>>`.
* `tsconfig.json` is `types::tsconfig(app, true, shim)`; `tsconfig.build.json` is `types::tsconfig_build(app, &built.browser_routes)`. Both include `ext/**/*` beside `src/**/*` when the directory is there.
* `.fsr-bundle/<path>` is the browser copy of every lowered component module with a hoist: the source with `hoist::apply` over it, which snapfirec reads through `--overlay` in place of the original. Not for the editor and not for `fsr test`'s Rust side; the plan carries the same decisions as `Expr::Hoist`.
* `generated/fsr.ts` is what the generated `tsconfig.json` maps `@snapfire/fsr` to; it imports the base package as `@snapfire/fsr-authoring`, re-exports `fail` and `Services`, imports `Session`, declares `Routes` with one key per pattern whose value has a `string` field per parameter, `Ctx<P extends keyof Routes = keyof Routes>` with `params`, `query`, `session`, `identity`, `locale`, `services` and `now`, `ActionCtx<Input, P>` and an `action<Input, Out>` wrapper over `@snapfire/fsr`'s.

## 4. Inference

### Ts

* `pub enum infer::Ts { Str, Num, Big, Bool, Null, Unknown, Named(String), List(Box<Ts>), Map(Box<Ts>), Tuple(Vec<Ts>), Record(Vec<(String, Ts)>), Union(Vec<Ts>), Inter(Vec<Ts>) }`
* `Ts::print(&self, flavour: Flavour) -> String`; `Big` is `bigint` on the server and `bigint | number` on the client; a union or big inside a list or an intersection is parenthesised.

### Inferer

* `pub struct infer::Inferer<'a> { pub contract: &'a Contract, pub session: Option<&'a str>, pub input: Option<&'a str>, pub input_type: Option<Ts>, pub consts: &'a Consts, pub config: &'a [(String, Ts)] }`: `config` is `[public]` as the build read it, what `ctx.config.<key>` is typed as; a key it lacks is `unknown`.
* `Inferer::returns(&self, body: &Body) -> Ts`: the union of every `return`, `Null` when none.
* `Inferer::expr(&self, expr: &Expr, env: &[(String, Ts)]) -> Ts`. Reads type by their root, a call by its method's return, a session key by the `Session` record, `map` by its lambda's body over the element, `filter` by its operand, `Object.entries` as `[string, V][]`, an object literal as a record intersected with its spreads, a coalesce against an empty object or array as its left side. Anything else is `Unknown`, which absorbs a union it joins.

## 5. Typechecking

`fsr` spawns `snapfiretc` and renders what it says; it links nothing of the checker. The surface of the checker itself is in [snapfire_typecheck](../../typecheck/API_REFERENCE.md).

### Typecheck

* `pub struct Typecheck { pub enabled: bool, pub checker: Option<PathBuf>, pub tsc: Option<PathBuf>, pub version: Option<String>, pub expect: Option<String>, pub record: Option<PathBuf> }`
* `Typecheck::beside(app: &Path) -> Typecheck`: the `[typecheck]` section of the configuration beside `app` and `record` set to its first `.toml` source. A project with no configuration is enabled, with the checker's default version and nothing to record into.
* `Default` is disabled with everything absent, so a caller building one by hand opts in; `DevOptions::default` and `DevOptions::beside` both enable it.

### Checked

* `pub struct Checked { pub version: String, pub source: String, pub sha512: Option<String>, pub pinned: bool, pub diagnostics: Vec<Diagnostic>, pub recorded: Option<PathBuf> }`: the checker's JSON report, plus the file this run recorded the version in.
* `pub struct Diagnostic { pub file: Option<String>, pub line: u32, pub column: u32, pub code: String, pub severity: String, pub message: String }`, with `is_error()` and a `Display` printing `file(line,column): severity code: message`.
* `Checked::errors() -> usize` counts the diagnostics whose severity is `error`; `Checked::row() -> String` is the report row, `tsc 7.0.2 from cache, 1 error`.

### spawn, finish, run

* `pub fn spawn(app: &Path, options: &Typecheck) -> Result<Option<Child>, BuildError>`: starts the checker over `<app>/tsconfig.json` with `--format json`. `None` when typechecking is off, the tsconfig has not been written or no checker is installed, which are not failures.
* `pub fn finish(child: Option<Child>, options: &Typecheck) -> Result<Option<Checked>, BuildError>`: waits, reads the report and records the version when `options.version` is `None` and `options.record` names a file. `BuildError::Typecheck` when the child printed nothing or something that is not a report.
* `pub fn run(app: &Path, options: &Typecheck) -> Result<Option<Checked>, BuildError>`: both halves, for a caller with nothing to do meanwhile. `emit` and `fsr dev` use the halves instead, so the check and the bundle run at once.

### find_checker

* `pub fn find_checker(explicit: Option<&Path>) -> PathBuf`: `explicit`, else `$SNAPFIRETC`, else `snapfiretc` beside the running binary, else the bare name for `PATH`. `pub const CHECKER: &str = "snapfiretc"`.

### record

* `pub fn record(path: &Path, version: &str, sha512: Option<&str>) -> Result<bool, BuildError>`: writes `version` into the file's `[typecheck]` section, adding the section when it has none. `false` when the section already names a version, so a pin written by hand is never rewritten.

## 6. Vendoring and Declarations

### Layout

* `pub struct xwpm::Layout { pub vendor: String, pub base: String, pub importmap: String, pub types: String, pub xwpm: bool }`, paths relative to the app directory.
* `Layout::of(app: &Path) -> Result<Layout, BuildError>`: `of_site` with the `[site]` section beside `app`.
* `Layout::of_site(app: &Path, site: Option<&SiteOptions>) -> Result<Layout, BuildError>`: the defaults `vendor`, `/static/js/vendor`, `importmap.json`, `types` and `xwpm: false`; with `<app>/xwpm.wmf` present, its root records with `xwpm: true`. For a site `base` is `<at>/static/js/vendor`, which a `base` record in the wmf overrides.
* `Layout::from_wmf(text: &str) -> Result<Layout, String>`: root records `vendor`, `base`, `importmap` and `types` override the defaults; other records are ignored; sections are skipped; a root line that is not `key = value` is an error naming its line.
* `xwpm::run(app: &Path, args: &[&str]) -> Result<(), BuildError>`: runs `xwpm` in the app directory; `Xwpm` when it cannot start or exits non-zero.
* `xwpm::XWPM_FILE` is `xwpm.wmf`.

### Spec

* `pub struct vendor::Spec { pub package: String, pub version: String, pub subpath: Option<String> }`
* `Spec::parse(raw: &str) -> Result<Spec, BuildError>`: `name@version`, `name@version/subpath`, `@scope/name@version[/subpath]`; anything else is `Spec`.
* `Spec::specifier(&self) -> String`: `package` or `package/subpath`, the import map key.

### add

* `pub fn vendor::add(app: &Path, specs: &[Spec], externals: &[String]) -> Result<AddReport, BuildError>`
* Under the default layout, per spec: `GET https://esm.sh/<package>@<version>[/<subpath>]?target=es2022&bundle[&external=<externals>]`, follows every absolute path the stub names, writes each file by its base name under `<vendor>/<package>/`, rewrites same-package absolute imports to `./<name>` and fails with `Dependency` on any other; the file behind the stub's `export *` becomes the entry, written to the import map as `<base>/<package>/<name>` and to the vendor manifest. Every entry the manifest already records whose map URL does not sit under `base` is rewritten to it and reported as `remapped`; a specifier the map does not carry is left out, since the manifest can name a package whose files the tree no longer holds. Under xwpm: `xwpm add <package>@<version>` once per distinct package and version.
* In a site whose shell already serves a specifier, nothing is fetched for it: the map takes the shell's URL and the specifier is reported as `from_shell`. A version other than the one the contract records for that package is `ShellPinned`, since the shell's map overrides the site's at mount.
* `pub struct AddReport { pub added: Vec<(String, String, usize)>, pub remapped: Vec<(String, String)>, pub from_shell: Vec<(String, String)>, pub delegated: Vec<String> }`: specifier, file relative to the vendor directory and bytes; specifier and the URL it now carries; specifier and the shell's URL it took instead of vendoring; or the xwpm invocations run.
* `vendor::read_import_map`, `vendor::write_import_map`, `vendor::import_map_packages(app, &layout)`: the `imports` table whole and its bare keys as package names (`react/jsx-runtime` is `react`, `@a/b/c` is `@a/b`).
* `vendor::package_of(specifier: &str) -> String`.
* `vendor::ESM_HOST` is `https://esm.sh`.

### Direction

* `pub struct direction::Direction { pub name: &'static str, pub entry: Option<&'static str>, pub vendors: &'static [(&'static str, &'static str, Option<&'static str>)], pub edits: &'static [(&'static str, &'static str)] }`: the name `fsr use` takes, the client entry under `@snapfire/fsr-client/` the import map names when the direction has a browser half, the package, version and subpath of each module it vendors and the file and line an existing application edits by hand.
* `pub const direction::DIRECTIONS: &[Direction]`, the table. `pub fn direction::find(name: &str) -> Result<&'static Direction, BuildError>`, `Direction` on a name outside it.

| Name | Maps | Vendors | Edits |
| --- | --- | --- | --- |
| `react` | `@snapfire/fsr-client/react` and `@snapfire/fsr-authoring/template`, the latter at the client's `template.js`, the runtime of the dialect's placements | `react@18.3.1`, `react@18.3.1/jsx-runtime`, `react-dom@18.3.1/client` | nothing |
| `vue` | `@snapfire/fsr-client/vue` | `vue@3.5.13` | nothing |
| `elements` | `@snapfire/fsr-client/elements` | nothing | nothing |
| `htmx` | `@snapfire/fsr-client/htmx` | `htmx.org@2.0.10` | `src/main.ts`: the htmx import, the `bindHtmx` import and `bindHtmx(htmx)` after `enableNavigation()` |
| `tera` | nothing | nothing | nothing; present only under the `tera` feature |

* `pub const direction::REACT`, `direction::VUE`, `direction::HTMX`: the pinned versions, stated once. The `fsr add` command a build suggests for an unrecorded React names `REACT`.
* `Direction::specifier(&self) -> Option<String>` and `Direction::url(&self) -> Option<String>`: the map key and the URL under `snapfire_fsr_host::client::ROUTE`, `None` for a direction with no entry. `Direction::specs(&self) -> Vec<Spec>`.

### adopt

* `pub fn direction::adopt(app: &Path, names: &[String], options: UseOptions) -> Result<Adopted, BuildError>`
* `pub struct direction::UseOptions { pub fetch: bool, pub example: bool }`, `Default` with `fetch` true.
* Resolves every name first, so an unknown one refuses before anything is written. With `example` it checks that no example file is already there. Then per direction: the map line through `read_import_map` and `write_import_map`, an entry already at the host's URL reported as `present` and one at another URL refused as `AdapterUrl`; the specs against `VendorManifest`, a package recorded at the pinned version reported as `kept` and one at another version refused as `DirectionPinned`; the example files. Then, with `fetch`, `vendor::add` over what is left to vendor, which under a site takes what the shell serves and under xwpm delegates, `types::fetch` and the generation `fsr new` runs, each failure a note rather than a stop; without it, `fsr add`, `fsr types` and `fsr build` as `next`. Last, the direction's edits and the examples' placements.
* `pub struct direction::Adopted { pub mapped: Vec<(String, String)>, pub present: Vec<String>, pub vendored: Vec<(String, String, usize)>, pub kept: Vec<String>, pub from_shell: Vec<(String, String)>, pub delegated: Vec<String>, pub typed: Vec<(String, String, String)>, pub written: Vec<PathBuf>, pub notes: Vec<String>, pub edits: Vec<(String, String)>, pub next: Vec<String> }`.
* The examples, one per direction, each building and typechecking on its own: `react` writes `src/ui/Counter.tsx`, an island with one piece of state; `vue` writes `src/ui/Counter.vue`; `elements` writes `elements/hello-tag.tsx` and `src/elements/hello-tag.ts`, the template and the class `shadowOf` joins; `htmx` writes `routes/pulse/page.loader.ts` and `routes/pulse/page.tsx`, a page a fragment request refreshes; `tera` writes `routes/hello/page.loader.ts` and `routes/hello/page.tera`. Nothing is placed into an existing page.

### fetch

* `pub fn types::fetch(app: &Path, refresh: bool) -> Result<TypesReport, BuildError>`
* The queue is `@snapfire/fsr-authoring`, `@snapfire/fsr-client`, then `import_map_packages`. A package whose directory exists is kept unless `refresh`; the fsr packages are written from declarations embedded in the binary; any other `@snapfire/*` is `missing`. Under xwpm, `xwpm restore` and `xwpm types` run first and every other package is `missing` with that reason. Otherwise, the npm registry: the abbreviated packument chooses the highest release sharing the vendored major, else `latest`; the version document's `types` or `typings` names the entry and its tarball's `.d.ts`, `.d.mts`, `.d.cts` and `package.json` files are unpacked under `<types>/<package>/`; without one, `@types/<name>` (`@scope/name` as `@types/scope__name`) the same way with `index.d.ts` as the entry; its `dependencies` are queued under their package names. A package with neither is `missing`.
* `pub struct TypesReport { pub fetched: Vec<(String, String, String)>, pub kept: Vec<String>, pub missing: Vec<(String, String)>, pub delegated: Vec<String>, pub written: Vec<String> }`: package, version and source; kept packages; package and reason; xwpm commands run; the shim and the tsconfig written, as paths relative to the app.
* `pub const types::FOREIGN_SHIM: &str = "foreign.d.ts"`, the shim's name under `<types>/`.
* `pub fn types::source_dirs(app: &Path) -> Vec<&'static str>`: of `src`, `ext`, `elements`, `routes`, `schemas` and `tests`, the ones the application has.
* `pub fn types::foreign_shim(app: &Path, placed: &[String]) -> Option<String>`: `declare module "*.<ext>"` for every extension of a `.vue` file under the source directories and of every module in `placed`, the foreign components a build's templates import; `None` when there are none. The `vue` declaration types the default export as a function of loose props; any other extension as `unknown`.
* `pub fn types::write_foreign_shim(app: &Path, layout: &Layout, placed: &[String]) -> Result<Option<String>, BuildError>`: writes the shim to `<types>/foreign.d.ts`, creating the directory; answers the path relative to the app; with nothing foreign it removes a shim that is there and answers `None`.
* `types::definitely_typed(package) -> String`, `types::is_ambient(entry: &str) -> bool` (contains `declare module "` or `declare module '`).
* `types::present(app, &layout) -> Result<Vec<(String, TypedPackage)>, BuildError>`: every package directory under the types directory, scoped ones included, with the manifest's record or `index.d.ts` as the entry when it has none.
* `types::status(app) -> Result<Vec<(String, String)>, BuildError>`: the report rows for the fsr packages and every import map package: `<types>/<package>  <from> <version>`, `<types>/<package>` when unrecorded or `missing; run fsr types`.
* `types::NPM_REGISTRY` is `https://registry.npmjs.org`.

### tsconfig

* `pub fn types::tsconfig(app: &Path, generated: bool, shim: bool) -> Result<String, BuildError>`: `target` es2022, `module` esnext, `moduleResolution` bundler, `jsx` react-jsx, `jsxImportSource` `@snapfire/fsr-authoring` when the import map has no `react`, `strict`, `noEmit`, `skipLibCheck`; `paths` with `@snapfire/fsr` to `./generated/fsr`, then per present package `<name>` to `./<types>/<name>/<entry>` unless ambient and `<name>/*` to `./<types>/<name>/*`, then `@snapfire/fsr-authoring/template` to `./<types>/@snapfire/fsr-authoring/template.react` when the import map has `react`, so a file on the dialect's module is typed through React's JSX with `Children` as `ReactNode`; `include` of `<dir>/**/*` for each of `source_dirs`, `generated/**/*` when `generated`, `<types>/foreign.d.ts` when `shim` and each ambient entry. The build passes `generated` as true, since it is writing the directory; `fsr types` passes whether it is there.
* `pub fn types::tsconfig_build(app: &Path, route_files: &[String]) -> String`: `target` es2022, `outDir` dist, `rootDir` `.`, `sourceMap`, `jsx` react-jsx; `include` of `src/**/*`, `ext/**/*` when present, each of `route_files`, `generated/islands.ts` and `generated/client.ts`. A static template is not among `route_files`, so it is never compiled and never asks the import map for a JSX runtime.
* `pub fn types::element_declarations(app: &Path, layout: &Layout, elements: &[(String, String)]) -> Result<String, BuildError>`: `generated/elements.d.ts` for the tags and modules under `elements/`. Each entry is `Placed<typeof TemplateN>`, the template's props over `Host` with any other attribute allowed. `Host` is React's `DetailedHTMLProps<HTMLAttributes<HTMLElement>, HTMLElement>` when the import map has `react`, else the dialect's `Attributes`. With `react`, `Taken` is the union of every template's `keyof Props` and the pattern entry is `Omit<Host, Taken> & Loose`. With no template `Taken` is `never`. A dialect application with no template gets `export {};`.

### Manifests

* `pub struct vendor::VendorManifest { pub packages: BTreeMap<String, VendoredPackage> }`, `vendor::VendoredPackage { pub version: String, pub externals: Vec<String>, pub entries: BTreeMap<String, String> }`, entries from specifier to file relative to the vendor directory; read and written as `<vendor>/.fsr-vendor.json`.
* `pub struct types::TypesManifest { pub packages: BTreeMap<String, TypedPackage> }`, `types::TypedPackage { pub version: String, pub from: String, pub entry: String, pub ambient: bool }`; read and written as `<types>/.fsr-types.json`.
* Both: `read(app, &layout)`, `write(&self, app, &layout)`; a missing file reads as empty.

## 7. Error Handling

### BuildError

* `Io(PathBuf, std::io::Error)`
* `NoRoutes(PathBuf)`, when `app/routes` is not a directory.
* `Segment { path: PathBuf, name: String }`
* `Lower(LowerError)`, transparent; see `snapfire_fsr_lower`.
* `Import { document: String, error: ImportError }`
* `DuplicateType { name: String, first: String, second: String }`
* `Contract(ContractError)`, from `Contract::validate`.
* `UnknownInput { action: String, name: String }`
* `ActionsWithoutPage(PathBuf)`, an `actions.ts` in a directory under `routes/` with no page.
* `SlotsWithoutLayout(PathBuf)`, `SlotWithoutPage(PathBuf)`, `SlotRoute(PathBuf)` and `SlotUndeclared { path: PathBuf, file: String, slot: String }`, from the slot and variant rules above.
* `PageAndTemplate { dir: PathBuf, first: String, second: String }`, `TemplateFeature(PathBuf)` and `TemplateIsland { file: PathBuf, line: usize }`, from the template rules above. The `tera` feature is on by default and forwards to the host's.
* `PathsOffPage(String)`: a `layout.loader.ts` or a slot's `page.loader.ts` exports `paths`, which only a route's page loader may. `PathsWithoutParameter { module, pattern }`: a page loader exports `paths` on a route whose pattern has no parameter.
* `NoAdapter { module: String, ext: String }`, an island whose extension a plugin compiles but no client adapter mounts.
* `UnknownComponent { module: String }`, an island whose extension no framework claims.
* `IslandImports { module: String, adapter: String, missing: String, remedy: String }`, the first registered module whose adapter the import map cannot supply, with the specifiers it lacks. Also one importing a value from `@snapfire/fsr-authoring/template` with that specifier unmapped, since its runtime is the client's `template.js`; `remedy` names the `fsr use` direction that writes the line, empty for an adapter no direction maps.
* `Direction { name: String, known: String }`, an `fsr use` or `--with` name outside the direction table, with the table.
* `AdapterUrl { map: String, specifier: String, found: String, want: String }`, an import map already carrying the adapter's specifier at a URL other than the one the host serves.
* `DirectionPinned { direction: String, package: String, recorded: String, wanted: String, manifest: String }`, a vendor manifest recording a framework package at a version other than the one the direction pins; moving it is `fsr add`.
* `ExampleExists(PathBuf)`, an `--example` file already present.
* `FrameworkUnrecorded { package: String, specifier: String, manifest: String, app: String, version: String }`, an import map serving a package a client adapter imports with no entry for it in the vendor manifest; the message names the `fsr add` command that records it.
* `ReactMajor { version: String, supported: String }`, a vendored React whose major the renderer has no rules for.
* `FrameworkShellUnrecorded { package: String, specifier: String, contract: String, version: String }`, a site whose shell serves a framework package with no `frameworks` entry saying which version; the message names the contract and the entry a hand-written one needs. A shell built before its contract recorded every framework package is refused here until it is rebuilt.
* `FrameworkShellMismatch { package: String, site: String, shell: String, manifest: String, contract: String }`, a site vendoring one version of a framework package while its shell serves another, which the browser would never load.
* `ShellUrl { map: String, specifier: String, found: String, want: String, contract: String }`, a site mapping a framework specifier somewhere other than the URL its shell serves, which the browser would never fetch.
* `ShellPinned { specifier: String, package: String, wanted: String, shell: String, contract: String }`, an `fsr add` pinning a version of a package the site's shell already serves.
* `ElementName { file: String }`, a file under `elements/` whose name is not a custom element tag.
* `ElementTemplate { module: String, reason: String }`, an element template with state or a handler or a root `<template>` that cannot be its shadow root; `reason` says which.
* `Spec(String)`, an `fsr add` argument that is not `name@version[/subpath]`.
* `Http(String, String)`, the URL and the failure.
* `Manifest(PathBuf, String)`, a vendor manifest, types manifest, import map or `xwpm.wmf` that did not parse.
* `Serve(String)`, the stock host refusing to build or the listener failing, from `fsr serve`.
* `Dependency { package: String, wants: String }`, a vendored module importing a package outside its bundle.
* `VendorUrl { map: String, specifier: String, found: String, base: String, want: String }`, an import map entry for a package the vendor manifest records that does not sit under the layout's base, with the URL it should carry. A site's base is its own prefix, so a map written before the `[site]` section is named here.
* `Xwpm(String)`, an `xwpm` command that could not start or failed.
* `Typecheck(String)`, the checker that could not be read or the diagnostics of a check that found an error, the row first.
