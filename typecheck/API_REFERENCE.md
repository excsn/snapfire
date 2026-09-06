# API Reference: snapfire_typecheck

A `tsc` of a requested version, resolved and verified, and the diagnostics it prints as values.

## Contents

* [1. Constants](#1-constants)
  * [DEFAULT_VERSION](#default_version)
  * [REGISTRY](#registry)
  * [PINNED_PLATFORMS](#pinned_platforms)
* [2. Resolving a Compiler](#2-resolving-a-compiler)
  * [Options](#options)
  * [Resolved](#resolved)
  * [Source](#source)
  * [resolve](#resolve)
* [3. The Cache](#3-the-cache)
  * [cache_root](#cache_root)
  * [install_dir](#install_dir)
  * [cached](#cached)
  * [platform](#platform)
  * [is_pinned](#is_pinned)
* [4. Checking](#4-checking)
  * [check](#check)
  * [parse](#parse)
  * [Diagnostic](#diagnostic)
  * [Severity](#severity)
* [5. The Command Line](#5-the-command-line)
* [6. Error Handling](#6-error-handling)
  * [Error](#error)

## 1. Constants

### DEFAULT_VERSION

* `pub const DEFAULT_VERSION: &str = "7.0.2"`: the version resolved against when nothing asks for another, and the one this crate carries hashes for.

### REGISTRY

* `pub const REGISTRY: &str = "https://registry.npmjs.org"`: where a fetch reads the package document and the tarball. Only the tarball URL the document names is fetched, never a URL built from the base.

### PINNED_PLATFORMS

* `pub const PINNED_PLATFORMS: &[&str]`: `darwin-arm64`, `darwin-x64`, `linux-x64`, `linux-arm64`, `win32-x64` and `win32-arm64`, the platforms whose sha512 for `DEFAULT_VERSION` is in this crate's source. Every other platform TypeScript publishes is fetched with the registry's integrity.

## 2. Resolving a Compiler

### Options

What to run and where it may come from.

* `pub version: String`: the requested version. Defaults to `DEFAULT_VERSION`.
* `pub tsc: Option<PathBuf>`: a compiler to use as given. Its `--version` must be `version` or resolution fails with `Error::Mismatch`.
* `pub cache: Option<PathBuf>`: the cache root; `cache_root(None)` when absent.
* `pub registry: String`: defaults to `REGISTRY`.
* `pub expect: Option<String>`: the `sha512-` integrity a fetched tarball must have. A hash this crate pins for the version wins over it.
* `pub offline: bool`: fetching is refused, so a cache miss is `Error::Offline` rather than a network call.
* `Default` gives the default version, no explicit compiler, the platform cache, the npm registry and fetching on.

### Resolved

The compiler a resolution settled on.

* `pub tsc: PathBuf`: the executable to run.
* `pub version: String`: what it reports, which is always the requested version.
* `pub source: Source`: which rung answered.
* `pub sha512: Option<String>`: the integrity verified, set only when this resolution fetched.

### Source

Which rung of the ladder answered. Serializes lowercase.

* `Given`, `Cache`, `Path`, `Fetched`.
* `Display` prints `given`, `cache`, `PATH` and `fetched`.

### resolve

* `pub fn resolve(options: &Options) -> Result<Resolved, Error>`: an explicit path first, then the cache, then a `tsc` on `PATH` reporting the requested version, then a fetch. A compiler on `PATH` reporting another version is skipped without an error. Fetching writes into the cache; nothing else does.

## 3. The Cache

### cache_root

* `pub fn cache_root(explicit: Option<&Path>) -> Result<PathBuf, Error>`: `explicit`, else `$SNAPFIRE_CACHE`, else `~/Library/Caches/snapfire` on macOS, `%LOCALAPPDATA%\snapfire` on Windows and `$XDG_CACHE_HOME/snapfire` or `~/.cache/snapfire` elsewhere. `Error::NoHome` when there is no home directory and nothing named one.

### install_dir

* `pub fn install_dir(cache: &Path, version: &str, platform: &str) -> PathBuf`: `<cache>/tsc/<version>/<platform>`, the unpacked package. The compiler inside it is `lib/tsc`, and `lib/tsc.exe` on Windows.

### cached

* `pub fn cached(cache: Option<&Path>, version: &str) -> Result<Option<PathBuf>, Error>`: the cached compiler for a version, when one is unpacked already. Reads nothing else and never fetches.

### platform

* `pub fn platform() -> Result<String, Error>`: npm's `<os>-<arch>` for the running target: `macos` is `darwin`, `windows` is `win32`, `x86_64` is `x64` and `aarch64` is `arm64`. `Error::Platform` for an architecture TypeScript publishes nothing for.

### is_pinned

* `pub fn is_pinned(version: &str, platform: &str) -> bool`: whether this crate's source carries the hash for that pair, so a caller knows whether an integrity it was handed is worth recording.

## 4. Checking

### check

* `pub fn check(tsc: &Path, root: &Path, config: &Path) -> Result<Vec<Diagnostic>, Error>`: runs `tsc --noEmit --pretty false -p <config>` with `root` as the working directory, so every diagnostic's file is relative to it. An empty vector means the project is clean. `Error::Tsc` when the compiler exits nonzero without printing a diagnostic, carrying both streams.

### parse

* `pub fn parse(text: &str) -> Vec<Diagnostic>`: the lines of `--pretty false` output as diagnostics. An indented line continues the one above it, and a line in no known shape becomes a diagnostic of its own rather than being dropped.

### Diagnostic

One diagnostic, carrying TypeScript's own code.

* `pub file: Option<String>`: relative to the root the check ran in; `None` for a diagnostic about the project rather than a file.
* `pub line: u32`, `pub column: u32`: 1-based, both 0 when the compiler gave no position.
* `pub code: String`: `TS2322` and the like. Empty for a line that carried none.
* `pub severity: Severity`, `pub message: String`.
* `pub fn is_error(&self) -> bool`.
* `Display` prints `file(line,column): severity code: message`, the shape the compiler itself uses.
* Serializes with `file` omitted when absent.

### Severity

* `Error`, `Warning`. Serializes lowercase, and `Display` prints the same.

## 5. The Command Line

`snapfiretc` is the executable both build tools spawn.

* `--root <dir>`: the working directory the compiler runs in. Defaults to `.`.
* `--config <tsconfig>`: relative to the root or absolute. Defaults to `tsconfig.json`.
* `--format text|json`: `text` prints one diagnostic per line on stdout and the resolution on stderr; `json` prints one object on stdout and nothing else. Defaults to `text`.
* `--tsc <path>`, `--tsc-version <version>`, `--cache <dir>`, `--registry <url>`, `--expect <sha512>`, `--offline`: the fields of `Options`.
* `--which`: resolve and report, checking nothing.
* `--version`, `--help`.
* Exit codes: `0` nothing wrong, `1` at least one diagnostic is an error, `2` the check could not be run.
* The JSON object carries `tsc`, `version`, `source`, `sha512`, `pinned` and `diagnostics`.

## 6. Error Handling

### Error

* `Io(PathBuf, std::io::Error)`: writing or unpacking the cache.
* `Http(String, String)`: a URL and what went wrong, including a status that is not a success.
* `Mismatch { path, found, want }`: a compiler given by path or found on `PATH` reports another version. Only a path given by the caller turns this into a failure.
* `Spawn { path, source }`: the compiler could not be started.
* `Platform(String)`: no TypeScript is published for the running target.
* `Integrity { url, found, want }`: the tarball's bytes are not what was published or pinned. Nothing is written.
* `NoIntegrity(String)`: the registry document carries no integrity and no hash was pinned or passed.
* `Offline(String)`: the version is not in the cache and fetching is off.
* `NoBinary(PathBuf)`: the unpacked package holds no `lib/tsc`.
* `NoHome`: no home directory to put the cache in.
* `Tsc { path, status, output }`: the compiler failed without printing a diagnostic, with what it wrote.
