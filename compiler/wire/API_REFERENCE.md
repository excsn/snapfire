# API Reference: snapfire_compiler_wire

The wire contract between `snapfirec` and a framework compiler plugin, one JSON object per line over stdin and stdout.

## Contents

* [1. The Protocol](#1-the-protocol)
  * [`PROTOCOL`](#protocol)
  * [`EXTENSIONS`](#extensions)
  * [`SUFFIXES`](#suffixes)
  * [`Hello`](#hello)
* [2. Requests](#2-requests)
  * [`Request`](#request)
  * [`Unit`](#unit)
  * [`Options`](#options)
  * [`Kind`](#kind)
* [3. Responses](#3-responses)
  * [`Response`](#response)
  * [`Outcome`](#outcome)
  * [`Compiled`](#compiled)
  * [`Described`](#described)
  * [`Script`](#script)
  * [`Lang`](#lang)
* [4. Diagnostics](#4-diagnostics)
  * [`Diagnostic`](#diagnostic)
  * [`Severity`](#severity)
* [5. The Host](#5-the-host)
  * [`Worker`](#worker)
* [6. The Driven Protocol](#6-the-driven-protocol)
  * [Lines](#lines)
  * [`PluginRequest`](#pluginrequest)
  * [`PluginAnswer`](#pluginanswer)
  * [`AssetMap`](#assetmap)
  * [`MappedAsset`](#mappedasset)
* [7. Error Handling](#7-error-handling)
  * [`HostError`](#hosterror)

## 1. The Protocol

### PROTOCOL

* `pub const PROTOCOL: u32 = 4`: the plugin protocol version. The host refuses a `Hello` naming another.

### EXTENSIONS

* `pub const EXTENSIONS: &[&str] = &["vue", "svelte"]`: the extensions a plugin compiles, without dots.
* `pub fn claimed(ext: &str) -> Option<&'static str>`: `ext` as a static name when `EXTENSIONS` holds it.
* `pub fn binary_for(ext: &str) -> String`: `snapfirec-<ext>`, the binary the host runs from `PATH`.
* `pub fn install_hint(ext: &str) -> String`: `cargo install snapfire_<ext>`.

### SUFFIXES

* `pub const SUFFIXES: &[(&str, &str)] = &[(".svelte.ts", "svelte"), (".svelte.js", "svelte")]`: file name suffixes a plugin claims ahead of the last extension, each with the extension whose plugin compiles it.
* `pub fn claimed_name(name: &str) -> Option<&'static str>`: the extension whose plugin compiles the file `name`. A suffix in `SUFFIXES` is tried first, then the last extension. Case-insensitive.
* `pub fn stripped_first(name: &str) -> bool`: whether `name` ends in a claimed suffix ending in `.ts`. The host strips such a file's types and sends it as JavaScript.

### Hello

The first line a plugin writes, once its compiler has booted.

* `pub struct Hello { pub protocol: u32, pub name: String, pub version: String, pub compiler: String, pub extensions: Vec<String> }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* `name` is the extension without its dot, as in `snapfirec-<name>`.
* `version` is the plugin's own. `compiler` is free text naming the compiler it uses.
* `extensions` lists the extensions it claims, each with its dot.
* `name`, `version` and `compiler` are part of `snapfirec`'s plugin cache key.

## 2. Requests

### Request

One batch, on one line.

* `pub struct Request { pub id: u64, pub kind: Kind, pub units: Vec<Unit> }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* Every unit shares one extension. The response repeats `id`.
* `kind` is absent from the JSON for a compile.

### Unit

* `pub struct Unit { pub filename: String, pub path: String, pub source: String, pub options: Options, pub files: BTreeMap<String, String> }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* `filename`: the path a diagnostic names, relative to the project root.
* `path`: where the source is on disk.
* `files`: the sibling files the plugin asked for with `Outcome::Needs`, keyed by the specifier it asked with. Empty on the first send and absent from the JSON when empty.

### Options

What the build asked for.

* `pub struct Options { pub production: bool, pub source_map: bool, pub minify: bool, pub compiler: Option<String> }`. `Debug`, `Clone`, `Default`, `Serialize`, `Deserialize`.
* Each flag is `false` when absent from the JSON. `Default` is all `false` with no compiler.
* `production`: no development-only branches and no warnings kept. `source_map`: a source map beside the code. `minify`: minified code and styles.
* `compiler`: a framework compiler file the plugin boots in place of the one it carries. Absent from the JSON when `None`.

### Kind

* `pub enum Kind { Compile, Describe }`. `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Serialized lowercase. `Default` is `Compile`.
* `Compile` is answered per unit with `Outcome::Ok` and `Describe` with `Outcome::Described`. Either may be answered with `Failed` or `Needs` instead.

## 3. Responses

### Response

* `pub struct Response { pub id: u64, pub results: Vec<Outcome> }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* One outcome per unit, in request order. The host refuses a response whose `id` or count differs from the request.

### Outcome

* `pub enum Outcome { Ok(Compiled), Described(Described), Failed { diagnostics: Vec<Diagnostic> }, Needs { files: Vec<String> } }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* Tagged in JSON by `status`: `ok`, `described`, `failed` or `needs`.
* `Failed::diagnostics` is never empty.
* `Needs::files` are specifiers as the source wrote them, a `<style src>` among them. The host sends the unit again with them in `Unit::files`. A unit that answers `Needs` a second time has failed.

### Compiled

* `pub struct Compiled { pub js: String, pub lang: Lang, pub css: Option<String>, pub source_map: Option<String>, pub deps: Vec<String>, pub diagnostics: Vec<Diagnostic> }`. `Debug`, `Clone`, `Default`, `Serialize`, `Deserialize`.
* `js`: the module the unit became. The build resolves its imports.
* `lang`: the dialect `js` is written in.
* `css`: every style block as one string, already scoped when the source asked for it. Absent from the JSON when `None`, as is `source_map`.
* `deps`: specifiers the plugin found that the source's imports do not contain, a `src=` block's file among them. Each becomes an edge in the module graph.
* `diagnostics`: warnings that did not stop the compile.
* `Default` is an empty JavaScript module.

### Described

A component as its framework's parser reads it.

* `pub struct Described { pub template: Option<serde_json::Value>, pub script: Option<Script>, pub bindings: BTreeMap<String, String>, pub scope: Option<String>, pub slotted: bool, pub deps: Vec<String>, pub diagnostics: Vec<Diagnostic> }`. `Debug`, `Clone`, `Default`, `Serialize`, `Deserialize`.
* `template`: the template as a tree in the plugin's own shape. Each plugin's API reference gives its nodes.
* `script`: the component's script, when it has one.
* `bindings`: for each name the script binds, how the template reads it, in the framework's own words.
* `scope`: the attribute a scoped style selects on, when any style is scoped.
* `slotted`: whether a scoped style has a `:slotted()` rule.
* `deps` and `diagnostics`: as on `Compiled`.
* A description carrying an `Error` diagnostic describes the script alone.
* `template`, `script`, `bindings` and `scope` are absent from the JSON when `None` or empty. `Default` describes nothing.

### Script

A script block as written.

* `pub struct Script { pub content: String, pub lang: Lang, pub line: u32, pub column: u32, pub setup: bool, pub plain: bool }`. `Debug`, `Clone`, `Default`, `Serialize`, `Deserialize`.
* `line` and `column`: one-based, where `content` starts in the file.
* `setup`: the block is the framework's setup form.
* `plain`: a plain script stands beside the setup block.

### Lang

* `pub enum Lang { Js, Ts }`. `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Serialized lowercase. `Default` is `Js`.

## 4. Diagnostics

### Diagnostic

* `pub struct Diagnostic { pub severity: Severity, pub message: String, pub file: Option<String>, pub line: Option<u32>, pub column: Option<u32>, pub code: Option<String> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* `code`: the framework's own name for what it found, such as Svelte's `a11y_missing_attribute`.
* `file`, `line`, `column` and `code` are absent from the JSON when `None`.
* `pub fn error(message: impl Into<String>) -> Diagnostic`: an error with no place.
* `pub fn warning(message: impl Into<String>) -> Diagnostic`: a warning with no place.
* `pub fn at(mut self, file: impl Into<String>, line: Option<u32>, column: Option<u32>) -> Diagnostic`: the same diagnostic with a place. `line` and `column` are one-based.

### Severity

* `pub enum Severity { Error, Warning }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* Serialized lowercase. An `Error` in a unit's diagnostics fails the build. A `Warning` is printed.

## 5. The Host

### Worker

One plugin process, in `host`.

* `pub struct Worker`
* `pub fn start(ext: &str) -> Result<Worker, HostError>`: spawns `snapfirec-<ext>` with stdin, stdout and stderr piped and reads its `Hello`. `HostError::Protocol` when it names another `PROTOCOL`.
* `pub fn hello(&self) -> &Hello`
* `pub fn name(&self) -> String`: `snapfirec-<hello.name>`.
* `pub fn compile(&mut self, units: Vec<Unit>) -> Result<Vec<Outcome>, HostError>`: one `Compile` batch.
* `pub fn describe(&mut self, units: Vec<Unit>) -> Result<Vec<Outcome>, HostError>`: one `Describe` batch.
* Batch ids count up from 1.
* Dropping the worker kills the plugin process and waits for it.

## 6. The Driven Protocol

The lines `snapfirec --driven` and the process driving it exchange over the compiler's stdin and stdout, in `driven`.

### Lines

* `pub const PROTOCOL: u32 = 3`: the driven protocol version, announced in the compiler's first line.
* `pub const HELLO: &str = "snapfirec: driven"`
* `pub fn hello() -> String`: `HELLO`, a space and `PROTOCOL`.
* `pub fn parse_hello(line: &str) -> Option<u32>`: the version a hello line announces. `None` for any other line.
* `pub const REBUILT: &str = "snapfirec: rebuilt"` and `pub const FAILED: &str = "snapfirec: failed"`: one of them ends every batch.
* `pub const REFERENCES: &str = "snapfirec: references"`: written when a batch referenced an asset the map does not name. One root-relative path per line follows, then an empty line. The compiler then waits for `MAPPED`.
* `pub const MAPPED: &str = "mapped"`: the driver's answer once it has rewritten the map. The compiler reads the map again and compiles the waiting sources.
* `pub const PLUGIN: &str = "snapfirec: plugin"`: the next line is a `PluginRequest` as JSON. The driver answers with one line, a `PluginAnswer`.

### PluginRequest

* `pub enum PluginRequest { Hello { ext: String }, Compile { ext: String, units: Vec<Unit> } }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* Tagged in JSON by `ask`, lowercase.

### PluginAnswer

* `pub enum PluginAnswer { Hello { hello: Hello }, Compiled { outcomes: Vec<Outcome> }, Refused { why: String } }`. `Debug`, `Clone`, `Serialize`, `Deserialize`.
* Tagged in JSON by `answer`, lowercase.
* `Refused`: the plugin could not start or failed to answer.

### AssetMap

* `pub struct AssetMap { pub version: u32, pub assets: BTreeMap<String, MappedAsset> }`. `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `pub const MAP_VERSION: u32 = 1`. `snapfirec` refuses a map of another version.
* `pub fn new() -> AssetMap`: empty, at `MAP_VERSION`.
* Keys are paths relative to the compiler's root, with forward slashes.

### MappedAsset

* `pub struct MappedAsset { pub url: String, pub width: Option<u32>, pub height: Option<u32> }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`.
* `url`: what a reference is rewritten to. `width` and `height`: an image's displayed size, absent from the JSON when `None`.

## 7. Error Handling

A unit's failure is `Outcome::Failed`. A malformed line is a `serde_json::Error` on the side that read it.

### HostError

`Debug`, `std::error::Error`. Each variant's `binary` is the plugin's binary name. A `stderr` field holds what the plugin wrote to stderr, as a suffix of the message.

* `NotFound { binary: String, hint: String }`: `` `<binary>` is not on PATH; `<hint>` puts it there ``.
* `Spawn { binary: String, error: std::io::Error }`: the process would not start.
* `Silent { binary: String, stderr: String }`: it exited before writing its `Hello`.
* `Greeting { binary: String, line: String }`: its `Hello` did not parse.
* `Protocol { binary: String, theirs: u32, ours: u32 }`: it speaks another `PROTOCOL`.
* `Closed { binary: String, stderr: String }`: writing a request failed.
* `Died { binary: String, stderr: String }`: it closed stdout before answering.
* `Answer { binary: String, error: serde_json::Error }`: its response did not parse.
* `Batch { binary: String, got: u64, wanted: u64 }`: the response carried another `id`.
* `Count { binary: String, got: usize, wanted: usize }`: the response had another number of outcomes.
