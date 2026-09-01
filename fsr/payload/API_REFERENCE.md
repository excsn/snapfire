# API Reference: snapfire_fsr_payload

The encodings of Snapfire FSR: the lossless JSON pair, the HTML serializer and the row protocol.

## Contents

* [1. Crate Root](#1-crate-root)
  * [FORMAT_VERSION](#format_version)
  * [Re-exports](#re-exports)
* [2. JSON Encoding](#2-json-encoding)
  * [value_to_json](#value_to_json)
  * [json_to_value](#json_to_value)
  * [Tag Table](#tag-table)
  * [Typed Array Kinds](#typed-array-kinds)
* [3. HTML Serialization](#3-html-serialization)
  * [HtmlSession](#htmlsession)
  * [html_serialize](#html_serialize)
  * [Emitted Markup](#emitted-markup)
* [4. Row Protocol](#4-row-protocol)
  * [node_to_row_json](#node_to_row_json)
  * [row_json_to_node](#row_json_to_node)
  * [serialize_page](#serialize_page)
  * [Node Row Kinds](#node-row-kinds)
  * [Response Line Tags](#response-line-tags)
* [5. Error Handling](#5-error-handling)
  * [DecodeError](#decodeerror)
  * [Decode Failure Messages](#decode-failure-messages)

## 1. Crate Root

Modules: `html`, `json`, `rows`.

### FORMAT_VERSION

The wire format announced in a response's `V` line.

* `pub const FORMAT_VERSION: u32`, value `1`.

### Re-exports

Everything public is re-exported from the crate root.

* `pub use html::{html_serialize, HtmlSession};`
* `pub use json::{json_to_value, value_to_json, DecodeError};`
* `pub use rows::{node_to_row_json, row_json_to_node, serialize_page};`

## 2. JSON Encoding

Module `json`. Encoding is infallible; decoding returns `Result<Value, DecodeError>`. `serde_json` is compiled with `preserve_order`, so a `Value::Map` keeps its `IndexMap` order through the encoding and back.

### value_to_json

Encodes a `snapfire_fsr_core::Value` as `serde_json::Value`, tagging whatever plain JSON would lose.

* `pub fn value_to_json(value: &Value) -> serde_json::Value`

Constraints:

* `Value::Int` is a bare JSON number inside the inclusive range from `-9_007_199_254_740_991` to `9_007_199_254_740_991`; outside that range it is a tagged decimal string.
* `Value::UInt` that fits in `i128` is encoded as if it were `Value::Int`; only a magnitude above `i128::MAX` reaches the `u` tag.
* `Value::F64` is a bare JSON number only when it is finite and its fractional part is non-zero.
* `Value::F32` is always tagged.
* `Value::Map` is a JSON object unless the map itself holds the key `$`, in which case it is written as ordered pairs under the `m` tag.
* Keys within a tagged object are written in the order `$`, then the tag's own fields as listed in [Tag Table](#tag-table).

### json_to_value

Decodes `serde_json::Value` into a `Value`, reading both this crate's tags and JSON written elsewhere.

* `pub fn json_to_value(json: &serde_json::Value) -> Result<Value, DecodeError>`

Constraints:

* A JSON object is treated as a tag only when its `$` key holds a string. An object whose `$` key holds a non-string decodes as a plain `Value::Map`.
* A tag name outside the table is an error, not a map.
* A JSON number that fits `i64` or `u64` becomes `Value::Int`; any other number becomes `Value::F64`.
* A `u` tag is decoded through `Value::uint`, so a magnitude within `i128` normalises back to `Value::Int`.
* An `f32` tag holding a number is narrowed with an `as f32` cast.
* A `var` tag with no `p` key decodes to a payload of `None`.

### Tag Table

Every tagged form `value_to_json` emits and `json_to_value` accepts.

| Tag | Fields | Payload | Decodes to |
| --- | --- | --- | --- |
| `i` | `v` | decimal string, may be negative | `Value::Int` |
| `u` | `v` | decimal string | `Value::UInt`, normalised to `Value::Int` when it fits |
| `f` | `v` | `"nan"`, `"inf"`, `"-inf"` or a JSON number | `Value::F64` |
| `f32` | `v` | `"nan"`, `"inf"`, `"-inf"` or a JSON number | `Value::F32` |
| `b` | `v` | standard base64 with padding | `Value::Bytes` |
| `ta` | `k`, `v` | element kind, then the elements as little-endian bytes in base64 | `Value::TypedArray` |
| `m` | `v` | array of two-element `[key, value]` arrays, keys being strings | `Value::Map` |
| `var` | `t`, optional `p` | variant tag, then the encoded payload | `Value::Variant` |
| `ref` | `k`, `id` | `"action"` or `"module"`, then the identifier | `Value::Ref` |

Untagged forms: JSON `null`, `true` and `false`, strings, arrays, objects with no `$` key, integers inside the safe range and finite non-integral numbers.

### Typed Array Kinds

The `k` field of a `ta` tag, with the element width each implies for the base64 payload.

| Kind | Element | Bytes per element |
| --- | --- | --- |
| `i8` | `i8` | 1 |
| `u8` | `u8` | 1 |
| `i16` | `i16` | 2 |
| `u16` | `u16` | 2 |
| `i32` | `i32` | 4 |
| `u32` | `u32` | 4 |
| `i64` | `i64` | 8 |
| `u64` | `u64` | 8 |
| `f32` | `f32` | 4 |
| `f64` | `f64` | 8 |

Decoding fails when the decoded byte length is not a multiple of the element width or when the kind is not in this table.

## 3. HTML Serialization

Module `html`. Both entry points return a `String` and cannot fail.

### HtmlSession

Holds the island id counter for one response, so ids stay unique across chunks that are serialized separately.

* `pub fn new() -> Self`
* `pub fn serialize(&mut self, node: &Node) -> String`
* Also implements `Default`; `new` and `Default::default` are the same thing.

Constraints:

* Ids are allocated in depth-first tree order, starting at `0` for a fresh session. They take the form `sf-i{n}`.
* One session per response. Serializing two chunks of the same response through two sessions emits `sf-i0` twice.
* The counter is `u32` and only increases; a session is never rewound.

### html_serialize

One-shot form for a tree with no streamed continuation.

* `pub fn html_serialize(node: &Node) -> String`

Constraints:

* Equivalent to `HtmlSession::new().serialize(node)`, so island ids restart at `0` on every call.

### Emitted Markup

What each `Node` becomes.

| Node | Markup |
| --- | --- |
| `Node::Text` | the text with `&`, `<` and `>` replaced by `&amp;`, `&lt;` and `&gt;`. Quotes are not escaped |
| `Node::Raw` | the string verbatim, with no escaping |
| `Node::Seq` | each child in order, with nothing between them |
| `Node::Client` | `<sf-i id="sf-i{n}" data-sf-module="{module}">` then the body then `</sf-i>`, followed by `<script type="application/json" data-sf-props="sf-i{n}">{props}</script>` |
| `Node::Pending` | `<div data-sf-slot="{slot}">` then the fallback then `</div>` |

Constraints on a `Node::Client`:

* The body is the `ssr` tree when there is one, otherwise the children. When `ssr` is `Some`, the children are not emitted at all.
* The props script is always emitted, including for an empty props map, where the body is `{}`.
* The props JSON is `value_to_json` over `Value::Map(props)` with every `<` replaced by the JSON escape `\u003c`, so no string can terminate the script element.
* `module` is written into the attribute in its `Display` form, `path` then `#` then `export`, without attribute escaping.
* `slot` is written as its `u32`.

## 4. Row Protocol

Module `rows`. A response is newline-terminated lines, each a one-letter line tag, a space, then JSON.

### node_to_row_json

Encodes one `Node` tree as a single row value. Infallible.

* `pub fn node_to_row_json(node: &Node) -> serde_json::Value`

### row_json_to_node

Reads a row value back into a `Node`.

* `pub fn row_json_to_node(json: &serde_json::Value) -> Result<Node, DecodeError>`

Constraints:

* The row must be a JSON array whose first element is a string naming a kind in [Node Row Kinds](#node-row-kinds).
* In a `c` row, `m` must parse as `path#export` with both halves non-empty. `p` must be present and must decode to a `Value::Map`.
* In a `c` row, `ch` is treated as empty when it is absent or not an array. `s` is treated as absent when it is missing or `null`.
* In a `p` row, the slot id must be a JSON unsigned integer; it is narrowed to `u32` with an `as` cast.
* A decoded `t` row produces an owned `Cow`, never a borrowed one.

### serialize_page

The wire encoding of one complete page: a version line, then the tree line. Infallible.

* `pub fn serialize_page(node: &Node) -> String`

Constraints:

* Emits exactly two lines, `V {"fmt":1,"enc":"json"}` and `N {tree row}`, each ending in `\n`.
* Emits no `G` line and no `S` line. A page with unresolved slots needs the streaming writer in `snapfire_fsr_runtime`.

### Node Row Kinds

| Kind | Node | Row |
| --- | --- | --- |
| `t` | `Node::Text` | `["t", text]` |
| `r` | `Node::Raw` | `["r", html]` |
| `q` | `Node::Seq` | `["q", [child rows]]` |
| `c` | `Node::Client` | `["c", {"m": module id, "p": props, "ch": [child rows], "s": ssr row or null}]` |
| `p` | `Node::Pending` | `["p", slot id, fallback row]` |

The `p` field of a `c` row is the props map through `value_to_json`, so every form in [Tag Table](#tag-table) can appear inside it.

### Response Line Tags

| Line | Content | Written by |
| --- | --- | --- |
| `V` | `{"fmt": FORMAT_VERSION, "enc": "json"}` | `serialize_page` and the runtime stream writer |
| `N` | the tree row | `serialize_page` and the runtime stream writer |
| `G` | the segment sidecar for the navigator | `snapfire_fsr_runtime` |
| `S` | a slot id, a space, then the row resolving it | `snapfire_fsr_runtime` |

`S` lines arrive in completion order, not slot order. A resolution may itself contain further `p` rows whose slots resolve in later lines.

## 5. Error Handling

### DecodeError

The single error type of this crate, carrying the message and nothing else. Encoding never produces one.

* `pub struct DecodeError(pub String)`
* Derives `Debug`, `Clone`, `PartialEq` and `Eq`.
* `impl fmt::Display for DecodeError`, formatting as `payload json decode: {0}`.
* `impl std::error::Error for DecodeError`, with no `source`.

Returned by `json_to_value` and `row_json_to_node`.

### Decode Failure Messages

The message text a caller may see, by cause.

| Cause | Message |
| --- | --- |
| Tag name not in the table | ``unknown tag `{name}` `` |
| Required field absent | ``tag `{tag}` missing field `{name}` `` |
| Field is not a string where one is required | ``tag `{tag}` field `{name}` must be a string`` |
| `i` or `u` payload is not a decimal integer | ``tag `i` holds a non-integer``, ``tag `u` holds a non-integer`` |
| `f` or `f32` payload is a string that is not `nan`, `inf` or `-inf` | ``tag `f` unknown symbol `{s}` ``, ``tag `f32` unknown symbol `{s}` `` |
| `f` or `f32` payload is neither a string nor a number | ``tag `f` field `v` must be a number``, ``tag `f32` field `v` must be a number`` |
| `b` or `ta` payload is not valid base64 | ``tag `b` holds invalid base64``, ``tag `ta` holds invalid base64`` |
| Typed array kind unknown | ``unknown typed array kind `{kind}` `` |
| Typed array length is not a whole number of elements | ``typed array `{kind}` byte length {n} not a multiple of {width}`` |
| `m` payload is not an array of string-keyed pairs | ``tag `m` field `v` must be an array``, ``tag `m` entries must be pairs``, ``tag `m` keys must be strings`` |
| Ref kind is neither `action` nor `module` | ``unknown ref kind `{kind}` `` |
| Number outside every JSON representation | ``unrepresentable number`` |
| Row is not an array or has no kind | ``node row must be an array``, ``node row missing kind`` |
| Row kind not in the table | ``unknown node row kind `{kind}` `` |
| Row payload of the wrong shape | ``` `t` row needs a string ```, ``` `r` row needs a string ```, ``` `q` row needs an array ```, ``` `c` row needs an object ```, ``` `p` row needs a slot id ```, ``` `p` row needs a fallback ``` |
| `c` row field missing or wrong | ``` `c` row needs `m` ```, ``` `c` row needs `p` ```, ``` `c` row `p` must decode to a map ``` |
| Module id unparseable | ``module id must be `path#export` with a non-empty path and export`` |
