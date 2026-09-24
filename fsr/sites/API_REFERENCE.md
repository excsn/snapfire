# snapfire_fsr_sites API reference

`snapfire_fsr_sites`: the `[sites]` table of a shell's configuration resolved, hashed and mounted on `snapfire_fsr_host` and watched.

## Contents

* [1. Resolving](#1-resolving)
  * [`Resolved`](#resolved)
  * [`resolve`](#resolve)
  * [`hash_dir`](#hash_dir)
* [2. The Deploy Tree](#2-the-deploy-tree)
  * [`layout`](#layout)
  * [`Layout`](#layout-1)
  * [`Placement`](#placement)
  * [`Source`](#source)
  * [`Row`](#row)
  * [`parts`](#parts)
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
* [7. Error Handling](#7-error-handling)
  * [`SitesError`](#siteserror)
  * [`InstallError`](#installerror)
  * [`LayoutError`](#layouterror)
  * [`ArtifactError`](#artifacterror)

## 1. Resolving

### Resolved

* `pub struct Resolved { pub name: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub allow_engine: bool }`: one row of the table resolved. `Debug`, `Clone`, `PartialEq`, `Eq`.

### resolve

* `resolve(config: &Config) -> Result<Vec<Resolved>, SitesError>`: every `[sites.<name>]` row in name order; `name@version` under `sites.root` with that version, anything else a path against `config.root` with version `path`; each directory hashed and refused with `Artifact` when it is not a directory or its hash differs from a pinned `hash`. Empty without a `[sites]` section.

### hash_dir

* `hash_dir(dir: &Path) -> Result<String, ArtifactError>`: `Listing::of(dir)?.hash()`, the artifact hash of the tree at `dir`; sixteen hex digits.

## 2. The Deploy Tree

Where a deploy tree puts what it ships. A destination is derived from what a file is rather than from where it sat in the project, so no configured path is ever joined onto the tree root and no setting can place a file outside it. Applying a layout to a tree yields the tree, which is what lets one function serve a project about to be bundled and an artifact being verified.

Module constants: `CONFIG` is `"config"`, `APP` is `"app"`, `SERVE` is `"serve"`, `GENERATED` is `"generated"` and `LAYER` is `"bundle.toml"`.

### layout

* `layout(root: &Path, config: &Config) -> Result<Layout, LayoutError>`: the placements `config` implies, in destination order.
* Configuration: the whole `config/` directory when the project keeps one, else each of `config.sources` by file name, both under `config/`.
* Under `app/`: the plan as `generated/<file name>`; the contracts as `generated/contracts`; `server.prerender` as `generated/prerender`; the import map by its file name; `locales/` and `dist/.snapfire-build.json` when present.
* Under `app/clients/`, per `[clients.<name>]`: its document as `<name>.openapi.json` or `<name>.proto`, keeping the suffix the host reads to choose a transport, plus `<name>.mock.json` for a mock's recorded responses.
* Under `serve/`: each `[[static]]` root at its route with the slashes trimmed.
* `config/bundle.toml` as generated text. It names `[app] dir`, the moved `[server]` and `[document]` paths, each client's document and responses and every `[[static]]` route against `../serve/<route>`. It also carries the resolved `document.entry`, `styles` and `head`, since a tree has no `dist/`, `icons/` or `styles/` under its application directory for inference to read a second time.
* The layer is written only when the configuration directory holds no `bundle.toml` already, so a tree keeps the one it has.
* `Escapes` when a destination is not a plain relative path, which is how a route that climbs is refused; `Collides` when two sources want one destination; `NoConfig` when the configuration came from no file.

### Layout

* `pub struct Layout { pub places: Vec<Placement> }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `rows(&self) -> Result<Vec<Row>, LayoutError>`: every file the tree holds, in path order, directories walked and duplicates dropped. A placement whose source is absent contributes nothing.
* `parts(&self) -> Vec<String>`: the destinations.

### Placement

* `pub struct Placement { pub to: String, pub from: Source, pub required: bool }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `required` is whether the host refuses to start without it: the configuration, the plan, the import map and a client's document and responses. A contracts directory, a prerender cache and a message catalog are each read as whatever is there.
* `path(&self) -> Option<&Path>` is the source when it comes from the project; `exists(&self) -> bool` is whether it is there to be placed, always true for generated text.

### Source

* `pub enum Source { Path(PathBuf), Text(String) }`: something in the project or something the layout writes itself. `Debug`, `Clone`, `PartialEq`, `Eq`.

### Row

* `pub struct Row { pub path: String, pub from: Source }`: one file of a tree. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `bytes(&self) -> Result<Vec<u8>, LayoutError>`: its content, read or generated.

### parts

* `parts(root: &Path, config: &Config) -> Result<Vec<String>, ArtifactError>`: `layout(root, config)?.parts()`. Paths in the tree rather than in the project; for an artifact, which is a tree, they are also paths in the directory itself.

## 3. Artifacts

### Listing

* `pub struct Listing { pub entries: Vec<Entry> }`, every file an artifact ships in path order. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`.
* `Listing::of(dir)` and `Listing::of_config(root, config)` lay the directory out and list the rows; `Listing::of_rows(&[Row])` lists rows a caller already has, so a bundle hashes what it is about to write rather than walking it afterwards.
* `hash(&self) -> String`: xxh3 over each entry's path, size and digest in path order, sixteen hex digits. Over the listing rather than the bytes, so a manifest alone yields it and a pin can be checked before a download.
* `bytes(&self) -> u64`: the total.

### Entry

* `pub struct Entry { pub path: String, pub size: u64, pub sha256: String }`. `Serialize`, `Deserialize`.

### Manifest

* `pub struct Manifest { pub format: u32, pub name: String, pub version: String, pub at: String, pub hash: String, pub files: Vec<Entry> }`, written as `.snapfire-site.json` at an artifact's root. `MANIFEST` is that name and `FORMAT` is `2`.
* `Manifest::of(dir, version)` lays the tree out and hashes it; `Manifest::of_rows(dir, version, config, rows)` does it from rows a caller has. `read`, `read_archive`, `write`, `path`.
* `verify(&self, dir) -> Result<(), ArtifactError>`: every listed file present with the digest listed, nothing present and unlisted and the whole listing hashing to what the manifest declares. What an install checks before a staged directory is renamed into place.

### pack

* `pack(dir: &Path, version: &str, out: &Path) -> Result<Manifest, ArtifactError>`: the artifact at `dir` as a gzipped tar at `out`, laid out as a deploy tree, its manifest at the archive root. Entries carry no timestamp and no owner, so packing the same tree twice produces the same bytes.

### unpack

* `unpack(archive: &Path, into: &Path) -> Result<Manifest, ArtifactError>`: into a directory that must not exist, returning the manifest without verifying it. An entry that is absolute, that climbs out with `..` or that is not a regular file is refused before anything is written.

## 4. Installing

Feature `http` adds `HttpStore`. Module constant `STAGING` is `".staging"`, the directory under the cache root a fetch stages in; `STORE_HEADER_ENV` is `"FSR_SITES_STORE_HEADER"`.

### Store

* `pub trait Store: Send + Sync { fn describe(&self) -> String; fn fetch(&self, package: &str, version: &str, into: &Path) -> Result<Manifest, InstallError>; }`: where a version's bytes come from. `fetch` writes the artifact into `into`, an empty directory the cache owns, with its manifest at the root; the cache verifies what lands.

### TarStore

* `pub struct TarStore { pub dir: PathBuf }`: a directory of `<package>-<version>.tar.gz`. `new(dir)`, `archive(package, version) -> PathBuf`. `Absent` when the file is not there.

### ArchiveStore

* `pub struct ArchiveStore { pub archive: PathBuf }`: one archive as a store of a single version, whatever version is asked for.

### HttpStore

* `pub struct HttpStore { pub base: String, pub headers: Vec<(String, String)> }`, feature `http`: `GET <base>/<package>-<version>.tar.gz` with every header. `new(base)` trims a trailing slash, `header(name, value)` adds one, `url(package, version)` is the URL fetched. A 404 is `Absent`; any other failure is `Fetch`. The body streams to a file beside `into` that is removed after it is unpacked.

### Cache

* `pub struct Cache { pub root: PathBuf }`: `<root>/<name>/<version>` per installed version, where `name@version` resolves.
* `path(name, version)`, `holds(name, version)`, `versions(name)` oldest install first.
* `install(&self, store: &dyn Store, name, package, version, keep: Option<usize>) -> Result<Installed, InstallError>`: a held version is reported with `held: true` and not fetched; otherwise the store fills a staging directory, the manifest is verified and the directory renamed into place, then `keep` sweeps. `Mismatch` when the archive's manifest names another version.
* `sweep(name, keep, hold: &[String])` removes the oldest versions beyond `keep`, never one in `hold`; `sweep_staging()` removes what a dead fetch left.

### Installed

* `pub struct Installed { pub name: String, pub version: String, pub hash: String, pub path: PathBuf, pub held: bool, pub swept: Vec<String> }`.

### store_of

* `store_of(config: &Config) -> Result<Option<Box<dyn Store>>, SitesError>`: `sites.store` as a store. An `http://` or `https://` URL is an `HttpStore` carrying the `Name: Value` header in `FSR_SITES_STORE_HEADER` when it is set; anything else a `TarStore` against `config.root`. A URL without the `http` feature is `Artifact` naming `store`. `None` without a `sites.store`.

### fetch_missing

* `fetch_missing(config: &Config, store: &dyn Store) -> Result<Vec<Installed>, SitesError>`: every `name@version` row whose version the cache under `sites.root` lacks, installed from `store` as `name`. Path rows and held versions are skipped; a pinned hash is left for `resolve`. A failure is `Artifact` naming the row.

## 5. Mounting

### mount_all

* `mount_all(builder: HostBuilder) -> Result<HostBuilder, SitesError>`: `fetch_missing` from `store_of` when the configuration names a store, then `resolve` over the builder's configuration and for each site `Loader::mount` on the builder's loader (`Loader::at` when it has none), `load` and `HostBuilder::mount`.

### mount_all_with

* `mount_all_with(builder: HostBuilder, store: &dyn Store) -> Result<HostBuilder, SitesError>`: the same, fetching from `store` instead of the configured one.

### mountable

* `mountable(builder: HostBuilder) -> HostBuilder`: installs `mount_all` as the builder's sites mounter, so `Host::reload`, `Host::reload_sites` and `POST /__fsr/sites/reload` mount again.

### mountable_with

* `mountable_with(builder: HostBuilder, store: Arc<dyn Store>) -> HostBuilder`: `mountable` over `mount_all_with(store)`.

## 6. Watching

### watch

* `watch(host: Arc<Host>, root: PathBuf, poll: Option<Duration>)`: spawns onto the current tokio runtime a task that calls `Host::reload` on every `SIGHUP` (unix) and, with `poll`, a task that every `poll` rereads the configuration at `root`, resolves it and calls `Host::reload` when the rows' names, paths, versions or hashes changed since the last look. Results are logged under `fsr::sites`; a refused reload leaves the host as it was.

### poll_of

* `poll_of(config: &Config) -> Option<Duration>`: `sites.poll` parsed.

## 7. Error Handling

### SitesError

* `Host(HostError)`, transparent.
* `Artifact { name: String, message: String }`, displayed as `sites.<name>: <message>`.
* `Content(ArtifactError)`, transparent.

### InstallError

* `#[non_exhaustive]`. `Artifact(ArtifactError)`, transparent; `Io(PathBuf, std::io::Error)`; `Held { name, version, path }`; `Absent { store, package, version }`, displayed as `<store> holds no <package> at <version>`; `Fetch(url, message)`; `Mismatch { name, version, package, wanted }`.

### LayoutError

* `Escapes(String)`, a destination that would place a file outside the tree.
* `Collides(String)`, one destination wanted by two sources.
* `NoConfig`, a configuration read from no file, so a tree has nothing to carry.
* `Io(PathBuf, std::io::Error)` and `Layer(String)`, the generated configuration failing to serialize.

### ArtifactError

* `Io`, `Config(HostError)`, `NotASite(PathBuf)`, `Manifest { path, message }`, `Format(PathBuf, u32)`, `Archive(PathBuf, String)`, `Layout(LayoutError)`.
* From `verify`: `Hash { found, declared }`, `Digest { path, found, declared }`, `Missing { path }`, `Extra { path }`.
