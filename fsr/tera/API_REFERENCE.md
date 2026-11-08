# API Reference: snapfire_fsr_tera

The Tera evaluator for SnapFire FSR: it renders a template to a string and splits that string into payload chunks.

## Contents

* [1. Markers](#1-markers)
  * [`MARKER`](#marker)
* [2. Template Functions](#2-template-functions)
  * [`island`](#island)
  * [`on`](#on)
  * [`slot`](#slot)
  * [`head`](#head)
* [3. Registration](#3-registration)
  * [`register_markers`](#register_markers)
* [4. The Evaluator](#4-the-evaluator)
  * [`TeraEvaluator`](#teraevaluator)
* [5. Marker Grammar and Chunk Mapping](#5-marker-grammar-and-chunk-mapping)
  * [Segments](#segments)
  * [Island payload](#island-payload)
* [6. Template Context](#6-template-context)
  * [Variables](#variables)
  * [Value conversion](#value-conversion)
* [7. Error Handling](#7-error-handling)
  * [`EvalError`](#evalerror)
  * [Render-phase messages](#render-phase-messages)
  * [Split-phase messages](#split-phase-messages)

## 1. Markers

### `MARKER`

The codepoint that delimits a marker token in rendered output.

* `pub const MARKER: char = '\u{F8FF}'`
* A token is `MARKER`, the token text, `MARKER`. The token text is `island:<base64>` or `slot:<name>`.
* A rendered string with an odd count of `MARKER` is rejected.

## 2. Template Functions

The four functions `register_markers` installs. Each is called in an output expression, `{{ island(...) }}`.

### `island`

Emits a client node at this position.

* `island(module: String, props: any = {}, state: any?, key: String?, when: String?, mode: String?) -> String`
* `module` is required and must parse as `path#export`.
* `props` must serialise to JSON and decode to a map or null.
* `key` is written into the markup as `data-sf-region` and into the props as `$k`.
* `when` is `"load"`, `"visible"` or `"idle"`. Any other value is a render error.
* `mode` is `"browser"` or `"server"`. `"browser"` is the default and is not written into the payload. Any other value is a render error.
* `state` is written into the props under `$s`. It needs `props` to be a map.
* Emits the token `island:<base64 of {"m": module, "p": props, "w": when, "d": mode, "k": key}>`.
* Produces `Chunk::Node(Node::Client { module, props, children, rendered })`.
* When `key`, `when` or `mode = "server"` is given, the client node is wrapped in raw `<sf-s data-sf-island ...>` and `</sf-s>` chunks carrying `data-sf-region`, `data-sf-when` and `data-sf-mode`.
* `children` is empty and `rendered` is `false`, except for a server-mode island whose `module.path` names a template this instance holds.
* That template is rendered in place with `snapfire_fsr_runtime::island_data(props, props["$s"])` as its context. `children` holds the result and `rendered` is `true`.
* A slot inside such a template is an error. A server-mode island inside one is not rendered.

### `on`

Writes the `data-sf-on` attribute that binds an island's handlers.

* `on(<event>=<handler>, ...) -> String`
* `on(click="filter", input="search")` returns `data-sf-on="click:filter input:search"`.
* At least one event is required.
* A handler name is non-empty ASCII alphanumerics, `_`, `-` or `.`. A value that is not a string is a render error naming its event.
* The result is a safe string and is not escaped.
* No check is made that a handler of that name exists.

### `slot`

Emits a hole for the plan child registered under this name.

* `slot(name: String) -> String`
* `name` is required, non-empty and made of ASCII alphanumerics, `_` or `-`.
* Emits the token `slot:<name>`, which becomes `Chunk::Slot(SlotName(name))`.
* `slot(name="head")` is the same as `head()`.

### `head`

Emits the reserved head slot.

* `head() -> String`
* Arguments are ignored.
* Emits the token `slot:head`, which becomes `Chunk::Slot(SlotName("head"))`. The assembler fills it with the head node passed to `assemble`.

## 3. Registration

### `register_markers`

Installs `island`, `on`, `slot` and `head` on a Tera instance, replacing any function already registered under those names.

* `pub fn register_markers(tera: &mut Tera)`
* Must run before any template that calls those functions is added. Tera 2 resolves function names when a template is parsed, so `add_raw_templates` fails otherwise.

## 4. The Evaluator

### `TeraEvaluator`

An `Evaluator` backed by an owned Tera instance. `Send + Sync`.

* `pub fn new(tera: Tera) -> Self`: takes ownership and calls `register_markers`. Templates already added to `tera` are not affected.
* `pub fn has(&self, name: &str) -> bool`: whether a template of that name was added.
* `fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks`: renders the template named `module.path` and splits the result.
  * `module.export` is not used for template lookup.
  * The whole template renders before the first chunk is yielded.
  * On failure the stream holds exactly one `Err(EvalError)`.
  * Every chunk is `Chunk::Node` or `Chunk::Slot`. `Node::Pending` is never produced.

## 5. Marker Grammar and Chunk Mapping

### Segments

Rendered output splits on `MARKER`. Even-indexed segments are literal output and odd-indexed segments are tokens.

| Rendered segment | Chunk |
| --- | --- |
| literal text, non-empty | `Chunk::Node(Node::raw(text))` |
| literal text, empty | none |
| `island:<base64>` | `Chunk::Node(Node::Client { .. })`, wrapped as described under [`island`](#island) |
| `slot:<name>` | `Chunk::Slot(SlotName(name))` |
| anything else | error, ``unknown marker token `{token}` `` |

An even number of segments is rejected as unbalanced before any token is read.

### Island payload

JSON with five keys.

* `m`: the module id string.
* `p`: the props, decoded with `snapfire_fsr_payload::json_to_value`. `Value::Map` becomes the props, `Value::Null` becomes an empty map and anything else is an error.
* `k`: the region key or null.
* `w`: the timing or null.
* `d`: `"server"` or null.

## 6. Template Context

### Variables

* Every key of the `Data` passed to `evaluate` becomes a top-level Tera variable of the same name.
* The assembler adds request props such as `params`, `identity` and `csrf_token` before calling the evaluator. The Evaluation section of the [runtime reference](../runtime/API_REFERENCE.md) lists each one with the condition for it.
* A server-mode island template rendered in place gets the placement's props without `$s` and `$k`. It also gets `state`.
* Map keys keep insertion order. The crate enables Tera's `preserve_order` feature.

### Value conversion

| Value | Seen by the template |
| --- | --- |
| `Bool`, `Str` | a JSON scalar |
| `Null`, `Undefined` | `null` |
| `Int` or `UInt` within `i64` or `u64` | a JSON number |
| `Int` or `UInt` beyond that | a decimal string |
| `F64` or `F32`, whole and below 2^53 in magnitude | a JSON integer, `2` for `2.0` |
| `F64` or `F32`, any other finite value | a JSON number |
| `F64` or `F32`, infinite or NaN | `null` |
| `Seq` | a JSON array |
| `Map` | a JSON object, with `Undefined` entries left out |
| `Bytes` | a base64 string |
| `TypedArray` | `{"$": "ta", "k": "<kind>", "v": "<base64, little-endian>"}`, kind one of `i8` `u8` `i16` `u16` `i32` `u32` `i64` `u64` `f32` `f64` |
| `Variant` | `{"$": "var", "t": "<tag>"}`, plus `p` when the variant carries a payload |
| `Ref` | `{"$": "ref", "k": "action" or "module", "id": "<id>"}` |

* The three tagged forms are the payload encoder's. Passed back as `props` to `island`, one decodes to the original `Value`.

## 7. Error Handling

### `EvalError`

The error type of the chunk stream, re-exported from `snapfire_fsr_runtime`. `Debug`, `Clone`, `std::error::Error`. `#[non_exhaustive]`.

* `pub struct EvalError { pub module: String, pub message: String }`
* `pub fn new(module: String, message: String) -> Self`
* `Display`: `evaluate {module}: {message}`.
* `module` is the failing module in `path#export` form. For an island template rendered in place, it is the island's module.
* The assembler wraps it as `AssembleError::Eval`.

Related assembly errors:

* A `slot(name=...)` with no plan child under that name fails assembly with `AssembleError::MissingSlot`. Templates are not checked when parsed.
* A slot, `head()` included, in a fallback or error module fails assembly with `AssembleError::SlotInFallback`.

### Render-phase messages

Raised while Tera renders. `EvalError::message` holds Tera's error text, which contains the message.

| Message | Cause |
| --- | --- |
| Tera's own text | unknown template, missing required argument, wrong type, any other template failure |
| ``invalid slot name `{name}` `` | `slot` name empty or outside `[A-Za-z0-9_-]+` |
| `island props are not serializable: {e}` | `props` does not convert to JSON |
| `island state is not serializable: {e}` | `state` does not convert to JSON |
| `island props must be an object to carry state` | `state` given with `props` that is not a map |
| ``` `when` is "load", "visible" or "idle", not `{when}` ``` | `when` out of range |
| ``` `mode` is "browser" or "server", not `{mode}` ``` | `mode` out of range |
| ``invalid handler name `{name}` `` | `on` handler name empty or outside `[A-Za-z0-9_.-]+` |
| ``the handler for `{event}` is a name, written out`` | `on` handler value is not a string |
| ``` `on` takes an event, `on(click="increment")` ``` | `on()` with no arguments |

### Split-phase messages

Raised after rendering succeeded.

| Message | Cause |
| --- | --- |
| `unbalanced marker delimiters in rendered output` | odd count of `MARKER` |
| ``unknown marker token `{token}` `` | a token without the `island:` or `slot:` prefix |
| `island marker holds invalid base64` | the token body is not base64 |
| `island marker holds invalid json: {e}` | the decoded bytes are not JSON |
| `island marker missing module` | no string `m` key |
| `island module id: {e}` | `m` does not parse as `path#export` |
| `island props: {e}` | `p` does not decode |
| `island props must be a map` | `p` decodes to something other than a map or null |
| ``the slot `{name}` inside an island, which has no child to fill`` | a slot in an island template rendered in place |
