# API Reference: snapfire_fsr_sites

Resolves, hashes, installs, mounts and watches the `[sites]` table of a shell's configuration on `snapfire_fsr_host`.

## Contents

* [1. Resolving](#1-resolving)
  * [`Resolved`](#resolved)
  * [`resolve`](#resolve)
  * [`hash_dir`](#hash_dir)
* [2. The Deploy Tree](#2-the-deploy-tree)
  * [`Layout`](#layout)
  * [`Placement`](#placement)
  * [`Source`](#source)
  * [`Row`](#row)
* [3. Artifacts](#3-artifacts)
  * [`Listing`](#listing)
  * [`Entry`](#entry)
  * [`Manifest`](#manifest)
  * [`pack`](#pack)
  * [`unpack`](#unpack)
* [4. Installing](#4-installing)
  * [`Store`](#store)
  * [`TarStore`](#tarstore)
  * [`ArchiveStore`](#archivestore)
  * [`HttpStore`](#httpstore)
  * [`Cache`](#cache)
  * [`Installed`](#installed)
  * [`store_of`](#store_of)
  * [`fetch_missing`](#fetch_missing)
* [5. Mounting](#5-mounting)
  * [`mount_all`](#mount_all)
  * [`mount_all_with`](#mount_all_with)
  * [`mountable`](#mountable)
  * [`mountable_with`](#mountable_with)
* [6. Watching](#6-watching)
  * [`watch`](#watch)
  * [`poll_of`](#poll_of)
* [7. Serving](#7-serving)
  * [`serve::project_root`](#serveproject_root)
  * [`serve::host_of`](#servehost_of)
  * [`serve::builder_with_mounts`](#servebuilder_with_mounts)
  * [`serve::listen`](#servelisten)
  * [`fsr-server`](#fsr-server)
* [8. Error Handling](#8-error-handling)
  * [`SitesError`](#siteserror)
  * [`InstallError`](#installerror)
  * [`LayoutError`](#layouterror)
  * [`ArtifactError`](#artifacterror)
  * [`ServeError`](#serveerror)

## 1. Resolving

### Resolved

One row of the `[sites]` table, resolved.

* `pub struct Resolved { pub name: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub allow_engine: bool }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.

### resolve

* `resolve(config: &Config) -> Result<Vec<Resolved>, SitesError>`: every `[sites.<name>]` row that names an `artifact`, in name order. Empty when there is no `[sites]` section.
* An `artifact` holding `@` and no `/` is `name@version` and resolves to `<sites.root>/<name>/<version>`. Any other is a path against `config.root`, with version `path`.
* `Artifact` when a row is not a directory, cannot be hashed or hashes differently from a pinned `hash`.

### hash_dir

* `hash_dir(dir: &Path) -> Result<String, ArtifactError>`: `Listing::of(dir)?.hash()`. Sixteen hex digits.

## 2. The Deploy Tree

Constants in `layout`: `CONFIG = "config"`, `APP = "app"`, `SERVE = "serve"`, `GENERATED = "generated"`, `LAYER = "bundle.toml"`.

### Layout

Where a deploy tree puts each file it ships.

* `pub struct Layout { pub places: Vec<Placement> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.
* `layout(root: &Path, config: &Config) -> Result<Layout, LayoutError>`: the placements `config` implies, in destination order. `NoConfig` when `config.sources` is empty.
* `parts(root: &Path, config: &Config) -> Result<Vec<String>, ArtifactError>`: `layout(root, config)?.parts()`.
* `rows(&self) -> Result<Vec<Row>, LayoutError>`: every file the tree holds, in path order. A placement whose source is absent contributes nothing.
* `parts(&self) -> Vec<String>`: the destinations, as paths in the tree.

Destinations:

* `config/`: the whole configuration directory when the project has one apart from `root`, else each of `config.sources` by file name.
* Overlays: with the whole directory, every other `.toml` or `.yaml` in it except `app` and `bundle` is read as an overlay and the files it names are placed too. `Overlay` when one does not load.
* `config/bundle.toml`: generated, unless the configuration directory already holds one. It sets `[app] dir = "app"`, `[server] plan`, `contracts`, `prerender` and `dev = false`, `[document] import_map`, `entry` and `styles`, each `[clients.<name>] document` and `responses` and every `[[static]]` route against `../serve/<route>`.
* `app/generated/<file name>`: the plan, `plan.sexp` when `server.plan` has no file name.
* `app/generated/contracts`: the contracts.
* `app/generated/prerender`: `server.prerender`.
* `app/<file name>`: the import map.
* `app/generated/assets.json`, `app/generated/tailwind.cards.json`, `app/dist/.snapfire-build.json` and `app/locales/`: when present.
* `app/<path>`: every `.tera` template under the application directory.
* `app/clients/<name>.openapi.json` or `app/clients/<name>.proto`: each `[clients.<name>]` document. `<name>.mock.json` beside it for a mock's recorded responses.
* `serve/<route>`: each `[[static]]` root, slashes trimmed.
* `serve/<bundle route>/<hashed name>`: each `[bundle]` output under its content-hashed name.
* `serve/static/js/fsr/<file>`: the FSR client modules, when `config` has no `[site]` and no `[[static]]` root on `/static/js/fsr`. Minified files are included when `document.client` asks for them.

### Placement

One file or directory of a layout.

* `pub struct Placement { pub to: String, pub from: Source, pub required: bool }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.
* `required` is true for the configuration, the plan, the import map, each template and each client's document and responses. The host refuses to start without them.
* `path(&self) -> Option<&Path>`: the source, when it comes from the project.
* `exists(&self) -> bool`: whether the source is there. Always true for `Source::Text`.

### Source

* `pub enum Source { Path(PathBuf), Text(String) }`: a file in the project or text the layout generates. `Debug`, `Clone`, `PartialEq`, `Eq`.

### Row

One file of a tree.

* `pub struct Row { pub path: String, pub from: Source }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `#[non_exhaustive]`.
* `bytes(&self) -> Result<Vec<u8>, LayoutError>`: the file's content.

## 3. Artifacts

### Listing

Every file an artifact ships, in path order.

* `pub struct Listing { pub entries: Vec<Entry> }`. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `#[non_exhaustive]`.
* `Listing::of(dir: &Path) -> Result<Listing, ArtifactError>`
* `Listing::of_config(root: &Path, config: &Config) -> Result<Listing, ArtifactError>`
* `Listing::of_rows(rows: &[Row]) -> Result<Listing, ArtifactError>`
* `hash(&self) -> String`: xxh3 over each entry's path, size and digest, in path order. Sixteen hex digits. It covers the listing, not the bytes.
* `bytes(&self) -> u64`: the total size.

### Entry

* `pub struct Entry { pub path: String, pub size: u64, pub sha256: String }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `#[non_exhaustive]`.

### Manifest

The `.snapfire-site.json` at an artifact's root. Constants in `artifact`: `MANIFEST = ".snapfire-site.json"`, `FORMAT = 2`.

* `pub struct Manifest { pub format: u32, pub name: String, pub version: String, pub at: String, pub hash: String, pub files: Vec<Entry> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `#[non_exhaustive]`.
* `name` is the site's `[site] name` and `at` is its `[site] at`.
* `Manifest::of(dir: &Path, version: &str) -> Result<Manifest, ArtifactError>`
* `Manifest::of_rows(dir: &Path, version: &str, config: &Config, rows: &[Row]) -> Result<Manifest, ArtifactError>`: `NotASite` when `config` has no `[site]`.
* `Manifest::path(dir: &Path) -> PathBuf`
* `Manifest::read(dir: &Path) -> Result<Option<Manifest>, ArtifactError>`: `None` when `dir` has no manifest.
* `Manifest::read_archive(archive: &Path) -> Result<Manifest, ArtifactError>`: reads the manifest from a gzipped tar without unpacking it.
* `write(&self, dir: &Path) -> Result<(), ArtifactError>`
* `verify(&self, dir: &Path) -> Result<(), ArtifactError>`: every listed file is present with its listed digest, no unlisted file is present and the listing hashes to `hash`.

### pack

* `pack(dir: &Path, version: &str, out: &Path) -> Result<Manifest, ArtifactError>`: writes the artifact at `dir` to `out` as a gzipped tar laid out as a deploy tree, with its manifest at the archive root. Entries carry no timestamp and no owner. Packing the same tree twice gives the same bytes.

### unpack

* `unpack(archive: &Path, into: &Path) -> Result<Manifest, ArtifactError>`: `into` must not exist. The manifest is returned unverified.
* Refuses an absolute entry, an entry containing `..` and an entry that is not a regular file, before writing anything.

## 4. Installing

Feature `http` adds `HttpStore`. Constants in `install`: `STAGING = ".staging"` is the staging directory under a cache root. `LOCK = ".lock"` is the cache's lock file. At the crate root: `STORE_HEADER_ENV = "FSR_SITES_STORE_HEADER"`.

### Store

Where a version's bytes come from.

* `pub trait Store: Send + Sync`
* `fn describe(&self) -> String`: the store's name in reports and errors.
* `fn fetch(&self, package: &str, version: &str, into: &Path) -> Result<Manifest, InstallError>`: writes the artifact into `into`, an empty directory, with its manifest at the root. The cache verifies the result.

### TarStore

A directory of `<package>-<version>.tar.gz` files.

* `pub struct TarStore { pub dir: PathBuf }`. `Debug`, `Clone`, `#[non_exhaustive]`.
* `TarStore::new(dir: impl Into<PathBuf>) -> TarStore`
* `archive(&self, package: &str, version: &str) -> PathBuf`
* `archive` is `<dir>/<package>-<version>.tar.gz`.
* `fetch` returns `Absent` when the file is not there.

### ArchiveStore

One archive, answered for whatever version is asked.

* `pub struct ArchiveStore { pub archive: PathBuf }`. `Debug`, `Clone`, `#[non_exhaustive]`.
* `ArchiveStore::new(archive: PathBuf) -> ArchiveStore`

### HttpStore

`GET <base>/<package>-<version>.tar.gz` with every header. Feature `http`.

* `pub struct HttpStore { pub base: String, pub headers: Vec<(String, String)> }`. `Debug`, `Clone`, `#[non_exhaustive]`.
* `HttpStore::new(base: impl Into<String>) -> HttpStore`: trims a trailing slash.
* `header(self, name: impl Into<String>, value: impl Into<String>) -> HttpStore`
* `url(&self, package: &str, version: &str) -> String`
* `fetch` returns `Absent` on a 404 and `Fetch` on any other failure.

### Cache

Installed versions at `<root>/<name>/<version>`.

* `pub struct Cache { pub root: PathBuf }`. `Debug`, `Clone`, `#[non_exhaustive]`.
* `Cache::new(root: impl Into<PathBuf>) -> Cache`
* `path(&self, name: &str, version: &str) -> PathBuf`
* `holds(&self, name: &str, version: &str) -> bool`
* `versions(&self, name: &str) -> Vec<String>`: oldest install first.
* `install(&self, store: &dyn Store, name: &str, package: &str, version: &str, keep: Option<usize>) -> Result<Installed, InstallError>`: installs `package` at `version` from `store` as `name`. A held version returns with `held: true` and is not fetched. Otherwise the store fills a staging directory, the manifest is verified, the directory is renamed into place and `keep` sweeps. `Mismatch` when the manifest names another version.
* `sweep(&self, name: &str, keep: usize, hold: &[String]) -> Result<Vec<String>, InstallError>`: removes versions past the `keep` most recent, never one in `hold`. Returns what it removed.
* `sweep_staging(&self) -> Result<usize, InstallError>`: removes every staging directory. Only safe when nothing else is installing.
* `install`, `sweep` and `sweep_staging` each hold an exclusive lock on `<root>/.lock`.

### Installed

* `pub struct Installed { pub name: String, pub version: String, pub hash: String, pub path: PathBuf, pub held: bool, pub swept: Vec<String> }`. `Debug`, `Clone`, `#[non_exhaustive]`.
* `name` is the name the cache holds it under. `held` is true when nothing was fetched. `swept` lists the versions the sweep removed.

### store_of

* `store_of(config: &Config) -> Result<Option<Box<dyn Store>>, SitesError>`: `sites.store` as a store. `None` without a `sites.store`.
* An `http://` or `https://` URL is an `HttpStore`. It carries the `Name: Value` header in `FSR_SITES_STORE_HEADER` when that is set. A URL without the `http` feature is `Artifact` naming `store`. A header that is not `Name: Value` is also `Artifact` naming `store`.
* Anything else is a `TarStore` against `config.root`.

### fetch_missing

* `fetch_missing(config: &Config, store: &dyn Store) -> Result<Vec<Installed>, SitesError>`: installs every `name@version` row the cache under `sites.root` lacks, with no sweep. Skips path rows and held versions. Empty without a `sites.root`. Does not check a pinned hash. A failure is `Artifact` naming the row.

## 5. Mounting

### mount_all

* `mount_all(builder: HostBuilder) -> Result<HostBuilder, SitesError>`: runs `fetch_missing` against `store_of` when the configuration names a store, then `resolve`. Mounts each site with `Loader::mount` on the builder's loader (`Loader::at` when it has none), `load` and `HostBuilder::mount`.

### mount_all_with

* `mount_all_with(builder: HostBuilder, store: &dyn Store) -> Result<HostBuilder, SitesError>`: `mount_all`, fetching from `store`.

### mountable

* `mountable(builder: HostBuilder) -> HostBuilder`: installs `mount_all` as the builder's sites mounter. `Host::reload`, `Host::reload_sites` and `POST /__fsr/sites/reload` then mount again.
* `POST /__fsr/sites/reload` needs feature `reload_route`.

### mountable_with

* `mountable_with(builder: HostBuilder, store: Arc<dyn Store>) -> HostBuilder`: `mountable` over `mount_all_with(store)`.

## 6. Watching

### watch

* `watch(host: Arc<Host>, root: PathBuf, poll: Option<Duration>)`: spawns onto the current tokio runtime. Calls `Host::reload_checked` on every `SIGHUP` (unix).
* With `poll`, also rereads the configuration at `root` every `poll` and calls `Host::reload_checked` when a row's name, path, version or hash changed.
* Logs under `fsr::sites`. A refused reload leaves the host unchanged.

### poll_of

* `poll_of(config: &Config) -> Option<Duration>`: `sites.poll`, parsed.

## 7. Serving

The stock host over an application with no crate of its own, in module `serve`. The crate root's `VERSION` is this crate's version.

### serve::project_root

* `project_root(app: &Path) -> PathBuf`: the directory beside `app` when it holds `config/`, `app.toml` or `app.yaml`, else `app`.

### serve::host_of

* `host_of(app: &Path, serve: bool, mounts: &[(String, PathBuf)]) -> Result<(Host, Option<Traces>, Option<LoggingGuard>), ServeError>`: the host over `app` with its `[sites]` table mounted.
* Refuses a configuration whose `[app] dir` names another directory.
* With `serve`, sets the process's logging from the project's `fibre_logging.yaml`, else warnings and errors to stderr.
* With `serve`, collects traces under `server.dev` or when the configuration exposes or inlines them. Without it, both `Option`s are `None`.

### serve::builder_with_mounts

* `builder_with_mounts(root: &Path, mounts: &[(String, PathBuf)]) -> Result<HostBuilder, ServeError>`: the builder, with `mounts` replacing or adding `[sites]` rows on every reload.

### serve::listen

* `async listen(host: Arc<Host>, root: PathBuf, listen: &str, bound: impl FnOnce(&str) -> io::Result<()>) -> io::Result<()>`: binds `listen`, passes the bound address to `bound`, watches the `[sites]` table under `root` and serves until the process ends.

### fsr-server

* `fsr-server [<app dir>] [--listen <addr>]`. Feature `server`, which also enables `http` and the host's `auth`, `oidc`, `passkey`, `tera`, `tls`, `grpc`, `graphql`, `http` and `share_cards`.
* `<app dir>` defaults to `app` beside the binary. `--listen` defaults to the configured listen address. `RELEASE_ENV` defaults to `production`.
* `--help` or `-h` prints the usage line.
* Exits with code 2 when `server.dev` is on, on an unknown flag or a second app directory and when the host fails to build or serve.

## 8. Error Handling

### SitesError

`#[non_exhaustive]`.

* `Host(HostError)`: transparent.
* `Artifact { name: String, message: String }`: displays as `sites.<name>: <message>`.
* `Content(ArtifactError)`: transparent.

### InstallError

`#[non_exhaustive]`.

* `Artifact(ArtifactError)`: transparent.
* `Io(PathBuf, std::io::Error)`
* `Held { name: String, version: String, path: PathBuf }`: `<name>@<version> is already installed at <path>`.
* `Absent { store: String, package: String, version: String }`: `<store> holds no <package> at <version>`.
* `Fetch(String, String)`: the URL and the failure.
* `Mismatch { name: String, version: String, package: String, wanted: String }`: the artifact is `name@version` and `package@wanted` was asked for.

### LayoutError

`#[non_exhaustive]`.

* `Escapes(String)`: a destination that is not a plain relative path.
* `Collides(String)`: a destination one configuration file places from two sources.
* `NoConfig`: the configuration was read from no file.
* `Io(PathBuf, std::io::Error)`
* `Layer(String)`: the generated `bundle.toml` failed to serialize.
* `Disagree(String, String, String)`: a destination two configuration files place from different sources, with both file names.
* `Overlay(String, String)`: an overlay under `config/` that does not load, with the reason.

### ArtifactError

`#[non_exhaustive]`.

* `Io(PathBuf, std::io::Error)`
* `Config(HostError)`: transparent.
* `NotASite(PathBuf)`: the configuration has no `[site]`.
* `Manifest { path: PathBuf, message: String }`
* `Format(PathBuf, u32)`: a manifest format this build does not read.
* `Archive(PathBuf, String)`
* `Layout(LayoutError)`: transparent.
* From `verify`: `Hash { found: String, declared: String }`, `Digest { path: String, found: String, declared: String }`, `Missing { path: String }`, `Extra { path: String }`.

### ServeError

* `pub struct ServeError(pub String)`: why `serve::host_of` or `serve::builder_with_mounts` could not build the host. `Debug`. Displays as the string.
