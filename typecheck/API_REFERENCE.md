# API Reference: snapfire_typecheck

Resolves and verifies a `tsc` of a requested version, runs it over a tsconfig and returns its diagnostics as values.

## Contents

* [1. Constants](#1-constants)
  * [`DEFAULT_VERSION`](#default_version)
  * [`REGISTRY`](#registry)
  * [`PINNED_PLATFORMS`](#pinned_platforms)
* [2. Resolving a Compiler](#2-resolving-a-compiler)
  * [`Options`](#options)
  * [`Resolved`](#resolved)
  * [`Source`](#source)
  * [`resolve`](#resolve)
* [3. The Cache](#3-the-cache)
  * [`cache_root`](#cache_root)
  * [`install_dir`](#install_dir)
  * [`cached`](#cached)
  * [`platform`](#platform)
  * [`is_pinned`](#is_pinned)
* [4. Checking](#4-checking)
  * [`check`](#check)
  * [`parse`](#parse)
  * [`Diagnostic`](#diagnostic)
  * [`Severity`](#severity)
* [5. The Command Line](#5-the-command-line)
  * [`snapfiretc`](#snapfiretc)
* [6. Error Handling](#6-error-handling)
  * [`Error`](#error)

## 1. Constants

### DEFAULT_VERSION

* `pub const DEFAULT_VERSION: &str = "7.0.2"`: the version resolved when nothing asks for another.

### REGISTRY

* `pub const REGISTRY: &str = "https://registry.npmjs.org"`: where a fetch reads the package document. The tarball is fetched from the URL that document names.

### PINNED_PLATFORMS

* `pub const PINNED_PLATFORMS: &[&str] = &["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64", "win32-x64", "win32-arm64"]`: the platforms whose sha512 for `DEFAULT_VERSION` is in this crate's source.

## 2. Resolving a Compiler

### Options

What to run and where it may come from.

* `pub struct Options { pub version: String, pub tsc: Option<PathBuf>, pub cache: Option<PathBuf>, pub registry: String, pub expect: Option<String>, pub offline: bool }`. `#[non_exhaustive]`. `Debug`, `Clone`.
* `version`: the requested version.
* `tsc`: a compiler to use as given. Its `--version` must report `version`.
* `cache`: the cache root. `cache_root(None)` when absent.
* `registry`: the registry base URL.
* `expect`: the `sha512-` integrity a fetched tarball must have. A hash this crate pins for the version and platform takes precedence.
* `offline`: refuse to fetch.
* `Default`: `DEFAULT_VERSION`, no compiler, the default cache, `REGISTRY`, no expected hash and fetching on.
* `pub fn version(mut self, version: String) -> Self`
* `pub fn tsc(mut self, tsc: Option<PathBuf>) -> Self`
* `pub fn cache(mut self, cache: Option<PathBuf>) -> Self`
* `pub fn registry(mut self, registry: String) -> Self`
* `pub fn expect(mut self, expect: Option<String>) -> Self`
* `pub fn offline(mut self, offline: bool) -> Self`

### Resolved

The compiler a resolution settled on.

* `pub struct Resolved { pub tsc: PathBuf, pub version: String, pub source: Source, pub sha512: Option<String> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* `tsc`: the executable to run.
* `version`: always the requested version.
* `source`: where the compiler came from.
* `sha512`: the integrity verified, set only when this resolution fetched. Omitted from JSON when `None`.

### Source

* `pub enum Source { Given, Cache, Path, Fetched }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Serializes lowercase. `Display` prints `given`, `cache`, `PATH` and `fetched`.

### resolve

* `pub fn resolve(options: &Options) -> Result<Resolved, Error>`: tries `options.tsc`, then the cache, then a `tsc` on `PATH` reporting the requested version, then a fetch.
* A `tsc` on `PATH` reporting another version is skipped without an error.
* A fetch reads the document at `<registry>/@typescript%2ftypescript-<platform>/<version>`, then the tarball. The tarball is checked against the pinned hash, else `expect`, else the document's integrity, before anything is written.
* Only a fetch writes to the cache.

## 3. The Cache

### cache_root

* `pub fn cache_root(explicit: Option<&Path>) -> Result<PathBuf, Error>`: `explicit`, else `$SNAPFIRE_CACHE`, else `~/Library/Caches/snapfire` on macOS, `%LOCALAPPDATA%\snapfire` on Windows and `$XDG_CACHE_HOME/snapfire` or `~/.cache/snapfire` elsewhere.
* `Error::NoHome` when there is no home directory and nothing names one.

### install_dir

* `pub fn install_dir(cache: &Path, version: &str, platform: &str) -> PathBuf`: `<cache>/tsc/<version>/<platform>`, the unpacked package. The compiler inside it is `lib/tsc`. On Windows it is `lib/tsc.exe`.

### cached

* `pub fn cached(cache: Option<&Path>, version: &str) -> Result<Option<PathBuf>, Error>`: the cached compiler for `version` on this platform, when one is unpacked. Never fetches.

### platform

* `pub fn platform() -> Result<String, Error>`: npm's `<os>-<arch>` for the running target. `macos` is `darwin`, `windows` is `win32`, `x86_64` is `x64` and `aarch64` is `arm64`.
* `Error::Platform` for an architecture TypeScript publishes nothing for.

### is_pinned

* `pub fn is_pinned(version: &str, platform: &str) -> bool`: whether this crate's source carries the hash for that version and platform.

## 4. Checking

### check

* `pub fn check(tsc: &Path, root: &Path, config: &Path) -> Result<Vec<Diagnostic>, Error>`: runs `tsc --noEmit --pretty false -p <config>` with `root` as the working directory. Diagnostic paths are relative to `root`.
* An empty vector means the project is clean.
* `Error::Tsc` when the compiler exits nonzero without printing a diagnostic.

### parse

* `pub fn parse(text: &str) -> Vec<Diagnostic>`: the lines of `--pretty false` output as diagnostics.
* An indented line continues the diagnostic above it. A line in no known shape becomes a diagnostic of its own.

### Diagnostic

One diagnostic, with TypeScript's own code.

* `pub struct Diagnostic { pub file: Option<String>, pub line: u32, pub column: u32, pub code: String, pub severity: Severity, pub message: String }`. `#[non_exhaustive]`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `file`: relative to the root the check ran in. `None` for a diagnostic about the project. Omitted from JSON when `None`.
* `line` and `column`: 1-based. Both `0` when the compiler gave no position.
* `code`: `TS2322` and the like. Empty for a line that carried none.
* `pub fn is_error(&self) -> bool`
* `Display`: `file(line,column): severity code: message`. The place is left out without a `file` and the code is left out when empty.

### Severity

* `pub enum Severity { Error, Warning }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Serializes lowercase. `Display` prints `error` and `warning`.

## 5. The Command Line

### snapfiretc

* `snapfiretc [OPTIONS]`: resolves a compiler and checks the project.
* `--root <dir>`, default `.`: the working directory the compiler runs in. Must be a directory.
* `--config <tsconfig>`, default `tsconfig.json`: relative to the root or absolute. Must exist.
* `--format text|json`, default `text`. `text` prints one diagnostic per line on stdout and the resolution on stderr. `json` prints one object on stdout and nothing else.
* `--tsc <path>`, `--tsc-version <version>`, `--cache <dir>`, `--registry <url>`, `--expect <sha512>` and `--offline`: the `Options` fields of the same names.
* `--which`: resolve and report, checking nothing.
* `--version` and `-h`, `--help`.
* The JSON object carries `tsc`, `version`, `source`, `sha512`, `pinned` and `diagnostics`.
* Exit codes: `0` when nothing is wrong, `1` when at least one diagnostic is an error and `2` when the check could not run.

## 6. Error Handling

### Error

`#[non_exhaustive]`. `Debug`, `std::error::Error`.

* `Io(PathBuf, std::io::Error)`: writing or unpacking the cache.
* `Http(String, String)`: a URL and what went wrong, including a status that is not a success.
* `Mismatch { path: PathBuf, found: String, want: String }`: the compiler `Options::tsc` names reports another version.
* `Spawn { path: PathBuf, source: std::io::Error }`: the compiler could not be started.
* `Platform(String)`: no TypeScript is published for the running target.
* `Integrity { url: String, found: String, want: String }`: the tarball does not hash to the expected integrity. Nothing is written.
* `NoIntegrity(String)`: the registry document carries no integrity and no hash was pinned or passed.
* `Offline(String)`: the version is not in the cache or on `PATH` and fetching is off.
* `NoBinary(PathBuf)`: the unpacked package holds no `lib/tsc`.
* `NoHome`: no home directory to put the cache in.
* `Tsc { path: PathBuf, status: String, output: String }`: the compiler failed without printing a diagnostic. `output` holds what it wrote.
