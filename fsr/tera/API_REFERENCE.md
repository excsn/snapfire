# API Reference: snapfire_fsr_tera

The Tera evaluator for Snapfire FSR: it renders a template to a string and splits that string into payload chunks.

## Contents

* [1. Markers](#1-markers)
  * [MARKER](#marker)
* [2. Template Functions](#2-template-functions)
  * [island](#island)
  * [slot](#slot)
  * [head](#head)
* [3. Registration](#3-registration)
  * [register_markers](#register_markers)
* [4. TeraEvaluator](#4-teraevaluator)
  * [TeraEvaluator](#teraevaluator)
* [5. Marker Grammar and Chunk Mapping](#5-marker-grammar-and-chunk-mapping)
* [6. Template Context](#6-template-context)
* [7. Constraints](#7-constraints)
* [8. Error Handling](#8-error-handling)
  * [EvalError](#evalerror)

## 1. Markers

### MARKER

The codepoint that delimits a marker token in rendered output.

* `pub const MARKER: char = '\u{F8FF}'`

A token in the output is `MARKER`, the token text, `MARKER`. The token text is `island:<base64>` or `slot:<name>`. Splitting is on the character, so a rendered string containing an odd count of `MARKER` is rejected.

## 2. Template Functions

The three functions `register_markers` installs. Each returns a string, so a template calls them in an output expression, `{{ island(...) }}`.

### island

Emits a client node at this position.

* `island(module: String, props: any = {}) -> String`
* `module` is required, a module id in `path#export` form. Absence or a non-string is a render error.
* `props` is optional and defaults to an empty object. It must serialise to JSON and must decode to a map or null; anything else fails the split.
* Emits the token `island:<base64 of {"m": module, "p": props}>`.
* Produces `Chunk::Node(Node::Client { module, props, children: Vec::new(), ssr: None })`. Islands emitted here always have empty `children` and no `ssr` subtree.

### slot

Emits a hole for the plan child registered under this name.

* `slot(name: String) -> String`
* `name` is required. It must be non-empty and every character must be an ASCII alphanumeric, `_` or `-`; otherwise the render fails with ``invalid slot name `{name}` ``.
* Emits the token `slot:<name>`, which becomes `Chunk::Slot(SlotName(name))`.
* `head` is reserved: `slot(name="head")` is accepted by the validator and resolves like [`head`](#head).

### head

Emits the reserved head slot.

* `head() -> String`
* Takes no arguments. Any supplied are ignored.
* Emits the token `slot:head`, which becomes `Chunk::Slot(SlotName("head"))`. The assembler substitutes the head node passed to `assemble`.

## 3. Registration

### register_markers

Installs `island`, `slot` and `head` on a Tera instance, overwriting any function already registered under those names.

* `pub fn register_markers(tera: &mut Tera)`
* Must run before any template using those names is added to the instance. Tera 2 resolves function names when a template is parsed, so `add_raw_templates` on a template calling `island`, `slot` or `head` fails if this has not run yet.

## 4. TeraEvaluator

### TeraEvaluator

An `Evaluator` backed by an owned Tera instance.

* `pub fn new(tera: Tera) -> Self` takes ownership and calls `register_markers` on the instance. That registration reaches only templates added afterwards; templates already loaded were parsed before it.
* `fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks` renders the template named `module.path` and splits the result. `module.export` takes no part in template lookup; it is carried only inside island module ids.
* The returned stream is built from a fully rendered string: the template completes before the first chunk is yielded, and the stream never blocks. On failure the stream holds exactly one `Err(EvalError)`.
* Every chunk is `Chunk::Node` or `Chunk::Slot`. This evaluator never produces `Node::Pending`; deferral belongs to the assembler.
* `TeraEvaluator: Send + Sync` through the `Evaluator` bound, so one instance serves concurrent requests behind an `Arc`.

## 5. Marker Grammar and Chunk Mapping

Rendered output splits on `MARKER` into alternating segments. Even-indexed segments are literal output, odd-indexed segments are tokens.

| Rendered segment | Chunk |
| --- | --- |
| literal text, non-empty | `Chunk::Node(Node::raw(text))` |
| literal text, empty | dropped, no chunk |
| `island:<base64>` | `Chunk::Node(Node::Client { .. })` |
| `slot:<name>` | `Chunk::Slot(SlotName(name))` |
| `slot:head` | `Chunk::Slot(SlotName("head"))` |
| anything else | error, ``unknown marker token `{token}` `` |

An even number of segments means an odd count of `MARKER` in the output, which is rejected as unbalanced before any token is interpreted.

The island payload is JSON with two keys: `m` is the module id string and `p` is the props. `p` decodes through `snapfire_fsr_payload::json_to_value`; a `Value::Map` becomes the props, `Value::Null` becomes an empty map, any other kind is an error.

## 6. Template Context

Every key of the `Data` map passed to `evaluate` becomes a top-level Tera variable of the same name, converted with `snapfire_fsr_payload::value_to_json`.

The assembler injects three before calling the evaluator, and they are present in fallback and error modules too:

| Variable | Type | Present when |
| --- | --- | --- |
| `params` | map of string to string | always, empty when the route has no parameters |
| `identity` | map with `subject` (string) and `claims` (map) | the request's session carries an identity |
| `csrf_token` | string | the request context has a CSRF token |

Error modules additionally receive `error`, the failure message as a string. Fallback and error modules receive no data source output.

`value_to_json` is lossless, not idiomatic: any value whose JSON form would be ambiguous becomes a tagged object carrying a `$` key naming the tag. A float lands in the tagged form more often than expected, since an integral `F64` such as `12.0` is tagged to keep it distinct from an integer.

| Value | Seen by the template |
| --- | --- |
| `Bool`, `Str`, `Null` | a plain JSON scalar |
| `Int` or `UInt` within +/- (2^53 - 1) | a plain JSON number |
| `Int` or `UInt` beyond that range | `{"$": "i"}` or `{"$": "u"}` with `v` a decimal string |
| `F64` with a fractional part | a plain JSON number |
| `F64` that is integral, infinite or NaN | `{"$": "f", "v": <number or "nan", "inf", "-inf">}` |
| `F32` | `{"$": "f32", "v": <number or "nan", "inf", "-inf">}` |
| `Seq`, `Map` | a JSON array or object |
| `Bytes` | `{"$": "b", "v": "<base64>"}` |
| `TypedArray` | `{"$": "ta", "k": "<kind>", "v": "<base64, little-endian>"}` where kind is one of `i8` `u8` `i16` `u16` `i32` `u32` `i64` `u64` `f32` `f64` |
| `Variant` | `{"$": "var", "t": "<tag>"}` plus `p` when the variant carries a payload |
| `Ref` | `{"$": "ref", "k": "action" or "module", "id": "<id>"}` |
| a `Map` that already holds a `$` key | `{"$": "m", "v": [[key, value], ...]}` |

A tagged value is opaque to template syntax but survives a round trip: passing one as `props` to `island` decodes it back to the original `Value`.

## 7. Constraints

Each of these is silent or late unless the caller knows about it.

* `register_markers` must run before `add_raw_templates` on the same instance. `TeraEvaluator::new` registering them is not a substitute for templates that are already loaded.
* Every `slot(name=...)` in a template must have a plan child registered under that exact name, otherwise assembly fails with `AssembleError::MissingSlot`. The template side is not checked at parse time.
* Fallback and error modules may not emit any slot, `head()` included. A slot chunk from one fails assembly with `AssembleError::SlotInFallback`.
* A segment whose subtree emitted the `head` slot is never written to the node cache, even when its plan node carries a cache key.
* `evaluate` renders the whole template before yielding, so template work is not interleaved with downstream consumption of the stream.
* Slot names are restricted to `[A-Za-z0-9_-]+`. Island module ids are not restricted by this crate beyond parsing as `path#export`.

## 8. Error Handling

Failures divide by phase. Tera raises the first group while rendering, and `evaluate` forwards the message unchanged inside an `EvalError`. The split raises the second group after rendering succeeded.

### EvalError

The error type of the chunk stream, re-exported from `snapfire_fsr_runtime`.

* `pub struct EvalError { pub module: String, pub message: String }`
* `impl Display` renders as `evaluate {module}: {message}`.
* `module` is the failing module's `path#export` form, not the template name alone.
* There are no variants. Callers log or propagate; the assembler wraps it as `AssembleError::Eval`.

Render-phase messages, raised by the template functions:

| Message | Cause |
| --- | --- |
| Tera's own error text | unknown template, missing required argument, bad type, any template failure |
| ``invalid slot name `{name}` `` | `slot` argument empty or outside `[A-Za-z0-9_-]+` |
| `island props are not serializable: {e}` | the `props` expression will not convert to JSON |

Split-phase messages, all raised with the module in `EvalError::module`:

| Message | Cause |
| --- | --- |
| `unbalanced marker delimiters in rendered output` | odd count of `MARKER` in the rendered string |
| ``unknown marker token `{token}` `` | a token with neither the `island:` nor the `slot:` prefix |
| `island marker holds invalid base64` | the token body is not valid base64 |
| `island marker holds invalid json: {e}` | the decoded bytes are not JSON |
| `island marker missing module` | the payload has no string `m` key |
| `island module id: {e}` | `m` does not parse as `path#export` |
| `island props: {e}` | `p` fails to decode from the lossless JSON form |
| `island props must be a map` | `p` decodes to something other than a map or null |
