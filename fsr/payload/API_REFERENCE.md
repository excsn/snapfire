# API Reference: snapfire_fsr_payload

The encodings of SnapFire FSR: the lossless JSON pair, the HTML serializer and the row protocol.

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
  * [OpenClient](#openclient)
  * [html_serialize](#html_serialize)
  * [escape_key](#escape_key)
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

* `pub const FORMAT_VERSION: u32`, value `3`.

### Re-exports

Everything public is re-exported from the crate root.

* `pub use html::{escape_key, html_serialize, HtmlSession, OpenClient};`
* `pub use json::{json_to_value, value_to_json, DecodeError};`
* `pub use rows::{node_to_row_json, row_json_to_node, serialize_page};`

## 2. JSON Encoding

Module `json`. Encoding is infallible. Decoding returns `Result<Value, DecodeError>`. A `Value::Map` keeps its key order through encoding and decoding.

### value_to_json

Encodes a `snapfire_fsr_core::Value` as `serde_json::Value`, tagging whatever plain JSON would lose.

* `pub fn value_to_json(value: &Value) -> serde_json::Value`

Constraints:

* `Value::Int` is a bare JSON number inside the inclusive range from `-9_007_199_254_740_991` to `9_007_199_254_740_991`. Outside that range it is a tagged decimal string.
* `Value::UInt` that fits in `i128` is encoded as `Value::Int`. Only a magnitude above `i128::MAX` is written under the `u` tag.
* `Value::F64` is a bare JSON number only when it is finite and its fractional part is non-zero.
* `Value::F32` is always tagged.
* `Value::Undefined` is `{"$": "undef"}`. A map entry holding `Value::Undefined` is left out.
* `Value::Map` is a JSON object unless the map itself holds the key `$`, in which case it is written as ordered pairs under the `m` tag.
* Keys within a tagged object are written in the order `$`, then the tag's own fields as listed in [Tag Table](#tag-table).

### json_to_value

Decodes `serde_json::Value` into a `Value`. Accepts this crate's tags and untagged JSON.

* `pub fn json_to_value(json: &serde_json::Value) -> Result<Value, DecodeError>`

Constraints:

* A JSON object is treated as a tag only when its `$` key holds a string. An object whose `$` key holds a non-string decodes as a plain `Value::Map`.
* A tag name outside the table is an error.
* A JSON number that fits `i64` or `u64` becomes `Value::Int`. Any other number becomes `Value::F64`.
* A `u` tag is decoded through `Value::uint`. A magnitude within `i128` becomes `Value::Int`.
* An `f32` tag holding a number is narrowed with an `as f32` cast.
* A `var` tag with no `p` key decodes to a payload of `None`.
* A `var` tag whose `t` is one of `snapfire_fsr_core::value::typed::ALL` decodes to a plain value, not a `Value::Variant`. Its `p` is required. A payload of another shape than the one below is an error.
  * `$map`: an array of `[key, value]` pairs, to a `Value::Map`. A key is a string, an integer, a boolean, `null` or `undefined`, keyed by its text.
  * `$set`: an array, to that `Value::Seq`.
  * `$date`: a number of milliseconds, to that number.
  * `$bigint`: an integer or a decimal string, to `Value::Int`.
  * `$url`: a string, to that `Value::Str`.
* `value_to_json` writes a typed variant like any other `var`.

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
| `undef` | none | none | `Value::Undefined` |

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

The island id counter and placed content of one response, shared by every chunk serialized for it.

* `pub struct HtmlSession { /* private */ }`. `Default`.
* `pub fn new() -> Self`
* `pub fn serialize(&mut self, node: &Node) -> String`: `write_into` on a new string, then `finish`. Content placed at a named place whose outlet is not in `node` goes to `take_placed`.
* `pub fn write_into(&mut self, node: &Node, out: &mut String)`: writes `node` at the end of `out`. Every `write_into` until the next `finish` must write to the same string.
* `pub fn finish(&mut self, out: &mut String)`: writes the content placed at each named place into its outlet in `out`, the first outlet of that name when there are several. Content whose outlet `out` does not hold goes to `take_placed`, its runs opened with `data-sf-to="{name}"`.
* `pub fn enter_segment(&mut self, key: &str) -> String`: sets the segment key that placed content written from now on is marked with. Returns the previous key.
* `pub fn take_placed(&mut self) -> String`: the placed content written since the last take. The session's placed content is then empty.
* `pub fn open_client(&mut self, module: &ModuleId, props: &Props, rendered: bool, out: &mut String) -> OpenClient`: writes an island's open tag with a fresh id, with `data-sf-ssr` when `rendered`. Until `close_client`, placed content names a rendered island by its `$k` prop when that is a string, else by its `sf-i{n}` id.
* `pub fn close_client(&mut self, open: OpenClient, props: &Props, out: &mut String)`: writes the island's close tag and its props script.
* `Default` is the same as `new`.

Constraints:

* Ids are allocated in depth-first tree order, starting at `0` for a fresh session. They take the form `sf-i{n}`.
* One session per response. Serializing two chunks of the same response through two sessions emits `sf-i0` twice.
* The counter is `u32` and only increases.

### OpenClient

An island `open_client` opened, which `close_client` takes.

* `pub struct OpenClient { /* private */ }`. `Debug`, `#[must_use]`.
* An island opened and never closed leaves the session naming it in placed content.

### html_serialize

Serializes one tree with its placed content after it.

* `pub fn html_serialize(node: &Node) -> String`: `serialize` then `take_placed` on a new session. Island ids restart at `0` on every call.

### escape_key

* `pub fn escape_key(key: &str) -> String`: `key` with `%` written as `%25` and `-` written as `%2D`. The result never contains `--`.

### Emitted Markup

What each `Node` becomes.

| Node | Markup |
| --- | --- |
| `Node::Text` | the text with `&`, `<` and `>` replaced by `&amp;`, `&lt;` and `&gt;`. Quotes are not escaped |
| `Node::Raw` | the string verbatim, with no escaping |
| `Node::Seq` | each child in order, with nothing between them |
| `Node::Client` | `<sf-i id="sf-i{n}" data-sf-module="{module}">` then the body then `</sf-i>`, followed by `<script type="application/json" data-sf-props="sf-i{n}">{props}</script>` |
| `Node::Pending` | `<div data-sf-slot="{slot}">` then the fallback then `</div>` |
| `Node::Placed` | nothing in place. Its nodes join the session's placed content for the body or the content for its named place |
| `Node::Outlet` | nothing. `finish` writes the content placed at its name here. An outlet inside placed content is not recorded |
| `Node::Slot` | nothing |

Placed content is written as runs: `<template data-sf-l="{segment key}" data-sf-island="{island}"></template>`, the content, then `<template data-sf-l-end></template>`. `data-sf-island` names the island whose own markup placed it, by its `$k` region key or else its `sf-i{n}` id. It is absent outside an island. Both attribute values have `&` and `"` escaped. Consecutive pieces with the same segment and island share one run. Content placed at a named place is written the same way inside its outlet. When `finish` finds no outlet for it, it goes to `take_placed` with `data-sf-to` naming the place.

Constraints on a `Node::Client`:

* The body is the children. When `rendered` is true the open tag carries `data-sf-ssr` and content placed inside the children names the island.
* The props script is always emitted, including for an empty props map, where the body is `{}`.
* The props JSON is `value_to_json` over `Value::Map(props)` with every `<` replaced by the JSON escape `\u003c`.
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
* In a `c` row, `ch` is treated as empty when it is absent or not an array. `r` is treated as `false` when it is missing or not a boolean.
* In a `p` row, the slot id must be a JSON unsigned integer. It is narrowed to `u32` with an `as` cast.
* A decoded `t` row produces an owned `Cow`, never a borrowed one.

### serialize_page

The wire encoding of one complete page. Infallible.

* `pub fn serialize_page(node: &Node) -> String`

Constraints:

* Emits exactly two lines, `V {"fmt":3,"enc":"json"}` and `N {tree row}`, each ending in `\n`.
* Emits no `G` line and no `S` line.

### Node Row Kinds

| Kind | Node | Row |
| --- | --- | --- |
| `t` | `Node::Text` | `["t", text]` |
| `r` | `Node::Raw` | `["r", html]` |
| `q` | `Node::Seq` | `["q", [child rows]]` |
| `c` | `Node::Client` | `["c", {"m": module id, "p": props, "ch": [child rows], "r": rendered}]` |
| `p` | `Node::Pending` | `["p", slot id, fallback row]` |
| `l` | `Node::Placed` | `["l", target, [child rows]]`, the target `body` or `#` and a place name |
| `o` | `Node::Outlet` | `["o", name]` |
| `q` | `Node::Slot` | `["q", []]`, read back as an empty `Node::Seq` |

The `p` field of a `c` row is the props map through `value_to_json`. Any form in [Tag Table](#tag-table) can appear inside it.

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

The decode error, holding its message.

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
| Row payload of the wrong shape | ``` `t` row needs a string ```, ``` `r` row needs a string ```, ``` `q` row needs an array ```, ``` `c` row needs an object ```, ``` `p` row needs a slot id ```, ``` `p` row needs a fallback ```, ``` `l` row needs a target ```, ``` `l` row needs an array ```, ``` `l` row names an unknown target `{target}` ```, ``` `o` row needs a name ```, ``` `o` row names `{name}`, which is not a place name ``` |
| `c` row field missing or wrong | ``` `c` row needs `m` ```, ``` `c` row needs `p` ```, ``` `c` row `p` must decode to a map ``` |
| Typed `var` payload of the wrong shape | ``tag `$map` field `p` must be an array of pairs``, ``tag `$map` entries must be pairs``, ``tag `$map` keys must be strings, integers, booleans or null``, ``tag `$set` field `p` must be an array``, ``tag `$date` field `p` must be a number``, ``tag `$bigint` field `p` must be an integer``, ``tag `$url` field `p` must be a string`` |
| Module id unparseable | ``module id must be `path#export` with a non-empty path and export`` |
