# Usage Guide: snapfire_plugin

How a framework compiler plugin speaks to `snapfirec` and what `snapfirec` promises it.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Announcing the Plugin](#announcing-the-plugin)
* [Answering a Batch](#answering-a-batch)
* [Reporting a Diagnostic](#reporting-a-diagnostic)
* [Asking for a Sibling File](#asking-for-a-sibling-file)
* [Being Found and Kept](#being-found-and-kept)
* [Error Handling](#error-handling)

## Core Concepts

* **Plugin**: a separate executable, `snapfirec-<ext>`, that turns one source extension into a JavaScript or TypeScript module and, when the source has them, styles.
* **Worker**: the plugin process, spawned once per build and kept across rebuilds; every unit reaches it over a pipe.
* **Protocol**: the version number both sides carry as `PROTOCOL`. A `Hello` naming another is refused.
* **Hello**: the first line the plugin writes, saying what it is and what it compiles, after it has booted whatever it needs.
* **Request**: one batch of units, one line, with an id the response repeats.
* **Unit**: one source: where it is, what to call it in a diagnostic, its text, the build's options and any sibling files the plugin asked for.
* **Outcome**: what one unit became: compiled, failed with diagnostics or in need of files.
* **Compiled**: the module, its dialect, its styles, the specifiers it found that the source did not contain and any warnings.
* **Diagnostic**: structured, with a severity, a message and a place, so every plugin's errors print alike.
* **Options**: production, source maps and minification as the build asked for them.

## Quick Start

A worker in full: boot, announce, then answer until stdin closes.

```rust
use std::io::{BufRead, Write};
use snapfire_plugin::{Compiled, Diagnostic, Hello, Lang, Outcome, Request, Response, PROTOCOL};

fn main() {
  let stdout = std::io::stdout();
  let mut out = stdout.lock();
  let hello = Hello {
    protocol: PROTOCOL,
    name: "vue".to_owned(),
    version: env!("CARGO_PKG_VERSION").to_owned(),
    compiler: "@vue/compiler-sfc 3.5.13".to_owned(),
    extensions: vec![".vue".to_owned()],
  };
  writeln!(out, "{}", serde_json::to_string(&hello).unwrap()).unwrap();
  out.flush().unwrap();

  for line in std::io::stdin().lock().lines() {
    let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
    let results = request.units.iter().map(|unit| compile(unit)).collect();
    writeln!(out, "{}", serde_json::to_string(&Response { id: request.id, results }).unwrap()).unwrap();
    out.flush().unwrap();
  }
}

fn compile(unit: &snapfire_plugin::Unit) -> Outcome {
  Outcome::Ok(Compiled { js: format!("export default {:?};", unit.source), lang: Lang::Js, ..Compiled::default() })
}
```

## Announcing the Plugin

`Hello` is written once, before anything is read and after the plugin has booted its compiler, so a host that has read the greeting knows the worker is ready rather than merely running.

```rust
let hello = Hello { protocol: PROTOCOL, name: "vue".to_owned(), version: "0.1.0".to_owned(), compiler: "@vue/compiler-sfc 3.5.13".to_owned(), extensions: vec![".vue".to_owned()] };
```

`name` is the extension without its dot, since the binary is found as `snapfirec-<name>`. `version` and `compiler` are half of the host's cache key, so a plugin that upgrades its compiler changes `compiler` and every cached compile misses once.

## Answering a Batch

A request carries units of one extension and an id. The response repeats the id and carries one outcome per unit, in order; a response with another id or another count is refused by the host.

```rust
let results: Vec<Outcome> = request.units.iter().map(|unit| match compile_one(unit) {
  Ok(compiled) => Outcome::Ok(compiled),
  Err(diagnostics) => Outcome::Failed { diagnostics },
}).collect();
let response = Response { id: request.id, results };
```

A unit that fails does not fail the batch. The host prints its diagnostics, marks the build failed and still writes what the other units became.

`Compiled::lang` says what dialect `js` is in. A plugin whose framework allows typed script blocks hands the types straight through as `Lang::Ts` and lets the build strip them, since the build already has a TypeScript front end.

## Reporting a Diagnostic

A diagnostic is structured. The host prints it with the build's own prefix, so three plugins' errors read as one tool's.

```rust
use snapfire_plugin::Diagnostic;

let refused = Diagnostic::error("Element is missing end tag.").at("src/Card.vue", Some(3), Some(5));
let tip = Diagnostic::warning("v-for on a component without a key").at("src/Card.vue", Some(7), None);
```

An `Outcome::Failed` carries at least one diagnostic; an `Outcome::Ok` may carry warnings in `Compiled::diagnostics`, which the host prints without stopping.

## Asking for a Sibling File

A plugin never opens a file. When a block names one, `<style src="./card.css">`, the plugin answers with what it needs, by the specifier as written:

```rust
if unit.files.get("./card.css").is_none() {
  return Outcome::Needs { files: vec!["./card.css".to_owned()] };
}
let css = &unit.files["./card.css"];
```

The host reads each file relative to the unit's `path`, sends the unit again with `files` filled and records the file as a dependency of the source, so a change to it recompiles the component. A unit that asks twice has failed. Name the specifier in `Compiled::deps` too, so the build knows what the source reached for.

## Being Found and Kept

The host looks for `snapfirec-<ext>` on `PATH`, once per extension per build. A missing binary is reported to the user with `cargo install snapfire_<ext>`, so a plugin crate is named after its extension. The worker is kept for the build's length and across every rebuild under `--watch` or `--driven`; stdin closing is how it learns the build is over; a worker that does not exit on that is killed.

## Error Handling

The crate has no error type of its own: the protocol's failures are values. A plugin reports a unit's failure as `Outcome::Failed`, prints anything about its own state to stderr and exits non-zero only when it cannot go on, since the host attaches that stderr to the error it raises.

```rust
match outcome {
  Outcome::Ok(compiled) => emit(compiled),
  Outcome::Failed { diagnostics } => diagnostics.iter().for_each(|d| eprintln!("{d:?}")),
  Outcome::Needs { files } => resend_with(files),
}
```

A `Hello` whose `protocol` is not `PROTOCOL` is refused by the host with both numbers and which side to update.
