# API Reference: snapfire_fsr_cli

The `fsr` binary and the library behind it: the commands, the build, the generated files, vendoring, declarations and typechecking.

## Contents

* [1. Commands](#1-commands)
  * [fsr new](#fsr-new)
  * [fsr use](#fsr-use)
  * [fsr add](#fsr-add)
  * [fsr types](#fsr-types)
  * [fsr migrate](#fsr-migrate)
  * [fsr fonts](#fsr-fonts)
  * [fsr build](#fsr-build)
  * [fsr check](#fsr-check)
  * [fsr dev](#fsr-dev)
  * [DevOptions](#devoptions)
  * [dev::Programs](#devprograms)
  * [fsr run](#fsr-run)
  * [fsr test](#fsr-test)
  * [fsr doctor](#fsr-doctor)
  * [fsr lint](#fsr-lint)
  * [fsr census](#fsr-census)
  * [fsr ir view](#fsr-ir-view)
  * [fsr bundle](#fsr-bundle)
  * [fsr serve](#fsr-serve)
  * [ServeOptions](#serveoptions)
  * [fsr prerender](#fsr-prerender)
  * [fsr sites](#fsr-sites)
* [2. The Build](#2-the-build)
  * [Options](#options)
  * [SiteOptions](#siteoptions)
  * [ShellContract](#shellcontract)
  * [build](#build)
  * [check](#check)
  * [Built](#built)
  * [write](#write)
  * [write_generated](#write_generated)
  * [write_overlay](#write_overlay)
  * [emit](#emit)
  * [Report](#report)
  * [Cause](#cause)
  * [Hoisted](#hoisted)
  * [Plugins](#plugins)
  * [tailwind](#tailwind)
  * [assets](#assets)
  * [intl](#intl)
* [3. Discovered Files](#3-discovered-files)
  * [Clients](#clients)
  * [Schemas](#schemas)
  * [Extensions](#extensions)
  * [Elements](#elements)
  * [Routes](#routes)
  * [Segments and Ids](#segments-and-ids)
  * [Modules](#modules)
  * [Plan Shape](#plan-shape)
  * [Generated Files](#generated-files)
  * [Themes](#themes)
* [4. Terminal Programs](#4-terminal-programs)
  * [terminal](#terminal)
  * [Session](#session)
* [5. Desktop Window](#5-desktop-window)
  * [desktop::run](#desktoprun)
  * [Window](#window)
  * [Menus](#menus)
  * [Bridge](#bridge)
  * [Scheme](#scheme)
* [6. Inference](#6-inference)
  * [Ts](#ts)
  * [Inferer](#inferer)
* [7. Typechecking](#7-typechecking)
  * [Typecheck](#typecheck)
  * [Checked](#checked)
  * [Diagnostic](#diagnostic)
  * [spawn](#spawn)
  * [finish](#finish)
  * [run](#run)
  * [find_checker](#find_checker)
  * [pin](#pin)
  * [record](#record)
* [8. Vendoring and Declarations](#8-vendoring-and-declarations)
  * [Layout](#layout)
  * [Spec](#spec)
  * [vendor::add](#vendoradd)
  * [published](#published)
  * [Direction](#direction)
  * [adopt](#adopt)
  * [types::fetch](#typesfetch)
  * [Declarations](#declarations)
  * [Shims](#shims)
  * [tsconfig](#tsconfig)
  * [types::range](#typesrange)
  * [Manifests](#manifests)
* [9. Error Handling](#9-error-handling)
  * [BuildError](#builderror)
  * [DoctorError](#doctorerror)
  * [IrError](#irerror)

## 1. Commands

Every command parses its arguments with clap. Each takes `--help`. A flag given without its value is an error. A usage error exits 2.

Feature `web` (on by default) adds `fsr prerender`, `fsr bundle`, `fsr census`, `fsr use`, `fsr add`, `fsr types`, `fsr sites` and `fsr fonts`. Feature `test` (on by default) adds `fsr test`. Feature `terminal` adds `fsr run` and `fsr dev --terminal`. Feature `desktop` adds `--desktop` to `fsr dev` and `fsr serve`.

### fsr new

* `fsr new <project dir> [--with <direction>]... [--no-fetch] [--shell] [--site --at <path> [--name <name>] [--into <shell dir>]] [--terminal]`
* `new::create(root: &Path, options: NewOptions) -> Result<Created, BuildError>`.
* Writes `config/app.toml`, `.gitignore` and under `app/`: an empty import map, an entry module, a root layout, an index page and its loader, a not-found page, an error page and a stylesheet.
* The application vendors no framework. Its layout imports `Link` from `@snapfire/fsr/components` and `Children` from `@snapfire/fsr/template`.
* Refuses a directory that already holds `app/` or `config/`.
* `--with <direction>`, repeatable, runs `direction::adopt` over the written project for each name in order. A name outside the direction table is refused before anything is written.
* With `htmx`, the entry module imports htmx and calls `bindHtmx` after `enableNavigation`.
* With `tailwind`, the shell's `styles/app.css` starts with `@import "tailwindcss";`. A site's sheet starts with `tailwindcss/theme` and `tailwindcss/utilities`.
* No other direction changes the template.
* Unless `--no-fetch`, writes the declarations for every import map package into `app/types/` and runs the generation, which writes `app/generated/` and both tsconfigs. The browser bundle is not built.
* `--no-fetch` writes the template and the directions' map lines alone and names `fsr add`, `fsr types` and `fsr build` as the next steps.
* Without `--with`, the `next` lines end with `fsr use <app> react`, marked as wanted only by an application that wants React, with the other directions named.
* `--terminal` writes `.gitignore`, a `config/app.toml` holding `[app] runs = "terminal"` and an Ink root page under `app/routes/`. It writes no import map, entry module or stylesheet.
* `--terminal` is refused with `--with`, `--shell` or `--site`. Unless `--no-fetch` it writes the declarations. Its `next` lines end with `fsr dev <app>`.
* `--shell` gives the configuration a `[sites]` table.
* `--site` gives it a `[site]` section and needs `--at`. `--into` writes both halves of the mount through `sites::link` instead.
* `--shell` and `--site` are refused together.
* A site's stylesheet is `styles/<name>.css` with every rule under `.<name>` and none on `body` or `:root`.
* A site's layout, error and not-found pages link to `--at` rather than `/`.
* A site listens on `3001` rather than `3000` and carries no `[cache]`.
* Prints `wrote <path>` per file, `added <specifier> <file> <bytes>` per vendored module, `types <package> <from> <version>` per declaration set and `next <command>` for what to run.
* Prints `note <text>` on stderr for a step that failed without stopping the scaffold.
* Exit 0 on success, 1 on a `BuildError`.

### fsr use

* `fsr use <app dir> <direction[@version]>... [--no-fetch] [--example]`
* `direction::adopt` over the names, in order.
* The directions are `react`, `vue`, `svelte`, `elements`, `htmx` and `tailwind`, plus `tera` and `mdx` in an fsr built with that feature.
* `<direction>@<version>` vendors that version of the direction's packages in place of the pinned one. A version that does not parse is refused as a `Spec`.
* `svelte` vendors its runtime as published, its module tree kept. Its compiler is never vendored.
* A build fetches the recorded Svelte version's `compiler/index.js` into `.fsr/compilers/svelte/<version>/` through `published::compiler`, checked against the registry's integrity. `snapfirec-svelte` compiles with that file.
* `tailwind` maps and vendors nothing. It writes `styles/tailwind.css` importing `tailwindcss/theme` and `tailwindcss/utilities` without the preflight.
* `tailwind` writes nothing when a stylesheet under the styles directory already imports Tailwind. It prints `present tailwind in <sheet>` instead.
* An application whose `[document] styles` is written rather than inferred is given the edit that lists the new sheet.
* Running it again changes nothing.
* An application whose `[[static]]` root answers `/static/js/fsr` gets the adapter's import map line written into its map. Any other gets the adapter named by the host when the bundle imports it.
* A line already in the map that points elsewhere is refused.
* Prints `mapped <specifier> <url>` per import map line written and `present <specifier>` per line already there.
* Prints `added <specifier> <file> <bytes>` per vendored module, `shell <specifier> <url>` per package taken from the site's shell and `kept <specifier>` per package already recorded at the pinned version.
* Prints `xwpm add <spec>` per delegated call, `types <package> <from> <version>` per declaration set and `wrote <path>` per example and generated file.
* Prints `edit <file>: <line>` for what the application changes by hand and `next <command>` for what to run.
* Prints `note <text>` on stderr for a step that failed without stopping the command.
* Exit 0 on success, 1 on a `BuildError`.
* `--example` writes one component per direction and prints where to place it. For `tailwind` it writes `routes/styled/page.tsx` and the theme `styles/themes/ocean.css`. A file already there is refused before anything is written.
* `--no-fetch` writes the map lines and the examples alone and names `fsr add`, `fsr types` and `fsr build` as the next steps.

### fsr add

* `fsr add <app dir> [<name@version[/subpath]>...] [--external <name,...>]... [--published] [--host <url>]...`
* `add::add(app: &Path, specs: Vec<Spec>, externals: &[String], published: bool, hosts: &[String]) -> Result<Added, BuildError>`.
* `--published` conflicts with `--external` and `--host`.
* With no spec, repairs the vendored modules' source maps alone.
* A package `vendor/.fsr-vendor.json` records as published takes the new version as published. `published::recorded` widens the specs to every specifier the record names. `fsr add app svelte@5.60.0` updates the whole runtime.
* `--published` vendors a package as published the first time, through `published::add_published`.
* Otherwise runs `vendor::add` over the specs.
* Prints `added <specifier> <file> <bytes>` per entry, `remapped <specifier> <url>` per entry moved to the layout's base and `xwpm add <spec>` per delegated call.
* Exit 0 on success, 1 on a `BuildError`. A spec without a version exits 2.
* Each module's source map is vendored beside it under the name its `sourceMappingURL` comment gives. A module whose map esm.sh does not have loses the comment.
* `vendor::hosts(app: &Path, given: &[String]) -> Vec<String>`: each `--host` in order, else the configuration's `[vendor] hosts`, else esm.sh. Each host answers the esm.sh API.
* A host that fails hands the package to the next one. The modules and maps behind a stub come from the host that answered it.
* A host that answers 404 ends the search.
* When every host fails, the command fails naming each one's error.

### fsr types

* `fsr types <app dir> [--refresh]`
* Runs `types::fetch`.
* A package whose entry is on disk is kept unless `--refresh` is given or a range asked of it refuses the version it holds. The entry is the one its manifest row records, else `index.d.ts`.
* Prints `ran <command>` per delegated xwpm command, `types <package> <from> <version>` per fetch, `kept <package>` and `missing <package> <why>`.
* Prints `unmet <package> <version>; <by> asks <range>` per range no published version met.
* Prints `wrote <path>` for `<types>/foreign.d.ts` when a source is foreign and for `tsconfig.json`.
* Exit 0, also with missing packages. Exit 1 on a `BuildError`.

### fsr migrate

* `fsr migrate <app> <project dir> [--no-fetch] [--map <file>]`: an application written for Next.js, SvelteKit, Nuxt or Ink, as a new FSR project under `<project dir>`.
* `migrate::run(from: &Path, to: &Path, fetch: bool, map: Option<&Path>) -> Result<Migrated, BuildError>`.
* `--map` reads that migration map in place of the application's own `migrate.toml`.
* `fsr migrate <project> [--dry-run]`: an FSR project on the earlier layout, rewritten in place.
* `migrate::relayout(project: &Path, dry_run: bool) -> Result<Vec<Relaid>, BuildError>`.
* Every file under `<project>/app` that names `@snapfire/fsr-client`, `@snapfire/fsr-authoring` or `Ctx` from `@snapfire/fsr` is moved onto `@snapfire/fsr/client`, `/host`, `/terminal`, `/template` and `/components`.
* A file the browser runs takes the client side. A body takes the host side.
* The import map's entries are renamed.
* Prints `wrote <path>` (`would <path>` with `--dry-run`), then each changed line as `- old` and `+ new`, then `next fsr check <app>`. Prints `nothing` when no file names an old specifier.
* `pub struct Relaid { pub path, pub lines }`: a file's changed lines as `(old, new)` pairs, one side empty.
* `fsr migrate --schema` prints the JSON Schema of `migrate.toml`. It conflicts with every other argument.

### fsr fonts

* `fsr fonts add <app dir> <spec>`, `spec` as `google:<Family>@<weights>`, a weight ending in `i` for italic: `google:Inter@400,700,400i`.
* `assets::add(app: &Path, spec: &str) -> Result<Vec<PathBuf>, BuildError>`.
* Fetches the provider's stylesheet with a current browser's user agent.
* Downloads every face it names into the directory `[dirs] fonts` names, as `<Family>-<weight>[-italic]-<subset>.woff2`.
* Writes `<file>.range` beside each face with its `unicode-range`.
* Writes `<file>.family` with the stylesheet's `font-family` when the file's own name table names a different family.
* A variable font is kept once per subset as `<Family>[-italic]-<subset>.woff2`, with no weight in its name.
* `assets::add_from(app: &Path, spec: &str, provider_base: &str) -> Result<Vec<PathBuf>, BuildError>`: the same against a provider at another base.
* Prints `fetched <path>` per file and a `fonts` line with the count.
* A family the provider does not serve is refused by name.

### fsr build

* `fsr build <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>] [--strict] [--verbose] [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]`
* Runs the `[build] before` programs, then `emit`, then the `[build] after` programs, each through `dev::run_programs`.
* Prints the report to stdout.
* Writes `generated/plan.sexp`, `generated/locale.data`, `generated/contracts/<client>.json` per document, `generated/contracts/schemas.json`, `generated/uploads.d.ts`, `generated/native.d.ts`, `generated/services.d.ts`, `generated/elements.d.ts`, `generated/host.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json` and `tsconfig.build.json`, each under `<app dir>`.
* Then bundles the browser modules into `<app dir>/dist/` with `snapfirec`.
* `--public-path` defaults to `/static/js/app`. For a site it defaults to `<at>/static/js/app`.
* `--snapfirec` defaults to `$SNAPFIREC`, else `snapfirec` beside this binary, else `PATH`.
* `--strict` sets `Options::strict` for the run, as `[build] strict` does.
* `--verbose` prints `Report::verbose`, a `wrote` line per file, the `typecheck` row, the `derived` line, each `before` and `after` row and snapfirec's progress lines.
* Without `--verbose`, the report is `Report`'s `Display`. The `typecheck` row prints only beside a diagnostic. snapfirec's lines print only when the bundle fails.
* The typecheck prints its diagnostics and one `typecheck <row>` line. It prints a `recorded` line when it wrote the version into the configuration.
* With no checker installed, the typecheck prints only a note on stderr.
* Exit 0 on success, 1 on any `BuildError`, 2 on a usage error.

### fsr check

* `fsr check <app dir> [--shell <module id>] [--slot <name>] [--strict] [--verbose] [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]`
* Runs `check` and prints the report.
* Writes the build's `generated/` files, its `tsconfig.json` and its `types/foreign.d.ts` and nothing else.
* Exit codes as `fsr build`, plus 1 when a diagnostic is an error.

### fsr dev

* `fsr dev <app dir> [<fsr build flags>] [--under <shell>] [--alone] [--desktop] [--terminal] [-- <arg>...]`
* `dev::run(app: &Path, options: DevOptions) -> Result<(), BuildError>`: the loop. Runs until Ctrl-C or until its window closes under `--desktop`.
* Builds the shell and every site its `[sites]` table mounts from a path.
* Each application gets its own `snapfirec --driven --asset-map` child.
* Runs `cargo build --message-format=json-render-diagnostics` when a `Cargo.toml` is beside the shell, then spawns the one binary the build reported.
* Watches what the project's build scripts declared with `rerun-if-changed`.
* With no `Cargo.toml` beside the app, runs `fsr serve` in place of the crate and watches `config/`, `app.toml` and `app.yaml` beside the app instead of `src/`.
* A change to the generated files is `POST /__fsr/reload` to `server.listen`. The loop prints the report the host answers with.
* The process restarts only when the reload is refused, as for a changed `[session]`.
* After a rebundle alone the loop posts `/__fsr/changed`, best effort. Open development documents refresh on it.
* A reload waits up to ten seconds for a server that is still starting to listen.
* Changed paths are classified per application by prefix. A path under no application belongs to the project.
* Ignored under any application: `generated/`, `dist/`, `types/`, `.fsr/`, `tests/`, both tsconfigs, `*.test.ts` and `.fsr-*`.
* A file whose content hashes as it did when the loop last looked is not a change.
* Three rebuilds in a row that nothing but the loop's own writes caused stop the loop until an edit arrives.
* `--verbose` prints each rebuild's report, the host's reload report, `dev: changed <paths>` and the loop's `server started` and `stopping` lines.
* Without `--verbose`, a failed batch prints `snapfirec failed:` followed by what snapfirec wrote to stderr for that batch. A batch that succeeds prints any warnings it wrote there once it is done.
* On a platform without `poll(2)`, snapfirec's stderr reaches the terminal directly.
* On a site, serves it under a shell. `--under` names the shell's project directory, an unpacked artifact or an archive.
* Without `--under`, the shell `[site] shell` points into is used when that tree has a configuration.
* `--alone` serves the site by itself.
* The shell is served as it stands through `fsr serve <shell app> --mount <name>=<site root>`. Its table and files are not written.
* `--under` on an application that is not a site is refused.
* `--desktop`, with the `desktop` feature: runs the loop on a thread beside a native window opened on the shell's `server.listen` once it accepts a connection. The development server stays on loopback.
* Closing the window stops the loop and the server it started. A `server.listen` on port 0 is refused.
* `--terminal` with the `terminal` feature runs `terminal::run` in place of the loop. So does an app `is_terminal_app` answers true for.
* The arguments after `--` reach the page's loader as `params`.
* Without the `terminal` feature, a terminal app is a `BuildError::Dev` naming it.
* `dev::OWNS_BUILD`: `"FSR_DEV_OWNS_BUILD"`, set in the environment of the cargo commands `dev::run` spawns.
* `dev::owns_build() -> bool`: whether `OWNS_BUILD` is set and not empty.
* `dev::BUNDLE_OVERLAY`: `".fsr/bundle"`.
* `dev::DECLARE`: `"fsr:watch="`, the prefix of a line a `[build] before` or `after` program prints to declare a path it reads.
* `dev::print_report(report: &Report, verbose: bool)`: prints the report as `fsr build` does.

### DevOptions

* `pub struct DevOptions { pub build: Options, pub public_path: String, pub snapfirec: Option<PathBuf>, pub typecheck: Typecheck, pub under: Option<PathBuf>, pub alone: bool, pub verbose: bool, pub desktop: bool, pub terminal: bool, pub args: Vec<String> }`, `#[non_exhaustive]`.
* `desktop` exists with the `desktop` feature. `terminal` and `args` exist with the `terminal` feature.
* `Default`: `Options::default()`, `public_path` `BUNDLE_BASE`, typecheck enabled, every flag off.
* `DevOptions::beside(app: &Path) -> DevOptions`: `Options::beside(app)`, `public_path` from `bundle_base` and `Typecheck::beside(app)`.

### dev::Programs

* `pub struct dev::Programs { pub root: PathBuf, pub app: PathBuf, pub before: Vec<PathBuf>, pub after: Vec<PathBuf>, pub verbose: bool }`, `#[non_exhaustive]`: one application's `[build] before` and `after` projects, resolved against its project root. `root` and `app` are absolute.
* `verbose` prints each program's `before` or `after` row before it runs.
* `dev::programs_of(app: &Path) -> Programs`: read from the configuration beside `app`. Both lists are empty without a `[build]` section.
* `dev::run_programs(key: &str, projects: &[PathBuf], programs: &Programs, declared: &mut BTreeMap<PathBuf, Vec<PathBuf>>) -> Result<(), BuildError>`: runs each project in order with `run_program` and stops at the first failure. Each success replaces its entry in `declared`.
* `dev::run_program(root: &Path, key: &str, project: &Path, app: &Path) -> Result<Vec<PathBuf>, BuildError>`: `cargo build --message-format=json-render-diagnostics` in `project` with `OWNS_BUILD` set, then its one binary run from `root` with `FSR_ROOT` and `FSR_APP` set.
* `run_program` returns the paths its `DECLARE` lines named, resolved against `root`. It prints every other line.
* A missing `Cargo.toml`, a failed build, a build that made other than one binary and a non-zero exit are each `BuildError::Dev` naming `[build] <key> <project>`.

### fsr run

* `fsr run <app dir> [-- <arg>...]`, with the `terminal` feature.
* `terminal::run_once(app: &Path, args: &[String]) -> Result<(), BuildError>`: runs the terminal program once with no watcher.
* An application whose configuration lacks `runs = "terminal"` under `[app]` is refused, naming `fsr serve`.
* A page the session refuses, at open or on its first frame, ends the run with the reason before the screen is entered.
* `--name value` and positional arguments after `--` reach the page's loader as `params`.
* The program's exit code is the process's.

### fsr test

* `fsr test <app dir> [filter] [--specs-only | --rust-only] [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]`
* `test::run(app: &Path, options: &Options, typecheck: &Typecheck, filter: Option<&str>) -> Result<Summary, BuildError>`: builds, then typechecks the application with its tests when `typecheck.enabled`.
* Then runs every `*.test.ts` body test through the interpreter and every `*.spec.ts` and `*.spec.tsx` page spec in QuickJS over linkedom.
* A run formats from every locale's data: it calls `IntlData::uninstall` first and the host a spec loads a route through is built with `HostBuilder::every_locale`.
* Before the build it fetches the declarations the application lacks, as `emit` does. One still missing is `BuildError::Types` naming it.
* A type error is `BuildError::Typecheck` with the checker's diagnostics. Nothing runs then.
* `filter` keeps the tests whose name or file holds it, specs and Rust tests alike.
* The binary passes `Typecheck::beside` with the flags applied. `Typecheck::default()` skips the typecheck.
* `test::rust_project(app: &Path) -> Option<PathBuf>`: the directory above `app` when it holds a `Cargo.toml`.
* `test::run_rust(project: &Path, filter: Option<&str>) -> Result<RustRun, BuildError>`: runs that crate's tests with `cargo nextest run --no-tests=warn` when `cargo nextest --version` succeeds, else `cargo test`. `filter` is the test-name filter.
* The runner writes to this process's output.
* `pub struct RustRun { pub runner: &'static str, pub passed: bool }`.
* The binary runs `run_rust` after `test::run` unless `--specs-only`. `--rust-only` runs it alone.
* `pub struct Summary { pub passed: usize, pub failed: usize, pub skipped: usize, pub todo: usize, pub lines: Vec<String> }`, `#[non_exhaustive]`.
* `Summary::record(&mut self, file: &str, name: &str, outcome: Outcome)`. `pub enum Outcome { Passed, Failed(String), Skipped, Todo }`.
* `Display` prints one line per test: its file, its name and `ok`, `FAILED` with the message beneath, `skipped` or `todo`. Then `test result:` with the counts.
* Exit 0 when nothing failed. Exit 1 when a test failed or the build or the typecheck stopped the run.

### fsr doctor

* `fsr doctor <app dir> [--verbose] [--env <release env>] [--app-env <name>] [--region <name>] [--runtime]`
* `--env`, `--app-env` and `--region` stand in for `RELEASE_ENV`, `APP_ENV` and `APP_REGION`. A variable a flag leaves out is read from the environment.
* `doctor::run(app: &Path) -> Result<Report, DoctorError>`: reads the configuration and the plan the way the host would and runs every check. Needs nothing running.
* `doctor::run_with(app: &Path, options: &DoctorOptions) -> Result<Report, DoctorError>`: `run` over the ladder `options.deployment` names. `run` is `run_with` with the defaults.
* Exit 0 when clean, 1 when anything was found, 2 when the configuration or the plan could not be read.
* `pub struct DoctorOptions { pub deployment: Option<Deployment>, pub runtime: bool }`, `#[non_exhaustive]`, `Default`. Setters `deployment(self, Deployment) -> Self` and `runtime(self, bool) -> Self`.
* `runtime` runs in the deployment's own environment. A value the files leave to the environment is then a finding when it is missing.
* `pub struct doctor::Report { pub findings: Vec<Finding>, pub clean: Vec<&'static str>, pub expected: Vec<String> }`, `#[non_exhaustive]`. `Report::is_clean(&self) -> bool`.
* `expected` holds the values the files leave to the deployment's environment when `runtime` is off. They print as `expects` lines and are never findings.
* `Display` prints each finding as two lines, the fact then the remedy, followed by a count.
* `Report::verbose(&self) -> Verbose<'_>` also prints `<check> nothing to report` for each check in `clean`. This is `--verbose`.
* `pub struct Finding { pub check: &'static str, pub what: String, pub remedy: String }`, `#[non_exhaustive]`.
* `doctor::streaming_routes(manifest: &Manifest) -> Vec<String>`: the patterns of the routes a loading boundary streams.
* Reports only. A condition the host refuses to start over stays a boot error.
* `fsr bundle` runs it before it writes.

The checks, by `Finding::check`:

| Check | Finds |
| --- | --- |
| `canonical` | `[document] origin` unset while the deployment names hosts or prerenders |
| `ctx.host` | a body reading `ctx.host` against an empty `[server] hosts` |
| `ctx.config` | a body reading a `ctx.config` key `[public]` does not declare |
| `locales` | a supported locale with no catalog under `locales/` |
| `stale` | a plan missing or older than `routes/`, `src/`, `clients/` or `schemas/` |
| `vendor` | an import map naming a package with nothing under `vendor/` |
| `statics` | a `[[static]]` root with no directory |
| `shadow` | a static route swallowing a route pattern |
| `bearer` | a client carrying a token with no `[auth]` provider to write one |
| `cache.tags` | a tag dropped by a call and cached by none |
| `links` | a literal `href` matching no route, `[[route]]`, static root prefix, file under the root at `/` or mounted site; skipped for a site |
| `render` | a `[server] render` line |
| `tree` | a required file a deploy tree would carry that the project lacks; a configuration `snapfire_fsr_sites::layout` refuses |
| `images` | an AVIF original carrying EXIF or XMP while `[images] strip` is on |
| `desktop` | `[desktop] serve = "scheme"` with a route a loading boundary streams |
| `runtime` | with `runtime` on, a `Config::runtime_values` entry that is a placeholder or missing |
| `sections` | a top-level section in `Config::ignored` one edit from one of `host_sections()` |
| `sites` | a `name@version` mount pinning no hash, an artifact missing a part or its plan, a site plan older than its routes, an artifact under the root no mount names |
| `csp` | a policy in the configuration that breaks the page outright |

### fsr lint

* `fsr lint <app dir> [--json | --sarif] [--deny-warnings] [--fix] [--rules]`
* `lint::run(app: &Path) -> Result<snapfire_lint::Report, snapfire_lint::ConfigError>`.
* The configuration is `Config::discover(serve::project_root(app))` over `snapfire_fsr_lint::base_for(react)`. `react` is true when the app vendors `react`, its import map names it or it is a terminal program.
* The `Layout::of(app)` vendor and types directories are added as ignores.
* The linter is `Linter::new_with(config, checks)` with `Plugins` as its compilers, run with `Linter::lint_dir(app)`.
* `checks` is `snapfire_fsr_lint::checks()` plus an `ExtraneousDependencies` over the packages the app vendored only as a dependency, less `@snapfire/` and what the shell contract's `imports` serve.
* A package vendored only as a dependency is a `.fsr-vendor.json` package with a non-empty `dependency_of`. Under xwpm it is an import map key no `Layout::modules` entry covers.
* `--fix` runs `lint::fix(app: &Path) -> Result<(Report, usize), ConfigError>`. Each file `Linter::fix_dir` changes is written. The count goes to stderr as `fixed    N findings` and the report is a fresh `lint_dir`.
* `--json` prints `Report::to_json`, `--sarif` prints `Report::to_sarif`, else `Report::render`.
* `--rules` prints each of `lint::rules() -> Vec<RuleInfo>` as `default category id` and exits 0 without linting.
* Exit 0 when `Report::failed(deny_warnings)` is false, 1 when it is true, 2 on a `ConfigError`.

### fsr census

* `fsr census <app dir>...`
* Builds each application in full and records every residue in a loader, an action, a handler, middleware or an extension.
* Prints each cause with its count and up to three places, ranked within three kinds: `refused`, `client` and `foreign`.
* Writes nothing. Exit 0 whatever it found.
* `census::run(apps: &[PathBuf]) -> Census`.
* `pub struct Census { pub apps: usize, pub refused: BTreeMap<String, Vec<Sighting>>, pub client: BTreeMap<String, Vec<Sighting>>, pub foreign: BTreeMap<String, Vec<Sighting>>, pub failed: Vec<(String, String)> }`, `#[non_exhaustive]`. Each map is keyed by the cause.
* `refused` is residue `fsr build` stops on. `client` is a component left to render in the browser. `foreign` is a component another framework renders.
* `failed` names an application whose build failed for something other than residue, with the error.
* `pub struct Sighting { pub app: String, pub at: String, pub message: String }`, `#[non_exhaustive]`. `at` is `file:line:column`. It is the file alone when the file did not parse.
* `Display` writes the table.
* `census::cause_of(message: &str) -> String`: the cause a message counts under, each backticked span written `` `…` ``.

### fsr ir view

* `fsr ir view <app dir> <id> [section]`
* Prints one row of `generated/plan.sexp` as the file holds it: a source, action or handler by id, a component by module id or a derive by key.
* `section` is one of `body`, `meta`, `store`, `defaults` and `paths`. It prints that export of a source row alone.
* Reads the plan the last build wrote and builds nothing.
* Exit 0 when printed. Exit 1 when there is no plan, the plan does not read, the id names no row, the section is not one of the five, the row does not carry it or the row has one body and a section was given.
* An id that names no row lists every id.
* `ir::view(app: &Path, id: &str, section: Option<&str>) -> Result<String, IrError>`: the row's text, ending in a newline.

### fsr bundle

* `fsr bundle <app dir> [--out <dir>] [--no-doctor] [--verbose]`
* Prints the file count, the size and what to place beside the tree.
* `--verbose` lists each `serve/` directory with the route it serves and each path the host reads first.
* `bundle::run(app: &Path, out: &Path) -> Result<Bundled, BuildError>`: writes the deploy tree. `--out` defaults to `dist/` beside the project.
* The layout is `snapfire_fsr_sites::layout`: configuration under `config/`, everything the host reads at boot under `app/`, each `.tera` template at its path there and every static root under `serve/<route>/`. A generated `config/bundle.toml` names the paths that moved.
* Nothing is written outside `out`.
* Runs `doctor::run` over the project before writing anything. A finding is `BuildError::Doctor(Report)` and nothing is written.
* `bundle::run_checked(app: &Path, out: &Path, check: bool) -> Result<Bundled, BuildError>`: the same with the doctor check optional. `--no-doctor` passes `false`.
* `pub struct Bundled { pub out: PathBuf, pub served: Vec<(String, String)>, pub read: Vec<String>, pub files: usize, pub bytes: u64, pub beside: Vec<&'static str> }`, `#[non_exhaustive]`.
* `served` is each route with the directory under `serve/` that answers it. `read` is everything in the tree the host reads, tree-relative, in layout order. `beside` is what a deployment places beside the tree.
* `bundle::SERVE`: `"serve"`.
* `bundle::place_server(app: &Path, out: &Path) -> Result<Option<PathBuf>, BuildError>`: copies the `fsr-server` binary into the tree when the project beside `app` has no `Cargo.toml`. Answers where it went. `None` for a project with a crate.
* `fsr bundle` calls `place_server` after `run_checked` and prints `<tree>/fsr-server runs it`.
* `server::binary() -> Result<PathBuf, BuildError>`: `$FSR_SERVER`, else `fsr-server` from `snapfire_fsr_sites` at its `VERSION`.
* `server::binary` builds the server with `cargo install` into `<cache>/snapfire/server/<version>/<arch>-<os>/bin/` once, in release mode, stripped and without debug information. Later calls reuse it. `<cache>` is `$XDG_CACHE_HOME`, else `~/.cache`.
* An `fsr` built from a checkout builds the server from that checkout and keys the cache by its own executable as well.
* `server::has_crate(root: &Path) -> bool`. `server::SERVER_BIN`: `"fsr-server"`. `server::SERVER_ENV`: `"FSR_SERVER"`.

### fsr serve

* `fsr serve <app dir> [--listen <addr>] [--mount <name>=<path>]... [--verbose] [--desktop]`
* `serve::run(app: &Path, options: ServeOptions) -> Result<(), BuildError>`: the stock host, `snapfire_fsr_host::Host`, over the configuration `serve::project_root` finds. Listens on `--listen` or the configured `server.listen` until the process ends.
* `serve::project_root` is the directory beside the app when it holds `config/`, `app.toml` or `app.yaml`, else the app itself.
* Refuses a configuration whose `[app] dir` is not the app given.
* Prints `HostReport::brief`, then the `fsr server on` line with the bound address. `--verbose` prints the whole `HostReport` instead.
* Exit 1 on a `BuildError::Serve`.
* Mounts every site the shell's `[sites]` table names with `snapfire_fsr_sites::mount_all`, sets a reloader that mounts them again and watches the table with `snapfire_fsr_sites::watch` on `SIGHUP` and on `sites.poll`.
* `--mount` lays a site from a path over the `[sites]` table for the run, replacing the row of that name or adding one. Nothing is written.
* `serve::builder_with_mounts(root: &Path, mounts: &[(String, PathBuf)]) -> Result<HostBuilder, BuildError>`: the builder behind `--mount`, carrying a reloader that lays the rows over again.
* When `config.dev()`, installs the trace collector (`snapfire_fsr_host::trace::install`). `GET /__fsr/traces` then answers with what the last requests did. The banner's second line names the endpoint.
* `--desktop`, with the `desktop` feature: serves on a thread beside a native window and returns when the window closes.
* `[desktop] serve = "scheme"` under `--desktop` serves the window over `desktop::scheme` instead of a listener. `--listen` beside the scheme is a `BuildError::Serve`.
* `serve::host_for(app: &Path) -> Result<Host, BuildError>`: the same host without the listener or a collector.

### ServeOptions

* `pub struct ServeOptions { pub listen: Option<String>, pub mounts: Vec<(String, PathBuf)>, pub verbose: bool, pub desktop: bool }`, `#[non_exhaustive]`, `Default`. `desktop` exists with the `desktop` feature.
* Setters `listen(self, Option<String>)`, `mounts(self, Vec<(String, PathBuf)>)`, `verbose(self, bool)` and `desktop(self, bool)`, each returning `Self`.
* `ServeOptions::address(&self, configured: &str) -> String`: `listen`, else `desktop::LOOPBACK` under `desktop`, else `configured`.

### fsr prerender

* `fsr prerender <app dir> [--out <dir>]`
* `serve::prerender(app: &Path, out: Option<&Path>) -> Result<Vec<(String, PathBuf)>, BuildError>`: builds the host with `host_for` and calls `Host::prerender`.
* Writes into `--out`, else `server.prerender`, else `dist/prerender` under the app. One rendering per configured locale. Writes `prerender.build` beside the documents.
* Prints what it wrote. Prints that nothing qualifies when no route does.

### fsr sites

* Every command reads the `[site]` section of the configuration beside the app (`site_beside`) and builds with it. `fsr dev` serves a site's bundle under `<at>/static/js/app`.
* `fsr sites list <shell dir> [--host <url>]... [--header "K: V"]...`: with no host, `sites::list(shell: &Path) -> Result<Vec<Row>, BuildError>`, printed as `site` rows, then `cached` rows from `sites::cached(shell: &Path) -> Result<Vec<(String, Vec<String>)>, BuildError>`.
* `pub struct sites::Row { pub name: String, pub artifact: String, pub resolved: Option<PathBuf>, pub at: Option<String>, pub version: String, pub hash: String, pub note: Option<String> }`.
* With hosts, `sites::compare(shell: &Path, hosts: &[String], headers: &[(String, String)]) -> Result<Vec<Compared>, BuildError>`: every row beside what each instance answers on `GET /__fsr/sites`, marked `ok`, `lags` or `absent`. Exit 1 when any instance disagrees.
* `pub struct Compared { pub row: Row, pub against: Vec<(String, Option<Mounted>)> }`, with `Compared::agrees()`.
* `pub struct Mounted { pub name: String, pub at: String, pub version: String, pub hash: String }`.
* `fsr sites reload [<shell dir>] [--host <url>]... [--header "K: V"]... [--all]`: `sites::reload(hosts: &[String], headers: &[(String, String)], all: bool) -> Result<Vec<Reloaded>, BuildError>`, `POST /__fsr/sites/reload` on each instance in turn.
* `reload` stops at the first refusal unless `--all`. Exit 1 when any refused.
* `pub struct Reloaded { pub host: String, pub sites: Option<Vec<Mounted>>, pub refused: Option<String> }`, with `Reloaded::ok()`.
* `sites::hosts_for(shell: Option<&Path>, given: &[String]) -> Result<Vec<String>, BuildError>`: every `--host`, else the shell's `server.listen`.
* `sites::header(raw: &str) -> Result<(String, String), BuildError>`: parses one `Name: Value` header and refuses anything else.
* `$FSR_SITES_HEADER` joins the given headers. Both commands forward credentials and create none.
* `sites::mounted(host: &str, headers: &[(String, String)]) -> Result<Instance, BuildError>`: one instance. `pub struct Instance { pub host: String, pub sites: Vec<Mounted> }`.
* A `404` from either route is an error saying the host was built without the `sites_reload` feature or installed no sites mounter.
* `fsr sites hash <site dir> [--files]`: `sites::hash(site: &Path) -> Result<Hashed, BuildError>`. Prints the site, the hash and what it ships. `--files` lists every file the hash covers.
* `pub struct Hashed { pub name: String, pub at: String, pub hash: String, pub parts: Vec<String>, pub files: Vec<snapfire_fsr_sites::Entry>, pub bytes: u64 }`.
* `fsr sites pack <site dir> --version <version> [-o, --out <path>]`: `sites::pack(site: &Path, version: &str, out: Option<&Path>) -> Result<Packed, BuildError>`. `--out` is the archive or a directory to write `<name>-<version>.tar.gz` into. A missing `--version` is refused.
* `pub struct Packed { pub manifest: snapfire_fsr_sites::Manifest, pub out: PathBuf, pub bytes: u64, pub unpacked: u64 }`.
* `fsr sites install <shell dir> <archive> [--as <name>] [--keep <n>] [--no-pin] [--layer <layer>]`: `sites::install(shell: &Path, archive: &Path, name: Option<&str>, keep: Option<usize>, pin_it: bool, layer: Option<&str>) -> Result<Installation, BuildError>`.
* `install` unpacks into the cache, then writes `hash` into the mount that names that exact version.
* `--layer` writes the pin into `config/<layer>.toml`, created when absent, instead of `app.toml`.
* `pub struct Installation { pub installed: snapfire_fsr_sites::Installed, pub pinned: Option<Pinned> }`. `pinned` is `None` when nothing is mounted at that version yet or `--no-pin` was given.
* `fsr sites pin <shell dir> [<name>] [--layer <layer>]`: `sites::pin(shell: &Path, only: Option<&str>, layer: Option<&str>) -> Result<Vec<Pinned>, BuildError>`. Hashes what each mount resolves to and writes `hash` into `[sites.<name>]`, replacing the line when it is there.
* Only a `name@version` artifact is pinned. A path mount is skipped.
* `pin` errors on a name the table does not mount.
* `pub struct Pinned { pub name: String, pub hash: String, pub was: Option<String>, pub artifact: PathBuf, pub file: PathBuf }`, with `Pinned::moved()` for whether the file changed.
* `fsr sites link <shell dir> <site dir> --at <path> [--name <name>]`: `sites::link(shell: &Path, site: &Path, at: &str, name: Option<&str>) -> Result<Linked, BuildError>`. Writes the site's `[site]` and the shell's `[sites.<name>]`. A missing `--at` is refused.
* `pub struct Linked { pub name: String, pub at: String, pub shell_config: PathBuf, pub site_config: PathBuf, pub artifact: String, pub shell_json: String, pub site_kept: bool, pub next: Vec<String> }`.
* `fsr sites unlink <shell dir> <name> [--keep-site]`: `sites::unlink(shell: &Path, name: &str, keep_site: bool) -> Result<Unlinked, BuildError>`. Removes the mount and, unless `--keep-site`, the site's `[site]`.
* `pub struct Unlinked { pub name: String, pub shell_config: PathBuf, pub site_config: Option<PathBuf> }`.
* `sites::name_from(raw: &str) -> Result<String, BuildError>` and `sites::check(name: &str, at: &str) -> Result<(), BuildError>`.

## 2. The Build

Crate-root constants: `PLAN_FILE = "generated/plan.sexp"`, `CONTRACTS_DIR = "generated/contracts"`, `RUST_CONTRACT = "rust.json"` (every `#[service]` block's contract, under `CONTRACTS_DIR`), `FSR_DIR = ".fsr"`, `ROOT_PAGE = "routes/page.tsx"`, `ELEMENTS_DIR = "elements"`, `BUNDLE_BASE = "/static/js/app"` and `VENDOR_BASE = "/static/js/vendor"`.

### Options

* `pub struct Options { pub site: Option<SiteOptions>, pub shell: String, pub slot: String, pub strict: bool }`, `#[non_exhaustive]`.
* Setters `site(self, Option<SiteOptions>)`, `shell(self, String)`, `slot(self, String)` and `strict(self, bool)`, each returning `Self`.
* `Default`: no site, `shell#document`, `content`, not strict.
* `Options::beside(app: &Path) -> Options`: the defaults with `site` from `[site]` and `strict` from `[build] strict` in the configuration beside `app`.
* `Options::prefix(&self) -> String`: the prefix on every emitted id, empty without a site.
* `strict` refuses the build with `BuildError::Strict` when the report holds a `client` cause, a `foreign` cause or a `plugins` row.

### SiteOptions

* `pub struct SiteOptions { pub name: String, pub at: String, pub shell: Option<PathBuf> }`, `#[non_exhaustive]`: the `[site]` section as the build reads it.
* `SiteOptions::new(name: String, at: String) -> SiteOptions`, with no `shell`. `SiteOptions::shell(self, shell: Option<PathBuf>) -> Self`.
* `SiteOptions::prefix(&self) -> String`: `<name>:`. `SiteOptions::under(&self, path: &str) -> String`: `at` joined with `path`.
* `site_beside(app: &Path) -> Option<SiteOptions>`: the `[site]` of the configuration beside `app`, when that configuration names that app directory.
* `bundle_base(site: Option<&SiteOptions>) -> String` and `vendor_base(site: Option<&SiteOptions>) -> String`: `BUNDLE_BASE` and `VENDOR_BASE` under the site's prefix.
* `unprefixed(service: &str) -> &str`: a service name without its site prefix.

### ShellContract

* `pub struct ShellContract { pub version: u32, pub store: BTreeMap<String, String>, pub imports: BTreeMap<String, String>, pub frameworks: BTreeMap<String, String>, pub fsr: String, pub themes: Vec<String>, pub tokens: Vec<String> }`, `#[non_exhaustive]`, `Serialize`, `Deserialize`: `generated/shell.json`.
* `store` is every store key a shell's `store` export seeds, with its type as the browser reads it.
* `imports` is the shell's import map.
* `frameworks` is the exact version the shell vendors of `react`, `react-dom`, `vue` and `svelte`, read from its vendor manifest.
* `fsr` is the fsr version that wrote the contract.
* `themes` is the shell's named themes. `tokens` is every token the shell's root theme defines. Both are left out when empty.
* Every field but `version` defaults when absent.
* `SHELL_CONTRACT_VERSION`: `1`.
* `ShellContract::read(path: &Path) -> Result<ShellContract, BuildError>`: refuses a version above `SHELL_CONTRACT_VERSION` with `BuildError::Dev`.
* `ShellContract::declarations(&self) -> String`: `generated/shell.d.ts`, declaring `ShellStore` and `ShellImport`.

### build

* `pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError>`: imports `app/clients`, reads `app/schemas`, validates the contract, walks `app/routes` and lowers every body. Writes nothing.
* The first error in any file fails the whole build.
* `pub fn build_with(app: &Path, options: &Options, plugins: &mut plugins::Plugins) -> Result<Built, BuildError>`: `build` reading components through `plugins`. `build` is `build_with` over a pool of its own.
* `pub fn is_terminal_app(app: &Path) -> bool`: whether the configuration beside `app` says `[app] runs = "terminal"`.
* A terminal app lowers through `ComponentSet::for_terminal`, splits no page into an island and registers none. A component only a browser could render is `BuildError::BrowserOnlyInTerminal`.
* With `options.site` set, the plan is `Manifest::namespaced`, every contract file is `Contract::namespaced`, the islands registry registers `<name>:<module>` and the generated call sites call `<name>:<action id>`. The generated TypeScript is written unprefixed.
* With `site.shell` set, `generated/shell.d.ts` is written from the shell contract.
* Without a site, `generated/shell.json` is written.
* Before any route is lowered, every file under the source directories whose extension a plugin claims (`snapfire_compiler_wire::claimed`) is sent to that plugin in one describe batch.
* Each description is handed to `ComponentSet::describe`.
* A plugin not on `PATH` leaves its files foreign and adds a `Report::plugins` line.
* A plugin that starts and then fails is `BuildError::Plugin`.
* A file the plugin refuses is `ComponentSet::undescribed` with its diagnostics.
* A `Needs` answer is met once, with the sibling files read beside the source.
* A description carrying an `Error` diagnostic is kept as its script alone through `ComponentSet::script_only`. The file is `undescribed` with the errors.
* A relative import of a component `types::shadowed_components` finds, from a script under the source directories, is `BuildError::ShadowedImport`. `generated/islands.ts` imports such a component as `@app/<file>`.

### check

* `pub fn check(app: &Path, options: &Options, typecheck: &Typecheck, report: impl FnOnce(&Built)) -> Result<(Built, Option<Checked>), BuildError>`: `build`, then `write_generated`, then `tsconfig.json` and the foreign shim, then `report`, then `typecheck::run`.

### Built

* `pub struct Built { pub manifest: Manifest, pub contract: Contract, pub report: Report, pub files: Vec<(String, String)>, pub data: Vec<(String, Vec<u8>)>, pub defaults: SessionDefaults, pub browser_routes: Vec<String>, pub browser_files: Vec<String>, pub missing_declarations: Vec<String>, pub added: Vec<String>, pub composition_files: Vec<(String, Vec<String>)>, pub assets: AssetsManifest }`, `#[non_exhaustive]`.
* `data` pairs a path relative to the app with bytes: `generated/locale.data`. Empty for a site.
* `defaults` are the session defaults every body was lowered with.
* `browser_routes` are the route modules the browser mounts, as files relative to the app: every route module that is not `static`.
* `browser_files` are the files the browser loads, which the bundle and `fsr test` compile.
* `browser_files` holds every island the registry mounts, every module an `<Island define>` names, `browser_routes`, the islands split out of pages, the document's entry (`[document] entry` under the public path, else `src/main`), each browser half under `ext/`, every local file those import and each stylesheet a route's modules import.
* A server island and a template nothing mounts are not in `browser_files`.
* `missing_declarations` are the packages the build needs declarations for that `types/` lacks.
* `added` are the files only the bundle overlay holds: islands split out of pages and the copies a second JSX runtime compiles.
* `composition_files` pairs each route composition file with the local files it imports that the browser never loads.
* `assets` is `generated/assets.json`.
* `files` pairs a path relative to the app with its content, in this order: `generated/plan.sexp`, `generated/assets.json`, `generated/assets.d.ts`, `generated/contracts/<client>.json` per document in name order, `generated/contracts/schemas.json`, `generated/uploads.d.ts`, `generated/native.d.ts`, `generated/services.d.ts`, `generated/elements.d.ts`, `generated/host.ts`, `generated/islands.ts`, `generated/client.ts`, `tsconfig.json` and `tsconfig.build.json`.
* After those come the typed `.vue` declarations, `generated/shadowed/<file>.d.ts` per component `types::shadowed_components` finds that has none and `<types>/foreign.d.ts` when a source or a placement is in a language the build does not read.
* `generated/assets.d.ts` declares `*.png` and the other image and font extensions: an image import is `ImageAsset` and a font import is `string`.
* `generated/native.d.ts` comes from `native::read(dir: &Path) -> Result<Natives, BuildError>`, which reads every `#[native]` `impl` block's `pub` methods and the structs they name from the crate's `src/`, beside the app directory, with `syn`. It reads without expanding.
* In `native.d.ts`, a `fn` is typed as its value, an `async fn` as a promise and a Rust type outside the value model as `unknown`.
* `Built::adopt(&mut self, paths: &[String]) -> Result<Vec<(String, String)>, BuildError>`: defines the asset paths the bundle reported and the lowering never saw, then recomputes the asset manifest and the map. Returns the two files to write.
* `Built::refresh_assets(&mut self) -> Vec<(String, String)>`: the recomputation alone.

### write

* `pub fn write(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError>`: removes every `*.json` under `generated/contracts/`, the `.fsr/bundle/` directory and a `generated/foreign.d.ts`, then writes every entry of `built.files` and of `built.data` under `app`. An entry of `data` already holding the same bytes is left as it is. Returns the paths written.

### write_generated

* `pub fn write_generated(app: &Path, built: &Built) -> Result<(), BuildError>`: writes only the `generated/` entries of `built.files`, then every entry of `built.data`.

### write_overlay

* `pub fn write_overlay(app: &Path, built: &Built) -> Result<(), BuildError>`: removes `.fsr/bundle/` and writes only the entries of `built.files` under it.

### emit

* `pub fn emit(app: &Path, options: DevOptions) -> Result<Emitted, BuildError>`: what `fsr build` runs.
* `pub struct Emitted { pub built: Built, pub written: Vec<PathBuf>, pub checked: Option<Checked> }`, `#[non_exhaustive]`.
* First fetches declarations from the npm registry, as `fsr types` does, when `types::missing` or `types::unmet` names anything. Nothing is fetched under xwpm.
* A failed fetch is not an error by itself. The check before the bundle is `BuildError::Types` naming the packages still missing, with the fetch's error in parentheses.
* Then `build`, `write` and `snapfirec` over `tsconfig.build.json` into `<app>/dist`, with `options.public_path`, the layout's import map and `--overlay .fsr/bundle` when the build wrote one.
* Passes `--minify compact` unless the configuration says `server.dev = true`. Minified output is a `.min.js` beside each module and `"minified": ".min"` in the build facts, with plugins compiling for production.
* The typecheck runs alongside the bundle. `BuildError::Typecheck` when a diagnostic is an error, carrying the row and every diagnostic.
* `BuildError::Dev` names the compiler when it cannot start. It names the exit status when the compiler fails.
* `BuildError::Tool` names the compiler, the release its `--version` printed and `install::COMPILER_FLOOR` (`"0.18.0"`) when the release is older than the floor or `--version` names none. `fsr dev` and `fsr test` check the same floor.

### Report

What `build` found, in the order the report prints it.

* `pub struct Report { pub routes: Vec<(String, String)>, pub sources: Vec<(String, String)>, pub actions: Vec<(String, String)>, pub handlers: Vec<(String, String)>, pub middleware: Option<String>, pub themes: Vec<String>, pub theme_of: Vec<(String, String)>, pub theme_body: Option<String>, pub shell: Option<(String, usize, usize, Vec<String>)>, pub layouts: Vec<(String, String)>, pub slots: Vec<(String, String)>, pub intercepts: Vec<(String, String)>, pub components: Vec<(String, String, String)>, pub causes: Vec<Cause>, pub foreign: Vec<Cause>, pub plugins: Vec<String>, pub hoisted: Vec<Hoisted>, pub islands: Vec<(String, usize)>, pub extracted: Vec<(String, String, Vec<String>)>, pub kept: Vec<(String, String)>, pub extensions: Vec<(String, String)>, pub derives: Vec<(String, String, String)>, pub browser: Vec<(String, String)>, pub csp: Vec<(String, String)>, pub services: Vec<(String, String)>, pub schemas: Vec<(String, String)>, pub types: Vec<(String, String)>, pub images: Vec<(String, String)>, pub fonts: Vec<String>, pub embeds: Vec<(String, String)>, pub embed_failures: Vec<(String, String, String)>, pub browser_embeds: Vec<String>, pub tailwind: Option<(usize, Vec<(String, String)>)>, pub mdx: Vec<(String, usize, usize, String)>, pub theme_warnings: Vec<String>, pub unchecked: Vec<(String, String, String)>, pub shadowed: Vec<(String, String, String)>, pub plan: Option<usize> }`, `Debug`, `Clone`, `PartialEq`, `Eq`, `Default`, `#[non_exhaustive]`.
* `routes`: a pattern and its directory relative to `app`.
* `sources` and `actions`: an id and the module that lowered to it.
* `handlers`: `METHOD pattern` and the `route.ts` that exports it.
* `middleware`: `middleware.ts` when the app has one.
* `themes`: the named themes, the shell's among them for a site.
* `theme_of`: each layout or page that sets a theme, with its name, `<name> forced` when forced.
* `theme_body`: `theme.ts` when the app has one.
* `shell`: for a site built against a shell contract, its path, its store key count, its import count and the site's import map entries that differ.
* `layouts`: the pattern a layout wraps and its module.
* `slots`: a parallel slot's source id and its page module.
* `intercepts`: `<pattern> into <slot>` and the `page.<slot>.tsx` module.
* `components`: module, owner and detail. The owner is `lowered`, `client`, `foreign` or `template`.
* For `client` and `foreign`, the detail is the residue's `file:line:column`.
* For `lowered`, the detail is `static` when the owner is `fsr` and nothing places it as an island, `vue` or `svelte` for a component that framework's front end lowered, else empty.
* A `client` row is a component that did not lower, placed as a React island.
* `causes`: each residue behind a `client` row, once.
* `foreign`: each residue that left a described component foreign, once.
* `plugins`: one line per framework plugin the build could not start and what that left foreign.
* `hoisted`: one `Hoisted` per lowered component whose bundle copy the build rewrote.
* `islands`: each component placed as an island in server mode, prefixed for a site, with how many handlers it answers.
* `extracted`: each page or layout whose state, handlers, effects or residue moved into an island beside it, with the island's module and the names it took.
* `kept`: each page or layout that could not be split and stays a framework root, with the reason.
* `extensions`: each export under `ext/` as `file#name` with `lowered`, `native render` or `native body`.
* `derives`: each derived key with `lowered` and the file deriving it, else with `browser` and `file:line:column: reason`.
* `browser`: a lowered module, prefixed for a site, with `file:line:column` of each render-path call the browser still makes after hoisting. A handler it runs as written is `file:line:column: reason`.
* `csp`: each finding of `fsr doctor`'s `csp` check with its remedy.
* `services`: a service and its document. `schemas`: a type and its file. `types`: a package and its `types::status` row.
* `images`: each image the markup named or the images directory holds, with its size and variant count, as `160x100, 4 variants` or `served as it is`.
* `fonts`: one line per face the font directory holds, as `Inter 400 normal from fonts/Inter-Regular.woff2`, else per remote stylesheet.
* `embeds`: each `<Embed>` whose id the page computes under `[media] embed_stills = "local"`, as module and provider.
* `embed_failures`: each still `local` could not serve, as provider, id and why.
* `browser_embeds`: each component the browser renders alone that places an `<Embed>` under `local`.
* An `embeds`, `embed_failures` or `browser_embeds` entry always prints as an `embed` row and is refused by `[build] strict`.
* `tailwind`: set when a stylesheet under `styles/` imports Tailwind. The number of classes the plan renders, then each module with a class value the build cannot list, with that value.
* `mdx`: what converting an `.mdx` or `.md` file left out, as file, line, column and why.
* `theme_warnings`: a named theme setting `--breakpoint-*` or `--container-*`, one line each.
* `locales`: each locale the code names as a literal that `[locales]` does not cover, with the listed locales and the default the server formats it in.
* `unchecked`: each vendored React, Vue or Svelte newer than the latest release its renderer was checked against, as package, version and that release.
* `shadowed`: each framework package a site vendors at another version than its shell's contract pins, as package, site version and shell version.
* `plan`: the plan file's size in bytes.
* `Display` prints the `rendered` rows owned `client` or `foreign`, the `client` and `foreign` causes, `plugins`, `kept`, the `derived` rows owned `browser`, `browser`, `csp`, every `warning` row and a `shell` row naming the import map entries the shell maps differently. A clean build prints nothing.
* Each `client` cause prints as a `client` row, then `<n> module(s) render(s) in the browser for it`, then one indented row per module.
* Each `shadowed` and each `unchecked` entry prints as a `warning` row. A `shadowed` row names the `fsr add` that drops the copy.
* Each `tailwind` value the build cannot list prints as a `warning` row.
* `Report::verbose(&self) -> Verbose<'_>` prints every row: `routes`, `layouts`, `slots`, `intercepts`, `themes`, `theme`, the `shell` contract, `islands`, `plan`, `sources`, `actions` and `handlers` labelled `lowered`, `middleware`, `theme.ts`, every `rendered` row, `image`, `font`, `embed`, the `tailwind` count, `extracted`, every `derived` row, `extensions`, `hoisted`, `services`, `schemas` and `types`.
* Under `verbose`, `services` rows are labelled `http`, `grpc` or `rust` by their document's extension.
* Under `verbose`, `hoisted` rows name each count above zero: `4 values, 8 subtrees, 2 islands`.

### Cause

* `pub struct Cause { pub at: String, pub message: String, pub hint: Option<String>, pub pages: Vec<(String, String)> }`, `#[non_exhaustive]`: one residue that took modules out of server rendering.
* `at` is `file:line:column`. It is the file alone when the module did not parse.
* `pages` pairs each module that renders in the browser for it with the chain of placements from that module down to `at`. The chain is empty when the residue is in the module itself.

### Hoisted

* `pub struct Hoisted { pub module: String, pub values: usize, pub subtrees: usize, pub fallbacks: usize, pub islands: usize, pub keyed: usize, pub ids: usize, pub browser: usize }`, `Default`, `Ord`.
* `module` is prefixed for a site.
* `values` counts render-path calls the server computes. `subtrees` counts static subtrees the server renders. `fallbacks` counts those subtrees that keep a fallback in the bundle.
* `islands` counts islands it places. `keyed` counts keyed placements. `ids` counts `useId` calls. `browser` counts values only the browser knows, null through hydration.

### Plugins

* `pub struct plugins::Plugins`, `Default`: one `snapfire_compiler_wire::host::Worker` per extension, started on first use and kept for the command.
* `Plugins::new() -> Plugins`.
* The build describes through it. The driven compiler's `snapfirec: plugin` requests are answered from it.
* It keeps each file's last description by the whole unit: file name, source, options and the sibling files it was given. A rebuild sends only the units that changed.
* `Plugins::units_described(&self) -> usize`: how many units it has sent.
* A worker that fails a request is dropped with the descriptions it gave. The next request starts it again.
* `pub enum plugins::Refusal { Missing { binary: String, hint: String }, Failed(String) }`, `#[non_exhaustive]`: why a plugin cannot answer, reported once per command.
* `Refusal::Missing` displays as `` `<binary>` is not on PATH; `<hint>` puts it there ``.

### tailwind

The classes for a stylesheet that imports Tailwind, read from the plan.

* `pub const CLASSES_FILE: &str = "generated/tailwind.classes"`, `pub const CARDS_FILE: &str = snapfire_fsr_host::CARD_CLASSES_FILE`, `pub const UNLISTED: &str = "tailwind/unlisted-class"`.
* `pub fn wanted(styles: &Path) -> bool`: whether a `.css` file under `styles`, at any depth, has an `@import` naming `tailwindcss`.
* `pub fn collect(manifest: &Manifest) -> Classes`: every class the plan's components render.
* `collect` reads `class` on an element, a spread object's `className` or `class` and a `className`, `class` or `*ClassName` prop on a placement.
* `collect` follows both branches of a conditional, `&&`, `||`, `??`, a template's whole tokens, an array, an object's keys, `join`, `trim`, `concat`, `filter`, a `let`, a module constant and a field or an index into a literal object or array.
* A value reading `$props` is listed where the component is placed. Any other value the build cannot see is an `unlisted` row.
* `pub struct Classes { pub listed: BTreeMap<String, BTreeSet<String>>, pub unlisted: Vec<(String, String)> }`: each class with the modules using it. Each module with a value it cannot list.
* `Classes::file(&self) -> String`: the `CLASSES_FILE` text, one class per line, a tab, then the modules rendering it.
* `pub fn card_table(styles: &Path, classes: &Classes) -> Result<Vec<CardClass>, BuildError>`: `cards::card_rules` over the stylesheet Tailwind builds for the listed classes without polyfills. Uses the first stylesheet under `styles` that imports Tailwind, its imports resolved relative to it. Empty without one.
* `pub struct CardClass { pub class: String, pub min_width: Option<f32>, pub declarations: Vec<(String, String)> }`: one row of `CARDS_FILE`.
* `cards::card_rules(css: &str, classes: &BTreeSet<String>) -> Vec<CardRule>`: each of `classes` with a one-class rule in `css`, in the stylesheet's order, as `CardRule { class, min_width, declarations }`.
* A rule nested in anything but `&` or a minimum-width `@media` has no row.
* `var()` resolves to the rule's own custom properties, then the `:root` rules' variables, then an `@property` initial value, then the fallback.
* `calc()` of one unit is computed. `oklch()` and an opacity modifier's `color-mix()` become sRGB. Logical sides become physical ones.
* `cards::card_value(value: &str) -> String`: that rewrite of one value.

### assets

The build's half of images and fonts, `snapfire_fsr_cli::assets`.

* `pub struct Sections { pub dirs: DirsSection, pub images: ImagesSection, pub fonts: FontsSection, pub media: MediaSection }`.
* `Sections::of(app: &Path) -> Sections`: read from the configuration beside the app, defaults when there is none.
* `Sections::policy(&self) -> snapfire_fsr_assets::VariantPolicy`.
* `pub struct Resolver`: answers the lowerer's `AssetResolver` from the files.
* `Resolver::new(app: &Path, public_path: &str, sections: Sections) -> Resolver`.
* Each image is hashed with `snapfire_fsr_assets::hash::of`. Its size comes from its header, an SVG's from its root element.
* Each image's widths are chosen under the policy and any per-image override.
* Variants are named `<stem>.<hash>.<width>.<ext>` beside the hashed original under `public_path`, with `[images] base` prefixed.
* An image the lowerer never asked about is not an entry.
* `Resolver::scan(&self, dir: &Path, under: &str)`: records every image under a directory. `build` runs it over `[dirs] images`.
* `Resolver::entries(&self) -> Vec<ImageEntry>`, `Resolver::policy_out(&self) -> ImagePolicy` and `Resolver::original(&self, path: &str) -> Option<ImageFacts>`.
* `Resolver::refused: RefCell<Vec<(String, String)>>`: each file that was asked for and does not decode.
* `Resolver::reconcile(&self, paths: &[String]) -> Result<(), BuildError>`: defines every path the compiler reported as referenced and not in the map. An image is read for its widths and a font for its URL.
* `reconcile` is `BuildError::Assets` for a path that is neither or that this build cannot read.
* `Resolver::map(&self, fonts: &Fonts) -> snapfire_compiler_wire::driven::AssetMap`: every image and font this build defines, keyed by its path under the app.
* `assets::MAP_FILE`: `"generated/assets.map.json"`, passed to `snapfirec --asset-map`.
* `write_map(app: &Path, resolver: &Resolver, fonts: &Fonts) -> Result<(), BuildError>` and `map_text(resolver: &Resolver, fonts: &Fonts) -> String`.
* `assets::EMBEDS_DIR`: `"generated/embeds"`.
* `pub fn fonts(app: &Path, public_path: &str, sections: &Sections) -> Result<(Fonts, Vec<String>), BuildError>`: reads every woff2, woff, ttf and otf under `[dirs] fonts` for its family, weight, style and metrics.
* A `<file>.range` sidecar gives a face's `unicode-range`. A `<file>.family` sidecar gives the family it is matched and declared under, in place of the name table's.
* Each face is placed under a `[fonts.<key>]` by `files`, then by `family`, else under the family's slug.
* A variable face is declared at its `wght` axis, as `font-weight: 100 900`.
* `fonts` writes the `@font-face` rules, the `--font-<key>` variables and the preload list.
* Each key gets one fallback face against `fallback`, sized from the face nearest regular that has `a` to `z` to measure. A key whose faces have none gets no fallback face and a report line saying so.
* Refused by name: a `fallback` the asset crate does not know, a key naming no file, a `files` entry that is not there, a key with neither `family` nor `files` and a `remote` without `family`.
* The second value is the report's `fonts` lines.
* `pub fn manifest(resolver: &Resolver, fonts: Fonts) -> AssetsManifest`.
* `pub fn derive(app: &Path, out: &Path, manifest: &AssetsManifest) -> Result<Derived, BuildError>`: writes every hashed original, variant, hashed face and referenced font file the manifest names that is not under `out` already. Each source is decoded once.
* A file already there is kept. `emit` and the dev loop run `derive` after the bundle, under `<app>/dist`.
* `pub struct Derived { pub written: Vec<PathBuf>, pub kept: usize }`.

### intl

The locale data a build writes for the host.

* `pub const intl::DATA_FILE: &str = "generated/locale.data"`
* `pub fn intl::cut_of(app: &Path, plan: &str) -> Cut`: what the application's `[locales]` and plan call for, `snapfire_fsr_host::locale::cut` with `intl_data::compiled_source()` as its source. An application whose configuration does not load is cut as one with no `[locales]`.
* `pub fn intl::data(cut: &Cut, written: Option<Vec<u8>>) -> Result<Vec<u8>, BuildError>`: the bytes of `DATA_FILE` for `cut`.
  * `written`, what the last build left, is returned as it is when `IntlData::cut_of` reads the same cut from it.
  * The ICU4X data holds each listed locale and the locales it falls back to, with no descendants. The word dictionaries `cjdict`, `burmesedict`, `khmerdict`, `laodict` and `thaidict` are in when `cut.words`.
  * A list `Locales::new` refuses and a cut ICU4X cannot export are each `BuildError::Configuration` naming `[locales]`.
* `pub fn intl::unlisted(cut: &Cut, plan: &str) -> Vec<String>`: the locales `intl_data::named_locales` reads from the plan that no locale of `cut` covers.
* `build` writes the file for an application and none for a site. With `unlisted = "error"` a non-empty `intl::unlisted` is `BuildError::UnlistedLocales`.

## 3. Discovered Files

### Clients

* Every `app/clients/<name>.openapi.json`, in name order, is imported with `snapfire_fsr_service::import` as service `<name>`.
* Every `clients/<name>.proto` follows, imported with `snapfire_fsr_service::import_proto`, with feature `grpc`.
* Every `clients/<name>.graphql` follows, imported with `snapfire_fsr_service::import_graphql`, with feature `graphql`.
* Their types and services merge into one contract. A type name two documents define is `BuildError::DuplicateType`.

### Schemas

* Every `app/schemas/*.ts`, in name order, is read with `snapfire_fsr_lower::read_schema`. Each exported interface or string-literal union becomes a contract type.
* A name declared twice is `DuplicateType`.
* `snapfire_fsr_lower::builtin_types` joins them first, which supplies `Upload`. An application declaring its own `Upload` is `DuplicateType`.
* `Upload` is written as a TypeScript global in `generated/uploads.d.ts`.
* The type named `Session` types `ctx.session` and is imported into `generated/host.ts` from its file. Without one, `session` is `Record<string, unknown>`.
* An `export const defaults` in the `Session` file is read with `read_session_defaults` and folded into every lowered session read.
* `Contract::validate` runs after clients and schemas. An unresolved reference is `BuildError::Contract`.

### Extensions

* Every `app/ext/*.ts`, in name order, is lowered with `ComponentSet::lower_extensions` before any route.
* Each export is a `Report::extensions` row. One that does not lower fails the build with the lowerer's `Extension` error.
* `@ext/<name>` reaches the module from anywhere under the app. `ext/**/*` is in every generated tsconfig.
* A `body` extension on a component's render path fails the build with the lowerer's `Reach` error.

### Elements

* Every `app/elements/<tag>.tsx`, in name order, is a custom element's shadow template. Its default export renders with the element's attributes as its props.
* The file name is the tag. One that is not lowercase, does not start with a letter or holds no hyphen is `BuildError::ElementName`.
* Each template is lowered before any route, without hoisting. State or a handler is `BuildError::ElementTemplate`.
* A template whose root is a `<template shadowrootmode>` declares the element's shadow root, read by `ShadowRoot::take`. The plan carries `(shadow closed delegatesfocus)` in place of the element.
* A root template `ShadowRoot::take` refuses is `BuildError::ElementTemplate` with the refusal as the reason.
* A placement of the tag carries `render::SHADOW_ATTR` naming the template's module. The server writes the template inside the element as a declarative shadow root before its light children.
* The wrapper is `<template shadowrootmode="open">` unless the template declares its own root.
* A non-scalar attribute reaches the template without being written on the host element.
* `elements/**/*` is in the generated `tsconfig.json`. Nothing under `elements/` is compiled for the browser.

### Routes

* A directory under `routes/` is a route when it holds a page file: `page.tsx`, `page.ts`, `page.jsx` or `page.js`.
* `page.mdx` and `page.md` are page files with the `mdx` feature. `page.tera` is one with the `tera` feature.
* A directory is a handler route when it holds `route.ts`.
* A layout file is `layout.tsx`, `layout.ts`, `layout.jsx`, `layout.js` or `layout.tera`. Each layout on the way from `routes/` to a route wraps that route's page, outermost first.
* A page and a handler in one directory is `BuildError::PageAndRoute`.
* Two page files or two layout files in one directory is `PageAndTemplate` naming both.
* A template in an fsr built without the `tera` feature is `TemplateFeature`.
* A template's module is `routes/<dir>/page.tera#default`. Nothing is lowered or bundled for it and the report lists it under `rendered` as `template`.
* A template's `page.loader.ts` and `actions.ts` lower as beside a `page.tsx`. Its context is the loader's returned object.
* Every `island(` call in a template whose `module` is a string literal is bundled, registered and, in server mode, lowered. One whose `module` is not a literal is `TemplateIsland` naming the line.
* A `page.<slot>.tsx` under a `layout.tera` is refused as undeclared. A `slots/<name>/` directory beside it is placed by `slot(name="<name>")` in the template.
* An `actions.ts` is read beside a page or in a slot. One in a directory with no page is `BuildError::ActionsWithoutPage`.
* `slots/<name>/` beside a layout is a parallel slot of that layout, with source id `layout.<name>` (`<layout id>.<name>` for a nested layout). It reads `page.tsx`, `page.loader.ts`, `loading.tsx` and `error.tsx`.
* `slots/` with no layout beside it is `BuildError::SlotsWithoutLayout`. A slot without `page.tsx` is `SlotWithoutPage`. A slot with a page or handler directory beneath it is `SlotRoute`.
* A layout declares every slot its template places with `<Slot name>`.
* `page.<slot>.tsx` beside a route's `page.tsx` is an intercept: an entry under the route's pattern in the manifest's `intercepts`.
* An intercept holds the layouts down to the nearest one declaring `<slot>`. That layout carries the variant as its `<slot>` child, with its page and every other slot in `keep`. Each layout above carries its own slots in `keep`.
* An intercept shares the route's source and error module and streams behind `loading.<slot>.tsx` alone. A route with several variants has one entry each, in file order.
* A slot no layout above declares is `SlotUndeclared`.
* Before a page, a layout, a slot page or a route's boundary is lowered, `ComponentSet::extract_route` moves its state, handlers and effects into an island beside it. The split is a `Report::extracted` row. One that cannot be split stays a React root and is a `Report::kept` row.
* `ComponentSet::lower_route` does the same for residue in the module's own body.
* A component that does not lower is a `client` row and a React island. In an application without React it is `BuildError::NoJsxFramework`.
* `export default tree(...)` in any route module is `BuildError::Lower` holding `LowerError::Retired`.
* Routes are sorted by pattern before ids are assigned. Node ids are assigned in tree order per plan, the shell at 0.

### Segments and Ids

* A segment is a name of ASCII letters, digits, `_`, `-` and `.`, not all dots. `[name]` is a parameter, `[...name]` a catch-all and `(name)` a group. Anything else, `()` included, is `BuildError::Segment`.
* A group adds nothing to the pattern. Its layouts and boundaries wrap the routes beneath it.
* `routes/` is `/`. No directory name is special: `routes/index/` is `/index`.
* A parameter is `{name}` in the pattern and a catch-all is `{*name}`.
* Source id: every segment joined with `.`, `$root` for the root. A parameter or a catch-all contributes `$<name>`. A `.` in a name is `~`. A group contributes `(<name>)`.
* `routes/product/[id]` is `product.$id`. `routes/docs/[...rest]` is `docs.$rest`. `routes/(shop)/cart` is `(shop).cart`.
* Action id: `<source id>.<export>` for each export `lower_actions` returns.
* Layout id: `layout` for `routes/layout.tsx`, else `<directory id>.layout`. It names the layout's loader as a source.
* Handler id: `<route id>.<METHOD>` for each export of `route.ts` named `GET`, `POST`, `PUT`, `PATCH` or `DELETE`. The row carries the method and the pattern.
* A handler's `action<T>` export names `T` as its input. A `T` no schema declares is `UnknownHandlerInput`.
* Two rows deriving one id is `ClaimedId`, naming the kind, the id and both files. Route, source, action, handler and props-type names are each checked.
* The props-type name drops `$` and a group's parentheses. `routes/a/x` beside `routes/a/[x]` is refused on it. `(shop).cart` is `ShopCartProps`.
* Two routes or handler routes matching the same URLs is `ClaimedPattern`, naming the pattern and both directories. Parameter names are not compared.
* Every `derive` in a script file the browser loads becomes a plan `derive` row through `ComponentSet::lower_derives`.
* A key derived in two places is `DerivedTwice`. Derived keys that read each other in a loop are `DeriveCycle`.
* A component placed as an island in server mode is `ServerIsland` when a handler in it or in a component it renders did not lower, when a handler reads a name the markup around it bound or when the same module is also placed as a browser island.

### Modules

* Component modules take `.tsx`, `.ts`, `.jsx` or `.js`, in that order of preference.
* Page: `<route dir>/page.tsx#default`, the directory relative to `app`.
* Error: `routes/error.tsx#default` when present, applied to every page. A route's own `error.tsx` takes precedence for that route.
* Error per kind: `error.<kind>.tsx#default` beside either, one per `FailureKind::ALL` entry spelled with hyphens, as `error.not-found.tsx` and `error.too-many-requests.tsx`. They land on the node as `error_kinds`.
* A route declaring any boundary supplies the whole set.
* Loading: `<route dir>/loading.tsx#default` when present. The node is deferred with it as the fallback.
* Not found: `routes/not-found.tsx#default` when present, the page for a path no route matches.
* Layout: `<dir>/layout.tsx#default`, with `<dir>/layout.loader.ts` as the source row under the layout id.
* Loader, actions and route modules are named by their relative paths in the source, action and handler rows.
* Middleware: `middleware.ts` at the top of the app, lowered as `Manifest.middleware`.

### Plan Shape

* Every route is a chain. Node 0 is the shell module. Each wrapping layout follows, in `Options::slot` for the shell and `content` for a layout, with ids from 1, its source and the routes-level error module. The page comes last with its source, its error module and its fallback.
* `not_found` is the same chain around the not-found module, inside the root layout when there is one. The host renders it with status 404 and `params.path` set to the path asked for.
* Sources and actions are emitted with `RowOwner::Lowered` and their bodies.
* An action whose `action<T>` names a type the contract lacks is `UnknownInput`. An action row carries `input` when it names one.
* `frameworks` holds the exact version of `react`, `react-dom`, `vue` and `svelte`.
* A site takes each `frameworks` version from the shell contract. Any other application takes it from `vendor/.fsr-vendor.json`.
* A site's own vendored copy of a package the shell contract pins is ignored. A copy at another version is a `Report::shadowed` row.
* An import map serving a framework package with no version recorded is `FrameworkShellUnrecorded` when the shell serves it, else `FrameworkUnrecorded`.
* A React major other than 18 or 19 is `ReactMajor`. A Vue major other than 3 is `VueMajor`.

### Generated Files

* `generated/contracts/<client>.json`: `Contract::to_json` of that document's import. `generated/contracts/schemas.json`: the schema types. `generated/contracts/rust.json`: every `#[service]` block's contract.
* `generated/services.d.ts`: `snapfire_fsr_service::typescript::declarations` of the merged contract.
* `generated/host.ts`: what the generated `tsconfig.json` maps `@snapfire/fsr/host` to.
* `host.ts` re-exports every host standard module from `@snapfire/fsr/host/index`, imports the base package as `@snapfire/fsr/authoring`, re-exports `fail` and `Services` and imports `Session`.
* `host.ts` declares `Routes`, one key per page and handler pattern, each value with a `string` field per parameter.
* `host.ts` declares `Ctx<P extends keyof Routes = keyof Routes>` with `params`, `query`, `session`, `identity`, `locale`, `services` and `now`.
* `host.ts` declares `ActionCtx<Input, P>`, `HandlerCtx<P>` and an `action<Input, Out>` wrapper over the base package's.
* `ActionCtx`, `HandlerCtx<P>` and `MiddlewareCtx` type `session` as `Session & SessionControl`, which adds `extend(seconds: number)`. `Ctx` does not.
* `host.ts` declares `RequestLine` and `MiddlewareCtx` for `middleware.ts`. `MiddlewareCtx.next` runs the route.
* `host.ts` declares `Meta`, `MetaCtx<Data>` and `DataOf<typeof load>` for a loader module's `meta`.
* `host.ts` declares `Config`, one field per `[public]` key typed from its value, as the type of `ctx.config`.
* `host.ts` declares `Theme` as the union of the named themes (`never` with none), `Scheme` as `"light" | "dark"`, `RouteTheme { name: Theme | null; forced: boolean }`, `ThemeCtx extends Ctx` with `theme: RouteTheme` and `ThemeChoice { name?: Theme | null; scheme?: Scheme | null }`.
* A loader module's exported `meta` lowers into the source row's `meta`. Its exported `store` lowers into the row's `store`.
* `generated/client.ts` imports `action as call` from `@snapfire/fsr/client` and prints every contract type in client flavour.
* `client.ts` declares `HostProps` with `params`, `locale`, `identity`, `csrf_token`, `csp_nonce` and `action_failure`.
* `client.ts` declares one `export type <Id>Props` per route and layout, from `infer::Inferer::returns` over its loader (`{}` without one) intersected with `HostProps`. The root layout's is `LayoutProps` and `routes/account/layout.tsx`'s is `AccountLayoutProps`.
* `client.ts` exports `actions`, nested by the dots of each action id, each `call("<id>") as unknown as (input: <Input>) => Promise<<returns>>`.
* `generated/islands.ts` exports `registerIslands()`, one `registerDeferredIsland` call per module. It imports no adapter directly.
* `islands.ts` writes one loader per adapter a registered module needs, as `const react = () => import("@snapfire/fsr/client/react").then((m) => ({ mount: m.reactMounter, ... }))`.
* The adapters are `@snapfire/fsr/client/react` for `react`, `@snapfire/fsr/client/vue` for `vue` and `@snapfire/fsr/client/svelte` for `svelte`. The Svelte loader also imports `./components/Pieces.svelte` and hands its tree to `registerTree`.
* An element definition is registered with `registerIsland` and `defineMounter`.
* A module with no lowered row takes the adapter its extension names.
* The registry holds, in order and without modules reported `static`: the routes-level error module, the not-found module, each layout, then each page with its error and loading modules, then every component placed as an island, every hydrated component a composition module renders inline, every island split out of a page, every component of another framework a React component renders inline and every component that did not lower.
* Each loader picks the named export from `../<path>.js` relative to `generated/`.
* A split-out island's call carries a `// @ts-ignore` line above it.
* An island whose extension a plugin claims (`snapfire_compiler_wire::EXTENSIONS`) but no adapter mounts is `BuildError::NoAdapter`. One whose extension nothing claims is `BuildError::UnknownComponent`.
* With an import map beside the app, each adapter module the registry imports and the specifiers that adapter imports must resolve in it. React needs `react` and `react-dom/client`, Vue needs `vue` and `@snapfire/fsr/client/jsx-runtime` needs nothing.
* The adapter modules must resolve in `snapfire_fsr_host::client::import_entries()` unless a `[[static]]` root answers `/static/js/fsr`. For a site they must resolve in the shell contract's `imports`.
* A trailing-slash key and a scope both count. A failure is `BuildError::IslandImports`. For the JSX runtime it names the line to add, `/static/js/fsr/jsx-runtime.js`. An app with no import map is not checked.
* `generated/elements.d.ts`: `types::element_declarations`.
* `generated/tailwind.classes` (`tailwind::CLASSES_FILE`): written when a stylesheet under `styles/` imports Tailwind, from `tailwind::collect` over the plan. Passed to snapfirec as `--tailwind-classes`.
* `generated/tailwind.cards.json` (`tailwind::CARDS_FILE`): written with card images in the plan, from `tailwind::card_table`, as `[{ class, min_width, declarations }]`.
* `tsconfig.json`: `types::tsconfig`. `tsconfig.build.json`: `types::tsconfig_build`, with `jsx` `react` when the import map has `react`, else `@snapfire/fsr/client`.
* `tsconfig.build.json` has a `files` list naming each island split out of a page.
* `.fsr/test/tsconfig.json`: the same roots plus the specs.
* `.fsr/bundle/<path>`: the browser copy of every module the build rewrote, read by snapfirec through `--overlay` in place of the original.
* The overlay holds each lowered component module with a hoist (`hoist::apply` over its source), each page or layout the build split, as composition placing its island.
* The overlay holds each island split out, `<stem>.island0.tsx` beside the page.
* The overlay holds each file an island reaches through its local script imports that imports the dialect's placements, pointed at the island's adapter.
* In an application that serves React, the overlay holds every `.tsx` and `.jsx` file importing a component in a plugin's language, each such import replaced by `const Name = foreign("<file>#<export>")` from `@snapfire/fsr/client/react`.

### Themes

* `styles/themes/<name>.css` (`THEMES_DIR = "themes"` under the styles directory): one named theme per file.
* `theme_files(dir: &Path) -> Result<Vec<String>, BuildError>`: the theme names, sorted, with `ROOT_THEME` (`"default"`) among them when there is at least one file. A `default.css` is `BuildError::ReservedTheme`.
* `themes::write(styles: &str, themes: &[NamedTheme], root: &RootTokens, shell: Option<&[String]>) -> Result<Written, BuildError>`: writes each theme into the bundle overlay over its own file as plain CSS.
* Each written theme starts with `@layer theme.root, theme.named;` (`themes::LAYER_ORDER`), then `[data-sf-theme="<name>"]` (`THEME_ATTRIBUTE`) in `@layer theme.named` with every token it sets.
* An app that is not a site also gets `styles/themes/default.css` in the overlay: the root's value of every token a theme sets and of every token those values read through `var()`.
* `themes::root_tokens(styles: &Path) -> Result<RootTokens, BuildError>`: the root's tokens, from the `@theme` blocks of every top-level stylesheet and the stylesheets they import. Also from `:root` rules in `@layer theme.root`.
* `themes::named_themes(app: &Path, styles: &str) -> Result<Vec<NamedTheme>, BuildError>`.
* A theme file's tokens are its bare custom properties and those of its `@theme` blocks and `:root` rules.
* `app/theme.ts` exporting `theme` lowers into the plan's `theme` body. Each layout's and page's `export const theme` becomes a `theme-of` row.

## 4. Terminal Programs

### terminal

* `terminal::MODULE`: `"routes/page.tsx#default"`.
* `terminal::run(app: &Path, args: &[String]) -> Result<(), BuildError>`: what `fsr dev --terminal` runs in place of the loop.
* `run` lowers `MODULE` under `app`, holds the terminal in raw mode on the alternate screen and draws the page with `snapfire_fsr_terminal` at the terminal's width, 80 when it reports none.
* `run` runs the component's `input` handler on each key and writes `Frame::diff` against what is shown.
* A change under `app` outside `generated/`, `dist/` and `.fsr/` lowers the page again. A resize redraws it whole. Ctrl-C returns `Ok(())`.
* A page that fails to lower shows why in place of the page until an edit fixes it.
* `terminal::open_watching(app: &Path, args: &[String], refused: impl FnMut(&str) -> io::Result<bool>) -> io::Result<Option<Session>>`.
* `terminal::params(args: &[String]) -> Params`: the command line as the loader's `params`.
* `terminal::key_event(event: KeyEvent) -> Option<Press>`: `Press::Exit` for Ctrl-C, `Press::Input({ input, key })` for a character or a named key, `None` for a key Ink does not report.
* `terminal::KEYS: [&str; 14]`: the flags of `key`. `terminal::key_declaration() -> String`: the `Key` type.
* `terminal::spawned(spawn: Spawn, input: Vec<StdinWrite>) -> Receiver<(Spawn, Report, Value)>`: starts the process `spawn` names.

### Session

* `pub struct terminal::Session`: the root page of a terminal application, lowered, with the state its handlers stepped it to.
* `Session::open(app: &Path) -> Result<Session, String>`: the page under `app`, lowered.
* `Session::open_with(app: &Path, params: Params) -> Result<Session, String>`: with `params` for its loader.
* `Session::open_module(app: &Path, module: &str) -> Result<Session, String>`: the component `module` names, as the root.
* `Session::with_extensions(self, extensions: Arc<Extensions>) -> Session`: `extensions` answer its handlers' calls in place of the standard library.
* `Session::view(&mut self) -> Result<Node, String>`: the page as it stands.
* `Session::press(&mut self, press: &Value) -> Result<Node, String>`: the page after every `useInput` handler it renders runs on `press`.
* `view` and `press` answer `Err` when the page renders anything but Ink's elements.
* `Session::reload(&mut self) -> Result<(), String>`: lowers the page again from disk, keeping the state and the loader's result.
* `Session::resize(&mut self, columns: u16, rows: u16)`.
* `Session::timers(&self) -> Vec<Timer>`: the timers the mount effects of the last render start, in order.
* `Session::take_started(&mut self) -> Vec<Timer>`: the timers started since the last call.
* `Session::fire(&mut self, timer: &Timer) -> Result<Node, String>`: the page after `timer` fires.
* `Session::holds(&self, timer: &Timer) -> bool`: whether the component that started `timer` rendered last time and nothing cleared it.
* `Session::take_spawns(&mut self) -> Vec<Spawn>` and `Session::take_stdin(&mut self) -> Vec<StdinWrite>`: the processes started and the standard input written since the last call.
* `Session::deliver(&mut self, spawn: &Spawn, report: Report, event: &Value) -> Result<Node, String>`: the page after `spawn` reports `report` with `event`.
* `Session::cursor(&self) -> Option<(u16, u16)>`: where the page last put the cursor with `setCursorPosition`.
* `Session::exited(&self) -> bool` and `Session::exit_code(&self) -> Option<i32>`.
* `Session::wants_screen(&self) -> bool`.
* `pub struct Timer { pub id: Option<String>, pub path: String, pub handler: usize, pub every: Duration, pub repeats: bool, pub settles: Option<(String, bool)>, pub carry: Value }`.
* `pub struct Spawn { pub path: String, pub id: String, pub command: String, pub args: Vec<String>, pub options: Value, pub stdin: bool }`.
* `pub struct StdinWrite { pub path: String, pub id: String, pub text: Option<String> }`. `text` of `None` closes the input.
* `pub enum Press { Input(Value), Exit }`.

## 5. Desktop Window

### desktop::run

* `desktop::run<W>(name: &str, scheme: Option<Handler>, work: W) -> Result<(), BuildError>` where `W: FnOnce(desktop::Window) -> Result<(), BuildError> + Send + 'static`: runs `work` on its own thread and the window on the calling thread.
* On macOS the calling thread must be the main thread.
* Returns the work's result when the work ends. It also returns when the window closes and the stop the work registered has run. With no stop registered, a closed window returns `Ok(())` at once.
* The window opens titled `name` and takes `window_title` of each document title the webview reports.
* Back and forward swipe gestures are on.
* `run` installs the menu bar `desktop::menus` lays out.
* `desktop::LOOPBACK`: `"127.0.0.1:0"`.

### Window

* `pub struct desktop::Window`.
* `Window::open(&self, url: String)`: loads `url` once its address accepts a connection.
* `Window::on_close(&self, stop: impl FnOnce() + Send + 'static)`: registers the stop.
* `desktop::url(listen: &str, tls: bool) -> Result<String, BuildError>`: the URL the window opens for a listen address. A wildcard address maps to loopback. Port 0 and anything that is not an address are refused.
* `desktop::name(app: &Path) -> String`: the configuration's `[desktop] name`, else the project directory's name.
* `desktop::window_title(name: &str, page: &str) -> String`: `<name> - <page>`, else `name` when `page` is blank or equal to it.

### Menus

* `desktop::menus(platform: Platform, name: &str) -> Vec<MenuSpec>`. `pub struct MenuSpec`, `pub enum Entry`.
* `pub enum Command { Back, Forward, Reload, Quit }`.
* `desktop::shortcut(command: Command, platform: Platform) -> Option<&'static str>`.
* `desktop::effect(command: Command) -> Effect`: `Effect::Script` runs in the page, `Effect::Reload` reloads it, `Effect::Close` is handled as a close request.
* `pub enum Platform`. `Platform::CURRENT` is the platform built for.

### Bridge

* `desktop::BRIDGE_SCRIPT`: installed with `with_initialization_script` as `window.__sfDesktop`. Its `call(method, args)` posts `{ id, method, args }` through `window.ipc.postMessage`.
* `desktop::bridge_call(body: &str) -> Result<Call, (Option<u64>, String)>`: reads one posted message. A refusal carries the id when there was one.
* `pub struct Call { pub id: u64, pub method: Method }`.
* `pub enum Method { OpenFile { multiple: bool, filters: Vec<(String, Vec<String>)> }, OpenExternal { url: String } }`.
* `OpenFile` is answered through `rfd::FileDialog`. `OpenExternal` is answered through `open::that_detached` when `external_allowed` holds.
* `desktop::external_allowed(url: &str) -> bool`: `http`, `https` or `mailto` with something after the scheme.
* `desktop::settle_script(id: u64, result: &Result<serde_json::Value, String>) -> String`: `window.__sfDesktop.settle(id, ok, value)`, with `<` written `<`.

### Scheme

* `desktop::scheme(host: Arc<Host>, runtime: tokio::runtime::Handle, cookies: Option<PathBuf>, log: bool) -> Handler`: answers each `app://` request with `Host::handle` on `runtime`, the body collected whole.
* `pub type Handler = Arc<dyn Fn(http::Request<Vec<u8>>, Box<dyn FnOnce(http::Response<Vec<u8>>) + Send>) + Send + Sync>`.
* `desktop::SCHEME_URL`: `"app://localhost/"`.
* `desktop::to_host(request: http::Request<Vec<u8>>, cookie: Option<String>) -> http::Request<Bytes>`: the path and query alone, with `Host: localhost` and the jar's `Cookie`.
* `desktop::followable(response: http::Response<Vec<u8>>, navigation: bool) -> http::Response<Vec<u8>>` and `desktop::streams(headers: &http::HeaderMap) -> bool`.
* `desktop::scheme_limits(manifest: &Manifest) -> Vec<String>`: what the boot prints about the scheme.
* `pub struct desktop::CookieJar`: holds what the host sets.
* `CookieJar::store(&mut self, set_cookie: &str, now: u64) -> bool`: reads the name, value and `Max-Age`. A `Max-Age` of `0` or less removes the cookie.
* `CookieJar::header(&self, now: u64) -> Option<String>`.
* `CookieJar::load(file: &Path, now: u64) -> CookieJar`: keeps what has not expired.
* `CookieJar::save(&self, file: &Path) -> io::Result<()>`: writes every cookie with an end, mode `0600` on Unix.
* `[desktop] cookies = "disk"` puts the jar at `desktop::cookie_file(name: &str) -> Option<PathBuf>`: `cookies.json` under `desktop::data_dir(platform: Platform, name: &str, home: &Path, appdata: Option<&Path>, xdg_data: Option<&Path>) -> PathBuf`.

## 6. Inference

### Ts

* `pub enum infer::Ts { Undefined, Str, Num, Big, Bool, Null, Unknown, Never, Named(String), TsExpr(String), List(Box<Ts>), Map(Box<Ts>), Tuple(Vec<Ts>), Record(Vec<(String, Ts)>), Union(Vec<Ts>), Inter(Vec<Ts>) }`.
* `Never` is what an empty array literal holds. `TsExpr` is a TypeScript type expression written as is.
* `Ts::print(&self, flavour: Flavour) -> String`. `Big` prints as `bigint` on the server and `bigint | number` on the client.
* A union or a `Big` inside a list or an intersection is parenthesised.

### Inferer

* `pub struct infer::Inferer<'a> { pub contract: &'a Contract, pub session: Option<&'a str>, pub input: Option<&'a str>, pub input_type: Option<Ts>, pub config: &'a [(String, Ts)], pub consts: &'a Consts, pub natives: &'a [NativeModule] }`, `#[non_exhaustive]`.
* `session` and `input` are contract type names. `input_type` is the type of `input` when it is not a contract type.
* `config` is `[public]` as the build read it. A key it lacks is `unknown`.
* `natives` are the `#[native]` modules. A `ctx.native` call types as the method's declared return.
* `Inferer::returns(&self, body: &Body) -> Ts`: the union of every `return`, `Null` when none.
* `Inferer::expr(&self, expr: &Expr, env: &[(String, Ts)]) -> Ts`.
* `expr` types a read by its root, a call by its method's return, a native call by its Rust return and a session key by the `Session` record.
* `expr` types `map` by its lambda's body over the element, `filter` by its operand and `Object.entries` as `[string, V][]`.
* `expr` types an object literal as a record intersected with its spreads. A coalesce against an empty object or array types as its left side.
* Anything else is `Unknown`, which absorbs a union it joins.

## 7. Typechecking

`fsr` spawns `snapfiretc` and renders what it reports. The checker's own surface is in [snapfire_typecheck](../../typecheck/API_REFERENCE.md).

### Typecheck

* `pub struct Typecheck { pub enabled: bool, pub checker: Option<PathBuf>, pub tsc: Option<PathBuf>, pub version: Option<String>, pub expect: Option<String> }`, `#[non_exhaustive]`, `Default`.
* `tsc` is a compiler for the checker to use as given. `version` is the TypeScript version, the checker's default when absent. `expect` is the integrity a fetched compiler must have.
* Setters `enabled(self, bool)`, `checker(self, Option<PathBuf>)`, `tsc(self, Option<PathBuf>)`, `version(self, Option<String>)` and `expect(self, Option<String>)`, each returning `Self`.
* `Typecheck::beside(app: &Path) -> Typecheck`: the `[typecheck]` section of the configuration beside `app`. A project with no configuration is enabled with the checker's default version.
* `Default` is disabled with every field absent. `DevOptions::default` and `DevOptions::beside` enable it.

### Checked

* `pub struct Checked { pub version: String, pub source: String, pub sha512: Option<String>, pub pinned: bool, pub diagnostics: Vec<Diagnostic> }`: the checker's JSON report.
* `Checked::errors(&self) -> usize`: the diagnostics whose severity is `error`.
* `Checked::row(&self) -> String`: the report row, as `tsc 7.0.2 from cache, 1 error`.

### Diagnostic

* `pub struct Diagnostic { pub file: Option<String>, pub line: u32, pub column: u32, pub code: String, pub severity: String, pub message: String }`.
* `Diagnostic::is_error(&self) -> bool`.
* `Display` prints `file(line,column): severity code: message`.

### spawn

* `pub fn spawn(app: &Path, options: &Typecheck) -> Result<Option<Child>, BuildError>`: starts the checker over `<app>/tsconfig.json` with `--format json`.
* `None` when typechecking is off, the tsconfig has not been written or no checker is installed.

### finish

* `pub fn finish(child: Option<Child>) -> Result<Option<Checked>, BuildError>`: waits and reads the report. Writes nothing.
* `BuildError::Typecheck` when the child printed nothing or something that is not a report.

### run

* `pub fn run(app: &Path, options: &Typecheck) -> Result<Option<Checked>, BuildError>`: `spawn`, then `finish`.

### find_checker

* `pub fn find_checker(explicit: Option<&Path>) -> PathBuf`: `explicit`, else `$SNAPFIRETC`, else `snapfiretc` beside the running binary, else the bare name for `PATH`.
* `pub const CHECKER: &str = "snapfiretc"`.

### pin

* `pub fn pin(app: &Path, checker: Option<&Path>) -> Result<Option<(PathBuf, String)>, BuildError>`: records the checker's default TypeScript version, from `snapfiretc --version`, in the first `.toml` source of the configuration beside `app` when its `[typecheck]` names none. Answers the file and the version.
* `None` when a version is named, there is no TOML configuration or no checker answers.
* `types::fetch` calls it. `fsr new`, `fsr use`, `fsr add` and `fsr types` pin the version. `fsr build`, `fsr check`, `fsr dev` and `fsr test` never write the configuration.

### record

* `pub fn record(path: &Path, version: &str, sha512: Option<&str>) -> Result<bool, BuildError>`: writes `version` into the file's `[typecheck]` section, adding the section when it has none.
* `false` when the section already names a version.

## 8. Vendoring and Declarations

### Layout

* `pub struct xwpm::Layout { pub vendor: String, pub base: String, pub importmap: String, pub types: String, pub xwpm: bool, pub modules: Vec<String> }`: paths relative to the app directory. `modules` is the keys of `xwpm.wmf`'s `[modules]`.
* `Layout::of(app: &Path) -> Result<Layout, BuildError>`: `of_site` with the `[site]` section beside `app`.
* `Layout::of_site(app: &Path, site: Option<&SiteOptions>) -> Result<Layout, BuildError>`: the defaults `vendor`, `/static/js/vendor`, `importmap.json`, `types` and `xwpm: false`.
* With `<app>/xwpm.wmf` present, `of_site` reads its root records and sets `xwpm: true`.
* For a site, `base` is `<at>/static/js/vendor` unless a `base` record in the wmf names another.
* `Layout::from_wmf(text: &str) -> Result<Layout, String>`: root records `vendor`, `base`, `importmap` and `types` override the defaults. Other root records and other sections are skipped. Each key of `[modules]` goes into `modules`.
* A line that is not `key = value` is an error naming its line.
* `xwpm::run(app: &Path, args: &[&str]) -> Result<(), BuildError>`: runs `xwpm` in the app directory. `BuildError::Xwpm` when it cannot start or exits non-zero.
* `xwpm::XWPM_FILE`: `"xwpm.wmf"`.

### Spec

* `pub struct vendor::Spec { pub package: String, pub version: String, pub subpath: Option<String> }`, `#[non_exhaustive]`.
* `Spec::parse(raw: &str) -> Result<Spec, BuildError>`: `name@version`, `name@version/subpath` or `@scope/name@version[/subpath]`. Anything else is `BuildError::Spec`.
* `Spec::specifier(&self) -> String`: `package` or `package/subpath`, the import map key.

### vendor::add

* `pub fn vendor::add(app: &Path, specs: &[Spec], externals: &[String], hosts: &[String]) -> Result<AddReport, BuildError>`.
* Under the default layout, per spec: `GET <host>/<package>@<version>[/<subpath>]?target=es2022&bundle[&external=<externals>]`.
* Follows every absolute path the stub names and writes each file by its base name under `<vendor>/<package>/`.
* Rewrites same-package absolute imports to `./<name>`. Any other absolute import is `BuildError::Dependency`.
* The file behind the stub's `export *` is the entry, written to the import map as `<base>/<package>/<name>` and to the vendor manifest.
* Every entry the manifest records whose map URL is not under `base` is rewritten to it and reported as `remapped`. A specifier the map does not carry is left out.
* Under xwpm: `xwpm add <package>@<version>` once per distinct package and version.
* In a site whose shell already serves a specifier, nothing is fetched for it. The map takes the shell's URL and the specifier is reported in `from_shell`.
* A version other than the one the shell contract records for that package is `BuildError::ShellPinned`.
* A package the shell contract pins in `frameworks` that the site vendored itself leaves the vendor manifest, its directory is removed and it is reported in `dropped`.
* `pub struct AddReport { pub added: Vec<(String, String, usize)>, pub remapped: Vec<(String, String)>, pub from_shell: Vec<(String, String)>, pub dropped: Vec<String>, pub delegated: Vec<String>, pub repaired: Vec<String> }`.
* `added`: specifier, file relative to the vendor directory and bytes. `remapped`: specifier and the URL it now carries. `from_shell`: specifier and the shell's URL.
* `dropped`: each package the site no longer vendors. `delegated`: the xwpm invocations run. `repaired`: `<package>/<file>` for each vendored module whose source map was repaired.
* `pub fn vendor::running(app: &Path, layout: &Layout) -> Result<VendorManifest, BuildError>`: the vendor manifest less every package the site's shell pins in its contract's `frameworks`.
* `vendor::read_import_map(app: &Path, layout: &Layout) -> Result<serde_json::Map<String, Value>, BuildError>` and `vendor::write_import_map(app: &Path, layout: &Layout, map: &serde_json::Map<String, Value>) -> Result<(), BuildError>`: the `imports` table, whole. Other keys survive.
* `vendor::import_map_packages(app: &Path, layout: &Layout) -> Result<Vec<String>, BuildError>`: the bare keys as package names. `react/jsx-runtime` is `react` and `@a/b/c` is `@a/b`.
* `vendor::package_of(specifier: &str) -> String`.
* `vendor::client() -> Result<reqwest::blocking::Client, BuildError>` and `vendor::get(client: &Client, url: &str) -> Result<Option<Vec<u8>>, BuildError>`.
* `vendor::ESM_HOST`: `"https://esm.sh"`. `vendor::VENDOR_MANIFEST`: `".fsr-vendor.json"`.

### published

* `pub fn published::add_published(app: &Path, specs: &[Spec]) -> Result<AddReport, BuildError>`: vendors each package as published on npm.
* Per package: the version document from `https://registry.npmjs.org/<name>/<version>`, refused unless it answers for that version.
* The tarball is refused unless it hashes to the document's `sha512` integrity.
* Its regular files are kept by their path under the top directory. An entry whose path climbs out or holds a `\` or a `:` is left out.
* Each spec's entry resolves through `exports` under the conditions `browser`, `production`, `import`, `module` and `default`, else `module`, `main` or `index.js`.
* The files it reaches by relative import or `#alias` are kept. An alias is rewritten to the relative path it names.
* Each bare import is vendored the same way at the highest version the importer's declared range accepts.
* A name that is not an npm package name is `BuildError::PackageName`.
* Each package's tree under the vendor directory is replaced whole. The import map and `.fsr-vendor.json` record every specifier with `published` set.
* A package reached only through another's bare import records that package in `dependency_of`. One a spec names clears it. One already recorded with it empty keeps it empty.
* `pub fn published::recorded(app: &Path, specs: &[Spec]) -> Result<Option<Vec<Spec>>, BuildError>`: `specs` widened to every specifier the vendor manifest records for a package it holds as published, at the version asked for. `None` when no spec names such a package.
* `pub fn published::compiler(app: &Path, name: &str, version: &str, file: &str) -> Result<PathBuf, BuildError>`: `file` of `name@version` at `<app>/.fsr/compilers/<name>/<version>/<file name>`, fetched and checked the first time.
* A `version` that does not parse is refused.
* `pub const published::COMPILERS: &str = ".fsr/compilers"`.
* `pub fn published::imports_of(text: &str) -> Result<Vec<(Range<usize>, String)>, String>`: the specifier of every static import, dynamic `import("…")` and re-export, with the byte range between its quotes, in source order. Read with oxc's parser.
* `Err` with the parser's message when the text is not a module.

### Direction

* `pub struct direction::Direction { pub name: &'static str, pub maps: &'static [(&'static str, &'static str)], pub vendors: &'static [(&'static str, &'static str, Option<&'static str>)], pub published: bool, pub edits: &'static [(&'static str, &'static str)] }`, `#[non_exhaustive]`.
* `maps` pairs each import map specifier with its file under the client route. `vendors` is the package, version and subpath of each module. `published` vendors the package as published. `edits` is each file and line an existing application edits by hand.
* `pub const direction::DIRECTIONS: &[Direction]`.
* `pub fn direction::find(name: &str) -> Result<&'static Direction, BuildError>`: `BuildError::Direction` on a name outside the table.
* `Direction::lines(&self) -> Vec<(String, String)>`: each import map line, the specifier and its URL under `snapfire_fsr_host::client::ROUTE`.
* `Direction::specs(&self) -> Vec<Spec>`.
* `direction::with_companions(specs: Vec<Spec>) -> (Vec<Spec>, Vec<Spec>)`: `specs` with every module a framework's adapter needs added at the version asked for. Returns the specs vendored one bundle each, then those vendored as published.
* `pub const direction::REACT: &str = "19.3.1"`, `VUE = "3.5.44"`, `SVELTE = "5.57.2"`, `HTMX = "4.0.0"`.
* `direction::TAILWIND_SHEET`: `"tailwind.css"`. `direction::CODE_SHEET`: `"code.css"`. `direction::CODE_THEMES`: `("GitHub", "OneHalfDark")`.
* `direction::code_sheet() -> Option<String>`.

| Name | Maps | Vendors | Edits |
| --- | --- | --- | --- |
| `react` | `@snapfire/fsr/client/react` at `react.js`, `@snapfire/fsr/template` at `template.js` | `react`, `react/jsx-runtime`, `react-dom/client` at `REACT` | nothing |
| `vue` | `@snapfire/fsr/client/vue` at `vue.js` | `vue` at `VUE` | nothing |
| `svelte` | `@snapfire/fsr/client/svelte` at `svelte.js` | `svelte` and twelve subpaths at `SVELTE`, as published | nothing |
| `elements` | `@snapfire/fsr/client/elements` at `elements.js` | nothing | nothing |
| `htmx` | `@snapfire/fsr/client/htmx` at `htmx.js` | `htmx.org` at `HTMX` | `src/main.ts`: the htmx import, the `bindHtmx` import and `bindHtmx(htmx)` after `enableNavigation()` |
| `tailwind` | nothing | nothing | nothing |
| `tera` | nothing | nothing | nothing; feature `tera` only |
| `mdx` | nothing | nothing | nothing; feature `mdx` only |

### adopt

* `pub fn direction::adopt(app: &Path, names: &[String], options: UseOptions) -> Result<Adopted, BuildError>`.
* `pub struct direction::UseOptions { pub fetch: bool, pub example: bool }`, `#[non_exhaustive]`. `Default` has `fetch` true and `example` false.
* Setters `UseOptions::fetch(self, bool) -> Self` and `UseOptions::example(self, bool) -> Self`.
* Resolves every name first. An unknown one is refused before anything is written.
* With `example`, refuses when an example file is already there.
* Per direction, writes the map line through `read_import_map` and `write_import_map`. An entry already at the host's URL is `present`. One at another URL is `BuildError::AdapterUrl`.
* Per direction, checks the specs against `VendorManifest`. A package recorded at the pinned version is `kept`. One at another version is `BuildError::DirectionPinned`.
* With `fetch`, runs `vendor::add` over what is left to vendor, then `types::fetch` and the generation `fsr new` runs. Each failure is a note rather than a stop.
* Without `fetch`, names `fsr add`, `fsr types` and `fsr build` in `next`.
* Last come the direction's edits and the examples' placements.
* `pub struct direction::Adopted { pub mapped: Vec<(String, String)>, pub present: Vec<String>, pub vendored: Vec<(String, String, usize)>, pub kept: Vec<String>, pub from_shell: Vec<(String, String)>, pub delegated: Vec<String>, pub typed: Vec<(String, String, String)>, pub pinned: Option<(PathBuf, String)>, pub written: Vec<PathBuf>, pub notes: Vec<String>, pub edits: Vec<(String, String)>, pub next: Vec<String> }`, `Default`, `#[non_exhaustive]`.

The examples `--example` writes:

| Direction | Files |
| --- | --- |
| `react` | `src/ui/Counter.tsx` |
| `vue` | `src/ui/Counter.vue` |
| `elements` | `elements/hello-tag.tsx`, `src/elements/hello-tag.ts` |
| `htmx` | `routes/pulse/page.loader.ts`, `routes/pulse/page.tsx` |
| `tera` | `routes/hello/page.loader.ts`, `routes/hello/page.tera` |
| `tailwind` | `routes/styled/page.tsx`, `styles/themes/ocean.css` |
| `mdx` | `routes/notes/page.mdx` |

* No example is placed into an existing page.

### types::fetch

* `pub fn types::fetch(app: &Path, refresh: bool) -> Result<TypesReport, BuildError>`.
* The queue is `@snapfire/fsr/authoring` (`types::AUTHORING_PACKAGE`), `@snapfire/fsr/client` (`types::CLIENT_PACKAGE`), then `react` for a terminal app, then `import_map_packages`.
* The fsr packages are written from declarations embedded in the binary. Any other `@snapfire/*` is `missing`.
* For a terminal app it also writes the declarations of FSR's terminal modules under `<types>/@snapfire/fsr/terminal/` (`types::TERMINAL_TYPES`).
* The terminal `index.d.ts` is declared from the signatures of the `terminal` members of `snapfire_fsr_ir::STANDARD`.
* The terminal `core.d.ts` holds `snapfire_fsr_terminal::prop_declarations`, `Key` from `terminal::KEYS` and the signatures of `snapfire_fsr_lower::meaning::terminal_primitives`. `ink.d.ts` re-exports Ink's hooks and those primitives.
* The terminal `hooks.d.ts`, `spinner.d.ts`, `spinners.d.ts` and `text-input.d.ts` are `types::declarations_of` `terminal::INK_SOURCE` and `terminal::COMPANIONS`.
* `types::declarations_of(sources: &[(String, &str)]) -> Result<Vec<(String, String)>, BuildError>`: runs `snapfirec --declaration`. `BuildError::Types` naming the compiler when it cannot.
* `.written-from` beside the terminal declarations holds a hash of their sources. A build writes them again only when it changed. `fsr types --refresh` always does.
* `types::refresh_embedded(app: &Path) -> Result<Vec<PathBuf>, BuildError>`: writes the embedded declarations again when they changed.
* A package whose entry is on disk is kept unless `refresh` is set or a range asked of it refuses the version it holds.
* Under xwpm, `xwpm restore` and `xwpm types` run first. Every other package is `missing` with that reason.
* Otherwise packages come from the npm registry.
* Each package a kept or fetched one's declarations need is queued with the range it asks.
* From a DefinitelyTyped package, every `dependencies` entry is queued under the package name it types. From a package with its own declarations, the `dependencies` they import by bare specifier are queued.
* An import map package is first asked for the vendored version's major, else the one a site's shell names.
* The abbreviated packument chooses the highest release every range asked of the package accepts, else the highest the first accepts, else `latest`.
* The entry is the version document's `types`, `typings`, the `types` condition of `exports["."]`, a `.d.ts` beside `main` or `index.d.ts`, the first the tarball holds.
* Its `.d.ts`, `.d.mts`, `.d.cts` and `package.json` files are unpacked under `<types>/<package>/`.
* Without an entry, `@types/<name>` is tried the same way, as `@types/scope__name` for `@scope/name`. It is tried first when the first range asked is a range of it.
* A package with neither is `missing`.
* A package fetched again for a later range has its needs read again, at most three times per package.
* `pub struct TypesReport { pub fetched: Vec<(String, String, String)>, pub kept: Vec<String>, pub missing: Vec<(String, String)>, pub unmet: Vec<(String, String, Asked)>, pub delegated: Vec<String>, pub written: Vec<String>, pub pinned: Option<(PathBuf, String)> }`.
* `fetched`: package, version and source. `missing`: package and reason. `unmet`: package, the version it holds and the range that refuses it. `delegated`: xwpm commands run. `written`: the shim and the tsconfig, relative to the app. `pinned`: the configuration file and the TypeScript version recorded in it.
* `pub struct types::Asked { pub by: String, pub of: Option<String>, pub range: String }`, `#[non_exhaustive]`: a range asked of a package's declarations, by a package, by `the vendored <version>` or by `the shell`. `of` is the npm package it is a range of, `None` for whichever supplies the declarations.
* `types::NPM_REGISTRY`: `"https://registry.npmjs.org"`.

### Declarations

* `pub struct types::Declarations`: one reading of `types/` against what the application needs.
* `Declarations::read(app: &Path) -> Result<Declarations, BuildError>`. `missing(&self) -> Vec<String>`, `unmet(&self) -> Vec<(String, String, Asked)>` and `status(&self) -> Vec<(String, String)>` answer from it.
* `types::missing(app: &Path) -> Result<Vec<String>, BuildError>`: the packages the build needs and `types/` lacks: the fsr packages, every import map package and what the declarations on disk import, followed through.
* `types::unmet(app: &Path) -> Result<Vec<(String, String, Asked)>, BuildError>`: each package on disk whose version a range asked of it refuses. A range that does not parse, a dist-tag or a URL, refuses nothing.
* `types::status(app: &Path) -> Result<Vec<(String, String)>, BuildError>`: a report row per package `missing` walks: `<types>/<package>  <from> <version>`, `<types>/<package>` when unrecorded or `missing; run fsr types`. Each is followed by `; <by> asks <range>` for a range it does not meet.
* `types::present(app: &Path, layout: &Layout) -> Result<Vec<(String, TypedPackage)>, BuildError>`: every package directory under the types directory, scoped ones included. The entry is the manifest's record, else `index.d.ts`.
* `types::declared_packages(app: &Path, layout: &Layout) -> Result<Vec<(String, TypedPackage)>, BuildError>`: `present` plus `@snapfire/fsr/client` and `@snapfire/fsr/authoring`.
* `@snapfire/fsr/client` holds the client's modules with the host and terminal standard modules' declarations under its `host/` and `terminal/`.
* `types::definitely_typed(package: &str) -> String`. `types::is_ambient(entry: &str) -> bool`: whether it contains `declare module "` or `declare module '`.
* `types::DESKTOP_PACKAGE`: `snapfire_fsr_host::client::DESKTOP_SPECIFIER`.

### Shims

* `pub const types::FOREIGN_SHIM: &str = "foreign.d.ts"`, under `<types>/`.
* `pub fn types::source_dirs(app: &Path) -> Vec<&'static str>`: the ones of `src`, `ext`, `elements`, `routes`, `schemas` and `tests` the application has.
* `pub fn types::foreign_shim(app: &Path, placed: &[String]) -> Option<String>`: `declare module "*.<ext>"` for the extension of every `.vue` file under the source directories and of every module in `placed`. `None` when there are none.
* In the shim, `vue` types the default export as a function of loose props. Any other extension is `unknown`.
* `pub fn types::write_foreign_shim(app: &Path, layout: &Layout, placed: &[String]) -> Result<Option<String>, BuildError>`: writes the shim to `<types>/foreign.d.ts` and answers the path relative to the app. With nothing foreign, removes a shim that is there and answers `None`.
* `pub const types::SHADOWED_DIR: &str = "generated/shadowed"`.
* `pub fn types::shadowed_components(app: &Path) -> Vec<(String, String)>`: each `.vue` or `.svelte` file under the source directories with a file beside it named for the component plus `.ts`, `.tsx`, `.d.ts`, `.js` or `.jsx`, compared without case. Paired with that file, both relative to the app, sorted.
* `pub fn types::shadowed_declaration_path(file: &str) -> String`: `generated/shadowed/<file>.d.ts`.
* `pub fn types::shadowed_declaration(file: &str) -> String`: its default export typed as the shim types the extension.

### tsconfig

* `pub fn types::tsconfig(app: &Path, generated: bool, shim: bool, declared: bool, shadowed: &[(String, String)]) -> Result<String, BuildError>`.
* `compilerOptions`: `target` and `lib` from `[build]` (`es2022` and no `lib` when absent), `module` esnext, `moduleResolution` bundler, `jsx` react-jsx, `strict`, `noEmit` and `skipLibCheck`.
* `jsxImportSource` is `@snapfire/fsr/authoring` when the import map has no `react` and the app is not a terminal app.
* `shadowed` pairs a component with the declaration it typechecks as. Every alias whose directory holds the component gets an exact `paths` entry to it.
* With `declared`: `allowArbitraryExtensions`, `rootDirs` of `.` and `generated/vue` and a second target under `generated/vue/` for each alias.
* `paths`: `@snapfire/fsr` to the client's `native.d.ts`, `@snapfire/fsr/host` to `./generated/host` and `@snapfire/fsr/host/*` to the client's `host/` declarations.
* `paths`, per present package: `<name>` to `./<types>/<name>/<entry>` unless ambient. Also `<name>/*` to `./<types>/<name>/*`.
* `paths`, for a terminal app: `@snapfire/fsr/terminal` to `./<types>/@snapfire/fsr/terminal/index.d.ts` and `@snapfire/fsr/terminal/*` to `./<types>/@snapfire/fsr/terminal/*`, with `@snapfire/fsr/terminal/system` and `/clipboard` to the client's `terminal/` declarations.
* `paths`: `@snapfire/fsr/template` to `./<types>/@snapfire/fsr/authoring/template.react` when the import map has `react`, typing `Children` as `ReactNode`. Without `react` it goes to `template`.
* `include`: `<dir>/**/*` for each of `source_dirs`, `generated/**/*` when `generated`, `<types>/foreign.d.ts` when `shim` and each ambient entry.
* The build passes `generated` as true. `fsr types` passes whether `generated/` exists.
* `pub fn types::tsconfig_build(app: &Path, files: &[String], generated: &[String], jsx: &str) -> String`.
* `tsconfig_build` writes `target` and `lib` as `tsconfig` does, `outDir` dist, `rootDir` `.`, `sourceMap`, `jsx` react-jsx and `jsxImportSource` `jsx`.
* `tsconfig_build` includes each of `files` not in `generated`, the stylesheets, `generated/islands.ts` and `generated/client.ts`. `files` lists each of `generated` when there is any.
* `pub fn types::element_declarations(app: &Path, layout: &Layout, elements: &[(String, String)]) -> Result<String, BuildError>`: `generated/elements.d.ts` for the tags and modules under `elements/`.
* Each entry is `Placed<typeof TemplateN>`: the template's props over `Host`, with any other attribute allowed.
* `Host` is React's `DetailedHTMLProps<HTMLAttributes<HTMLElement>, HTMLElement>` when the import map has `react`, else the dialect's `Attributes`.
* With `react`, the declarations augment `React.JSX.IntrinsicElements` and add a `` `${string}-${string}` `` entry typed `Omit<Host, Taken> & Loose`. `Taken` is the union of every template's `keyof Props`, `never` with no template.
* Without `react`, the declarations fill `ElementTemplates` in `@snapfire/fsr/template`. A dialect application with no template gets `export {};`.

### types::range

* `types::range::{Range, Version, Ident}`: npm's version ranges as node-semver reads them without `loose` or `includePrerelease`.
* `Range::parse(&str) -> Option<Range>`: takes `||`, hyphen ranges, `^`, `~`, `~>`, `<`, `<=`, `>`, `>=`, `=`, `x`, `X`, `*` and partial versions. `None` for a dist-tag, a URL, a path or an alias.
* `Range::satisfies(&self, &Version) -> bool`: a prerelease only when a comparator in the set names its `major.minor.patch` with a prerelease.
* `Version::parse(&str) -> Option<Version>`: `major.minor.patch[-pre][+build]` after an optional `v`. Versions order by semver precedence, build metadata ignored.

### Manifests

* `pub struct vendor::VendorManifest { pub packages: BTreeMap<String, VendoredPackage> }`: `<vendor>/.fsr-vendor.json`.
* `pub struct vendor::VendoredPackage { pub version: String, pub externals: Vec<String>, pub entries: BTreeMap<String, String>, pub published: bool, pub dependency_of: Vec<String> }`.
* `entries` maps a specifier to its file relative to the vendor directory. `published` marks a package vendored as published. `dependency_of` is the packages it was vendored for because they import it, empty when the application asked for it.
* `pub struct types::TypesManifest { pub packages: BTreeMap<String, TypedPackage> }`: `<types>/.fsr-types.json` (`types::TYPES_MANIFEST`).
* `pub struct types::TypedPackage { pub version: String, pub from: String, pub entry: String, pub ambient: bool }`, `#[non_exhaustive]`. `TypedPackage::new(version: String, from: String, entry: String, ambient: bool) -> TypedPackage`.
* `from` is the package, `@types/<name>`, `fsr` or `vendor`. `entry` is relative to `<types>/<name>/`. `ambient` marks an entry of `declare module` blocks, which is included rather than path-mapped.
* Both manifests: `read(app: &Path, layout: &Layout) -> Result<Self, BuildError>` and `write(&self, app: &Path, layout: &Layout) -> Result<(), BuildError>`. A missing file reads as empty.

## 9. Error Handling

### BuildError

`pub enum BuildError`, `#[non_exhaustive]`.

Files and configuration:

* `Io(PathBuf, std::io::Error)`
* `Configuration(String)`: the configuration does not load.
* `UnlistedLocales { named: Vec<String>, listed: Vec<String> }`: the code names locales `[locales]` does not cover and `unlisted = "error"` refuses that.
* `Manifest(PathBuf, String)`: a vendor manifest, types manifest, import map or `xwpm.wmf` that did not parse.
* `Doctor(doctor::Report)`: what `fsr doctor` found before a bundle. Displays as the report.

Routes and ids:

* `NoRoutes(PathBuf)`: no `routes/` directory under the app.
* `Segment { path: PathBuf, name: String }`: a directory name that is not a route segment.
* `PageAndRoute(PathBuf)`: a directory holding both a page and `route.ts`.
* `PageAndTemplate { dir: PathBuf, first: String, second: String }`: two page files or two layout files in one directory.
* `TemplateFeature(PathBuf)`: a template in an fsr built without `tera`. The `tera` feature is on by default.
* `TemplateIsland { file: PathBuf, line: usize }`: an `island(` call in a template whose `module` is not a literal.
* `ActionsWithoutPage(PathBuf)`: an `actions.ts` in a directory with no page.
* `SlotsWithoutLayout(PathBuf)`, `SlotWithoutPage(PathBuf)` and `SlotRoute(PathBuf)`.
* `SlotUndeclared { path: PathBuf, file: String, slot: String }`: a `page.<slot>.tsx` naming a slot no layout above declares.
* `ClaimedId { kind: String, id: String, first: String, second: String }`: two rows deriving one id.
* `ClaimedPattern { pattern: String, first: String, second: String }`: two routes answering the same URLs.
* `PublicClaimed { file: String, path: String, by: String }`: a file under the root static root, `public/` by default, at a URL a route without parameters, a generated card image, `[crawl]` `robots` or `sitemap` or the icons root's `favicon.ico` also answers. `by` names which.
* `PathsOffPage(String)`: a `layout.loader.ts` or a slot's `page.loader.ts` exports `paths`.
* `PathsWithoutParameter { module: String, pattern: String }`: a page loader exports `paths` on a route with no parameter.
* `MetaTwice { module: String, page: String }`: a `page.loader.ts` and the `page.mdx` beside it both export `meta`.
* `InlineScript(String)`: a head `script` in `meta` carrying inline code.
* `SocialImage(PathBuf, String)`: a card image the build cannot place, with why.

Contracts and lowering:

* `Import { document: String, error: ImportError }`
* `DuplicateType { name: String, first: String, second: String }`
* `Contract(ContractError)`: from `Contract::validate`.
* `UnknownInput { action: String, name: String }`: an action input type no schema declares.
* `UnknownHandlerInput { handler: String, name: String }`: a handler input type no schema declares.
* `Lower(LowerError)`: transparent, from `snapfire_fsr_lower`.
* `DerivedTwice { key: String, first: String, second: String }` and `DeriveCycle { keys: Vec<String> }`.
* `ServerIsland { module: String, reason: String }`: a component that cannot be an island in server mode.
* `HostInBrowser { file: String, line: u32 }`: a file the browser loads importing `@snapfire/fsr/host`.
* `Mdx { file: String, message: String }`: a content file of a collection that does not convert.
* `MdxFeature { what: String, feature: &'static str }`: an `.mdx` or `.md` page, `[mdx.collections]`, `[mdx] frontmatter = true` or `[mdx] highlight = true` in an fsr built without `mdx`, `mdx-frontmatter` or `mdx-highlight`.

Components and islands:

* `NoAdapter { module: String, ext: String }`: an island whose extension a plugin compiles and no client adapter mounts.
* `UnknownComponent { module: String }`: an island whose extension no framework claims.
* `NoJsxFramework { module: String, at: String, reason: String }`: a component that did not lower in an application with no JSX framework.
* `Unrendered { file: String, root: String, reason: &'static str }`: a file a browser root reaches that writes JSX or imports the template dialect, in an application with no JSX framework.
* `ShadowedImport { file: String, specifier: String, module: String, alias: String }`: a relative import TypeScript resolves to the module beside a component rather than the component.
* `IslandImports { module: String, adapter: String, missing: String, remedy: String }`: the first registered module whose adapter the import map cannot supply, with the specifiers it lacks.
* `IslandImports` also covers a module importing a value from `@snapfire/fsr/template` with that specifier unmapped. `remedy` names the `fsr use` direction that writes the line, empty when no direction maps it.
* `Plugin(String)`: a framework plugin that started and then failed a describe batch.
* `ElementName { file: String }`: a file under `elements/` whose name is not a custom element tag.
* `ElementTemplate { module: String, reason: String }`: an element template with state or a handler. Also a root `<template>` that cannot be its shadow root.
* `Strict(String)`: what `Options::strict` refused, one line per `client` or `foreign` cause, then each `plugins` row.

Terminal programs:

* `BrowserOnlyInTerminal { placed: String, why: String }`: a component of a terminal app that does not lower.
* `TerminalUndeclared { file: String }`: a file drawing with FSR's Ink in an app without `runs = "terminal"`.
* `TerminalFeature(PathBuf)`: a root page drawing with Ink in an fsr built without `terminal`.

Themes:

* `ReservedTheme { file: String }`: a `styles/themes/default.css`.
* `ThemeToken { file: String, token: String, defined: String }`: a named theme setting a token the root theme does not define, nor for a site its shell's root.
* `ThemeLayer { file: String }`: named themes with the root theme outside `theme.root`.
* `UnknownTheme { module: String, name: String, known: String }`: a layout or page naming a theme no file under `styles/themes/` defines, its own or the shell's.

Directions and frameworks:

* `Direction { name: String, known: String }`: an `fsr use` or `--with` name outside the direction table.
* `AdapterUrl { map: String, specifier: String, found: String, want: String }`: an import map carrying the adapter's specifier at a URL other than the host's.
* `DirectionPinned { direction: String, package: String, recorded: String, wanted: String, manifest: String }`: a vendor manifest recording a framework package at a version other than the direction's pin.
* `ExampleExists(PathBuf)`: an `--example` file already present.
* `FrameworkUnrecorded { package: String, specifier: String, manifest: String, app: String, version: String }`: an import map serving a framework package with no vendor manifest entry. The message names the `fsr add` that records it.
* `FrameworkShellUnrecorded { package: String, specifier: String, contract: String, version: String }`: a site whose shell serves a framework package with no `frameworks` entry.
* `FrameworkUnchecked { package: String, version: String, floor: String, add: String }`: a vendored React, Vue or Svelte older than `floor`, the earliest release of its major the renderer was checked against. `add` is the `fsr add` that moves it.
* `ReactMajor { version: String, supported: String }`, `VueMajor { version: String, supported: String }` and `SvelteMajor { version: String, supported: String }`: a vendored framework major the renderer or adapter does not support.
* `ShellUrl { map: String, specifier: String, found: String, want: String, contract: String }`: a site mapping a framework specifier somewhere other than its shell's URL.
* `ShellPinned { specifier: String, package: String, wanted: String, shell: String, contract: String }`: an `fsr add` pinning a version of a package the site's shell serves.

Vendoring:

* `Spec(String)`: an `fsr add` argument that is not `name@version[/subpath]`. Also an `fsr use` direction version that does not parse.
* `PackageName { name: String, by: String }`: a name that is not an npm package name.
* `Dependency { package: String, wants: String }`: a vendored module importing a package outside its bundle.
* `VendorUrl { map: String, specifier: String, found: String, base: String, want: String }`: an import map entry for a vendored package that is not under the layout's base, with the URL it should carry.
* `VendorImports { dir: String, file: String, specifier: String, map: String }`: a vendored `.js` or `.mjs` file importing a bare specifier the import map, its scopes or the shell's contract cannot resolve.
* `Http(String, String)`: the URL and the failure.
* `Xwpm(String)`: an `xwpm` command that could not start or failed.

Tools and commands:

* `Dev(String)`: the dev loop, a compiler that could not start or failed, a `[build]` program or an unreadable shell contract.
* `Tool(String)`: a compiler older than `install::COMPILER_FLOOR`.
* `Types(String)`: declarations still missing. Also `snapfirec --declaration` failing.
* `Typecheck(String)`: the checker could not be read. Also a check that found an error. The row comes first.
* `Assets(String)`: an image or a font the build could not derive. Also a `[fonts]` key it could not place. Displays with `assets: ` before it.
* `Bundle(String)`: displays with `bundle: ` before it.
* `Serve(String)`: the stock host refusing to build or the listener failing.
* `Sites(String)`: an `fsr sites` command failing.

### DoctorError

* `pub enum doctor::DoctorError { Io(PathBuf, std::io::Error), Config(String), Plan(PathBuf, String) }`, `#[non_exhaustive]`.

### IrError

* `pub enum ir::IrError { NoPlan(PathBuf), Plan(PathBuf, String), Unknown { why: ViewError, rows: Vec<String> }, View(ViewError) }`, `#[non_exhaustive]`, over `snapfire_fsr_plan::sexpr::ViewError`.
* `NoPlan` names the missing plan and says to run `fsr build` first. `Unknown` lists every row.
