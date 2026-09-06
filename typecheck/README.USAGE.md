# Usage Guide: snapfire_typecheck

How to check a project's types, how the compiler that does it is chosen and fetched, and how the two build tools drive it.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Checking a Project](#checking-a-project)
* [Choosing a Version](#choosing-a-version)
* [Using a Compiler You Already Have](#using-a-compiler-you-already-have)
* [Where the Cache Lives](#where-the-cache-lives)
* [Fetching and Verifying](#fetching-and-verifying)
* [Reading the Report as JSON](#reading-the-report-as-json)
* [Checking from snapfirec](#checking-from-snapfirec)
* [Checking from fsr](#checking-from-fsr)
* [Why a Separate Executable](#why-a-separate-executable)
* [Error Handling](#error-handling)

## Core Concepts

* **Requested version** is the TypeScript every step resolves against. Nothing is ever used because it happened to be installed.
* **Resolution ladder** is the order: a compiler given by path, the cache, a `tsc` on `PATH` that reports the requested version, then a fetch.
* **Cache** is one directory outside any repository, keyed by version and platform, shared by every project on the machine.
* **Platform** is npm's own `<os>-<arch>` spelling, `darwin-arm64` or `linux-x64`, which names both the package and its cache directory.
* **Pinned hash** is a `sha512-` integrity this crate carries in its own source for its default version, so the common path is verified against bytes committed to a repository.
* **Registry integrity** is the `dist.integrity` of the registry's document, which a version the crate pins no hash for is verified against instead.
* **Diagnostic** is one line of TypeScript's output as a value: a file, a position, a code such as `TS2322`, a severity and a message.
* **Report** is what `snapfiretc` prints: the diagnostics, plus which compiler ran and where it came from.
* **Checker** is the `snapfiretc` executable. `fsr` and `snapfirec` spawn it and render what it says; neither links this crate.

## Quick Start

From the command line, over a project's tsconfig:

```sh
snapfiretc --root app --config tsconfig.json
# routes/page.tsx(1,15): error TS2305: Module '"@generated/client"' has no exported member 'IndexProps'.
# tsc 7.0.2 from cache
```

From Rust:

```rust
use snapfire_typecheck::{check, resolve, Options};
use std::path::Path;

fn main() -> Result<(), snapfire_typecheck::Error> {
  let resolved = resolve(&Options::default())?;
  let diagnostics = check(&resolved.tsc, Path::new("app"), Path::new("tsconfig.json"))?;
  for diagnostic in &diagnostics {
    println!("{diagnostic}");
  }
  println!("tsc {} from {}", resolved.version, resolved.source);
  Ok(())
}
```

## Checking a Project

`--root` is the working directory the compiler runs in, so every diagnostic's file is relative to it, and `--config` is the tsconfig, relative to the root or absolute:

```sh
snapfiretc --root app --config tsconfig.json
```

The exit code is 0 when nothing is wrong, 1 when a diagnostic is an error and 2 when the check could not be run at all: no compiler, no such tsconfig or a compiler that failed without printing a diagnostic. A warning alone leaves the exit code at 0.

The flags passed to the compiler are `--noEmit --pretty false -p <config>`, so nothing is written and the output is machine-readable. Everything else comes from the tsconfig, including `strict`, the `paths` aliases and what is included.

## Choosing a Version

The requested version is `--tsc-version` when given, else the default this crate pins:

```sh
snapfiretc --root app --config tsconfig.json --tsc-version 7.0.3
```

An FSR application writes it once in its configuration instead, and `fsr` passes it through:

```toml
[typecheck]
version = "7.0.2"
```

A version the crate carries no hash for is fetched with the registry's own integrity, and `fsr` records both the version and that integrity in the configuration, so every later build verifies against a value the project holds:

```toml
[typecheck]
version = "7.1.0"
sha512 = "sha512-Qh4eU4..."
```

## Using a Compiler You Already Have

An explicit path skips the cache, `PATH` and the network:

```sh
snapfiretc --root app --config tsconfig.json --tsc /opt/typescript/lib/tsc --tsc-version 7.0.2
```

The path is asked what it is and a compiler reporting anything else is an error, never a warning, so an air-gapped build fails loudly rather than checking against the wrong TypeScript.

A `tsc` on `PATH` is taken on the same terms and without the flag, but only after the cache and only when it reports the requested version. One reporting anything else is ignored in silence: a machine having some other TypeScript on it is not a problem.

## Where the Cache Lives

One directory per platform, holding one unpacked package per version and platform:

| Platform | Directory |
| --- | --- |
| macOS | `~/Library/Caches/snapfire/tsc/<version>/<platform>/` |
| Windows | `%LOCALAPPDATA%\snapfire\tsc\<version>\<platform>\` |
| Everything else | `$XDG_CACHE_HOME/snapfire/tsc/<version>/<platform>/`, else `~/.cache/snapfire/...` |

`$SNAPFIRE_CACHE` replaces the `snapfire` directory, and `--cache <dir>` replaces it for one run:

```sh
snapfiretc --which --cache /var/cache/snapfire
```

An upgrade is a new directory beside the old one, so a rollback costs nothing and two applications pinning different versions share one cache without conflict. Reading it back is `--which`, which resolves and prints without checking anything:

```sh
snapfiretc --which
# tsc 7.0.2 from cache
```

## Fetching and Verifying

A cache miss with nothing usable on `PATH` fetches, which is one HTTPS GET for the registry document, one for the tarball and no npm client anywhere:

```sh
snapfiretc --which --tsc-version 7.0.2
# tsc 7.0.2 is not in the cache; taking it from PATH when it reports that version, else fetching it
# tsc 7.0.2 from fetched
```

The tarball is `@typescript/typescript-<platform>`, around 9 MB gzipped and 27 MB unpacked, and its sha512 is checked before anything is written: against this crate's pinned hash when it has one, else against `--expect` when the caller passes one, else against the registry document's own integrity. It is unpacked beside the target directory and renamed into place, so a cache entry is whole or absent and never half-written.

A machine that must not reach the network says so and gets an error rather than a hang:

```sh
snapfiretc --root app --config tsconfig.json --offline
```

A company behind a mirror points the fetch elsewhere:

```sh
snapfiretc --which --registry https://npm.internal/registry
```

## Reading the Report as JSON

`--format json` prints one object on stdout and nothing else, which is what `fsr` reads:

```sh
snapfiretc --root app --config tsconfig.json --format json
```

```json
{"tsc":"/Users/me/Library/Caches/snapfire/tsc/7.0.2/darwin-arm64/lib/tsc","version":"7.0.2","source":"cache","sha512":null,"pinned":true,"diagnostics":[{"file":"routes/page.tsx","line":1,"column":15,"code":"TS2305","severity":"error","message":"Module '\"@generated/client\"' has no exported member 'IndexProps'."}]}
```

`source` is `given`, `cache`, `path` or `fetched`. `sha512` carries a value only when this run fetched, and `pinned` says whether the crate's own source holds the hash for that version, which is how a caller knows whether the integrity is worth recording.

## Checking from snapfirec

`snapfirec` compiles and never checks. `--typecheck` spawns this crate's binary over the same tsconfig once the build has emitted, and a diagnostic fails the build:

```sh
snapfirec --config tsconfig.json --typecheck --tsc-version 7.0.2
```

The checker is `$SNAPFIRETC`, else `snapfiretc` beside `snapfirec`, else one on `PATH`; `--snapfiretc <path>` names it outright. Unlike `fsr`, a missing checker is an error here, since the flag asked for a check. Under `--watch` the check runs once, with the first build, rather than per rebuild.

## Checking from fsr

`fsr build`, `fsr dev` and `fsr check` run the checker over the application's `tsconfig.json`, the one that carries `strict` and the `@src`, `@routes` and `@generated` aliases, and specs are in it too. It runs beside `snapfirec` rather than after it, since neither reads the other's output:

```sh
fsr build app
# typecheck tsc 7.0.2 from cache, clean
```

`--no-typecheck` turns it off for one run and `[typecheck] enabled = false` for a project. `--tsc`, `--tsc-version` and `--snapfiretc` pass straight through. A checker that is not installed is a note rather than a failure, because an application that never asked for one still has to build; a checker that runs and finds an error fails `fsr build` and `fsr check`, while `fsr dev` prints the diagnostics and leaves the server up, since types are stripped either way and a running page is worth more than a stopped one.

A build script driving `fsr` from Cargo finds no checker beside itself, so it names one:

```rust
let mut options = snapfire_fsr_cli::DevOptions::beside(&app);
options.typecheck.checker = std::env::var_os("SNAPFIRETC").map(std::path::PathBuf::from);
```

## Why a Separate Executable

`snapfire_compiler` is binary-only: nine modules behind `snapfirec` and no library surface for a checker to be wired into. Everything already crosses a process boundary through a file, since `fsr build` writes the tsconfig and `snapfirec` reads it back, so a checker is a third peer on the same file rather than a new dependency for everybody.

That is what keeps the cost where it belongs. Whoever never asks for a typecheck never builds this crate, never downloads a 23 MB compiler and never learns it exists, and the engine behind the seam can change without either build tool noticing.

The measurement that settled running it on every build rather than as a step somebody remembers: the shopping example, 149 files, checked in 0.085 seconds.

## Error Handling

Every function returns `Result<_, Error>`, and the variants worth matching on say which rung failed:

```rust
use snapfire_typecheck::{resolve, Error, Options};

match resolve(&Options::default()) {
  Ok(resolved) => println!("{}", resolved.tsc.display()),
  Err(Error::Mismatch { path, found, want }) => eprintln!("{}: {found}, wanted {want}", path.display()),
  Err(Error::Integrity { url, .. }) => eprintln!("{url}: the bytes are not what was published"),
  Err(Error::Offline(version)) => eprintln!("TypeScript {version} is not on this machine"),
  Err(e) => eprintln!("{e}"),
}
```

`Error::Tsc` is the compiler exiting nonzero without printing a diagnostic, and it carries what the child wrote on both streams. It is a tooling failure rather than a type error, which is why `snapfiretc` exits 2 for it and 1 for a diagnostic.
