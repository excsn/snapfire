# API Reference: snapfire_plugin

The wire contract between `snapfirec` and a framework compiler plugin, one JSON object per line over stdin and stdout.

## Contents

* [1. The Protocol](#1-the-protocol)
  * [PROTOCOL](#protocol)
  * [Hello](#hello)
* [2. Requests](#2-requests)
  * [Request](#request)
  * [Unit](#unit)
  * [Options](#options)
* [3. Responses](#3-responses)
  * [Response](#response)
  * [Outcome](#outcome)
  * [Compiled](#compiled)
  * [Lang](#lang)
* [4. Diagnostics](#4-diagnostics)
  * [Diagnostic](#diagnostic)
  * [Severity](#severity)
* [5. Error Handling](#5-error-handling)

## 1. The Protocol

### PROTOCOL

* `pub const PROTOCOL: u32 = 2`
* The version both sides speak. A `Hello` naming another is refused by the host.

### Hello

* `pub struct Hello { pub protocol: u32, pub name: String, pub version: String, pub compiler: String, pub extensions: Vec<String> }`
* The first line a plugin writes, after its compiler has booted. `name` is the extension without its dot, which is the `snapfirec-<name>` the host found it as; `version` is the plugin's own; `compiler` names what it compiles through; `extensions` lists the extensions it claims, with dots.
* `name`, `version` and `compiler` are half of the host's cache key.

## 2. Requests

### Request

* `pub struct Request { pub id: u64, pub units: Vec<Unit> }`
* One batch, one line. Every unit shares one extension. The response repeats `id`.

### Unit

* `pub struct Unit { pub filename: String, pub path: String, pub source: String, pub options: Options, pub files: BTreeMap<String, String> }`
* `filename` is the path a diagnostic should name, relative to the project root; `path` is where the source is on disk; `files` holds the sibling files the plugin asked for with `Outcome::Needs`, keyed by the specifier it asked with; empty on the first send. Absent from the JSON when empty.

### Options

* `pub struct Options { pub production: bool, pub source_map: bool, pub minify: bool }`
* What the build asked for, in terms every plugin can honour. Every field defaults to false when absent from the JSON. `Default` is all false.

## 3. Responses

### Response

* `pub struct Response { pub id: u64, pub results: Vec<Outcome> }`
* One outcome per unit, in the order they were asked for. A response whose `id` or count differs from the request is refused by the host.

### Outcome

* `pub enum Outcome { Ok(Compiled), Failed { diagnostics: Vec<Diagnostic> }, Needs { files: Vec<String> } }`
* Tagged in JSON by `status`: `ok`, `failed` or `needs`. `Failed::diagnostics` is never empty. `Needs::files` are specifiers as the source wrote them, to be answered through `Unit::files`; a unit that answers `Needs` a second time has failed.

### Compiled

* `pub struct Compiled { pub js: String, pub lang: Lang, pub css: Option<String>, pub source_map: Option<String>, pub deps: Vec<String>, pub diagnostics: Vec<Diagnostic> }`
* `js` is the module the unit became, its imports the build's to resolve; `lang` is the dialect it is written in; `css` is every style block as one string, already scoped when the source asked; `deps` are specifiers the plugin reached for that the source's imports do not contain, a `src=` block's file among them; `diagnostics` are warnings that did not stop the compile. `Default` is an empty JavaScript module with nothing else.

### Lang

* `pub enum Lang { Js, Ts }`
* Serialized lowercase. `Default` is `Js`.

## 4. Diagnostics

### Diagnostic

* `pub struct Diagnostic { pub severity: Severity, pub message: String, pub file: Option<String>, pub line: Option<u32>, pub column: Option<u32> }`
* `Diagnostic::error(message: impl Into<String>) -> Diagnostic`, `Diagnostic::warning(message: impl Into<String>) -> Diagnostic`: a diagnostic with no place.
* `at(self, file: impl Into<String>, line: Option<u32>, column: Option<u32>) -> Diagnostic`: the same with a place. `line` and `column` are one-based.
* `file`, `line` and `column` are absent from the JSON when `None`.

### Severity

* `pub enum Severity { Error, Warning }`
* Serialized lowercase. An `Error` in a unit's diagnostics stops the build; a `Warning` is printed.

## 5. Error Handling

The crate defines no error type. A failure of a unit is `Outcome::Failed`; a failure of the plugin itself is whatever it wrote to stderr before exiting, which the host attaches to the error it reports. Serialization goes through `serde`, so a malformed line is a `serde_json::Error` on whichever side read it.
