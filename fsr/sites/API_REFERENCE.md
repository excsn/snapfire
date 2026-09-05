# snapfire_fsr_sites API reference

`snapfire_fsr_sites`: the `[sites]` table of a shell's configuration resolved, hashed and mounted on `snapfire_fsr_host`, and watched.

## Contents

* [1. Resolving](#1-resolving)
  * [`Resolved`](#resolved)
  * [`resolve`](#resolve)
  * [`hash_dir`](#hash_dir)
* [2. Mounting](#2-mounting)
  * [`mount_all`](#mount_all)
* [3. Watching](#3-watching)
  * [`watch`](#watch)
  * [`poll_of`](#poll_of)
* [4. Error Handling](#4-error-handling)
  * [`SitesError`](#siteserror)

## 1. Resolving

### Resolved

* `pub struct Resolved { pub name: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub allow_engine: bool }`: one row of the table resolved. `Debug`, `Clone`, `PartialEq`, `Eq`.

### resolve

* `resolve(config: &Config) -> Result<Vec<Resolved>, SitesError>`: every `[sites.<name>]` row in name order; `name@version` under `sites.root` with that version, anything else a path against `config.root` with version `path`; each directory hashed and refused with `Artifact` when it is not a directory or its hash differs from a pinned `hash`. Empty without a `[sites]` section.

### hash_dir

* `hash_dir(dir: &Path) -> std::io::Result<String>`: xxh3 over every file under `dir` in path order, each as its relative path, a zero byte, its bytes and a zero byte, entries whose name starts with `.` and `target` skipped; sixteen hex digits.

## 2. Mounting

### mount_all

* `mount_all(builder: HostBuilder) -> Result<HostBuilder, SitesError>`: `resolve` over the builder's configuration, then `Mount::load` and `HostBuilder::mount` for each.

## 3. Watching

### watch

* `watch(host: Arc<Host>, root: PathBuf, poll: Option<Duration>)`: spawns onto the current tokio runtime a task that calls `Host::reload` on every `SIGHUP` (unix) and, with `poll`, a task that every `poll` rereads the configuration at `root`, resolves it and calls `Host::reload` when the rows' names, paths, versions or hashes changed since the last look. Results are logged under `fsr::sites`; a refused reload leaves the host as it was.

### poll_of

* `poll_of(config: &Config) -> Option<Duration>`: `sites.poll` parsed.

## 4. Error Handling

### SitesError

* `Host(HostError)`, transparent.
* `Artifact { name: String, message: String }`, displayed as `sites.<name>: <message>`.
