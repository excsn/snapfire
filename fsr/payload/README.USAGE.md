# Usage Guide: snapfire_fsr_payload

How to encode Snapfire FSR values and payload trees, what every tag on the wire means and how to keep a streamed response consistent across its chunks.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [Encoding a value and reading it back](#encoding-a-value-and-reading-it-back)
  * [Rendering a tree to HTML](#rendering-a-tree-to-html)
  * [Writing a page as wire rows](#writing-a-page-as-wire-rows)
* [Encoding Values to JSON](#encoding-values-to-json)
* [Decoding JSON to Values](#decoding-json-to-values)
* [Carrying Wide Integers](#carrying-wide-integers)
* [Carrying Floats and Non-Finite Values](#carrying-floats-and-non-finite-values)
* [Carrying Bytes and Typed Arrays](#carrying-bytes-and-typed-arrays)
* [Escaping a Map With a Dollar Key](#escaping-a-map-with-a-dollar-key)
* [Encoding Variants and Refs](#encoding-variants-and-refs)
* [Serializing a Node Tree to HTML](#serializing-a-node-tree-to-html)
* [Keeping Island Ids Unique Across Chunks](#keeping-island-ids-unique-across-chunks)
* [Encoding Nodes as Wire Rows](#encoding-nodes-as-wire-rows)
* [Decoding Wire Rows Back to Nodes](#decoding-wire-rows-back-to-nodes)
* [Streaming Slot Resolutions](#streaming-slot-resolutions)
* [Checking the Format Version](#checking-the-format-version)
* [Choosing Between HTML and Rows](#choosing-between-html-and-rows)
* [Error Handling](#error-handling)

## Core Concepts

* **Value**: the data model from `snapfire_fsr_core`, wider than JSON: `i128` and `u128` integers, `f32` separate from `f64`, byte strings, typed arrays, variants and refs.
* **ValueMap**: an `IndexMap<String, Value>`, so a map has an order and encoding preserves it.
* **Node**: the payload tree, five shapes: `Text`, `Raw`, `Seq`, `Client` and `Pending`.
* **Island**: a `Node::Client`, a module the browser mounts over server-rendered markup, addressed in the DOM by an `sf-i` element.
* **Slot**: a `Node::Pending`, a hole in the tree whose content is resolved later, identified by a `SlotId`.
* **Tag**: an object of the form `{"$": "name", ...}` in the JSON encoding, marking a value JSON cannot carry natively.
* **Lossless pair**: `value_to_json` and `json_to_value`, which return the same value that went in, tags being how they manage it.
* **Foreign JSON**: JSON this crate did not write, for instance an API response; `json_to_value` accepts it, mapping plain shapes onto plain `Value`s.
* **HTML encoding**: the response as markup, with island props in sibling `<script type="application/json">` tags for the browser to pick up.
* **Row**: the wire encoding of one node, a JSON array whose first element is a one-letter kind.
* **Wire response**: newline-terminated rows, one per line, each line a one-letter row tag then a space then JSON.
* **HtmlSession**: the counter that hands out island ids, held across every chunk of one response so no two islands collide.
* **FORMAT_VERSION**: the integer announced in the `V` row, currently `1`.
* **Fingerprint**: `snapfire_fsr_core`'s canonical hash, which is how the round trip tests compare a value with its decoded self.

## Quick Start

### Encoding a value and reading it back

```rust
use snapfire_fsr_core::{Fingerprint, TypedArray, Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, value_to_json};

let mut props = ValueMap::new();
props.insert("name".to_owned(), Value::str("web-1"));
props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
props.insert("onSave".to_owned(), Value::action_ref("saveServer"));
let value = Value::Map(props);

let text = value_to_json(&value).to_string();
let reparsed: serde_json::Value = serde_json::from_str(&text).unwrap();
let decoded = json_to_value(&reparsed).unwrap();

assert_eq!(value.fingerprint(), decoded.fingerprint());
```

### Rendering a tree to HTML

```rust
use snapfire_fsr_core::{ModuleId, Node, TypedArray, Value, ValueMap};
use snapfire_fsr_payload::html_serialize;

let mut props = ValueMap::new();
props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));

let page = Node::Seq(vec![
  Node::raw("<main><h1>Servers</h1>"),
  Node::Client {
    module: ModuleId::new("components/ServerChart.tsx", "default"),
    props,
    children: Vec::new(),
    ssr: None,
  },
  Node::raw("</main>"),
]);

let html = html_serialize(&page);
assert!(html.contains("<sf-i id=\"sf-i0\" data-sf-module=\"components/ServerChart.tsx#default\">"));
```

### Writing a page as wire rows

```rust
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::serialize_page;

let page = Node::Seq(vec![Node::raw("<main>"), Node::text("hello"), Node::raw("</main>")]);
let wire = serialize_page(&page);

assert!(wire.starts_with("V {\"fmt\":1,\"enc\":\"json\"}\n"));
assert!(wire.contains("N [\"q\","));
```

## Encoding Values to JSON

`value_to_json` writes plain JSON wherever plain JSON is exact and a tagged object wherever it is not.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::value_to_json;

assert_eq!(value_to_json(&Value::Null).to_string(), "null");
assert_eq!(value_to_json(&Value::Bool(true)).to_string(), "true");
assert_eq!(value_to_json(&Value::int(7i64)).to_string(), "7");
assert_eq!(value_to_json(&Value::str("hello")).to_string(), "\"hello\"");
assert_eq!(value_to_json(&Value::F64(2.5)).to_string(), "2.5");
```

Every tag the encoder can emit, with the shape it carries:

| Tag | Emitted for | Shape |
| --- | --- | --- |
| `i` | `Value::Int` outside 2^53 - 1 in either direction | `{"$":"i","v":"-170141183460469231731687303715884105728"}` |
| `u` | `Value::UInt`, which only exists above `i128::MAX` | `{"$":"u","v":"340282366920938463463374607431768211455"}` |
| `f` | `Value::F64` that is not finite, plus `Value::F64` whose fraction is zero | `{"$":"f","v":"nan"}`, `{"$":"f","v":"inf"}`, `{"$":"f","v":"-inf"}`, `{"$":"f","v":1.0}` |
| `f32` | every `Value::F32` | `{"$":"f32","v":"nan"}`, `{"$":"f32","v":2.5}` |
| `b` | `Value::Bytes` | `{"$":"b","v":"AAEC/w=="}` |
| `ta` | `Value::TypedArray` | `{"$":"ta","k":"f64","v":"AAAAAAAA8D8AAAAAAAAEQAAAAAAAAAhA"}` |
| `m` | `Value::Map` that itself holds a `$` key | `{"$":"m","v":[["$","not a tag"],["other",1]]}` |
| `var` | `Value::Variant` | `{"$":"var","t":"Down"}`, `{"$":"var","t":"Retrying","p":3}` |
| `ref` | `Value::Ref` | `{"$":"ref","k":"action","id":"saveServer"}` |

The `$` key is always written first and the remaining keys follow in the order listed above, so the output is byte-stable.

## Decoding JSON to Values

`json_to_value` reads the tags back and reads JSON that carries no tags at all.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::json_to_value;

let foreign: serde_json::Value = serde_json::from_str(r#"{"a":[1,2.5,null,"x",true]}"#).unwrap();
let Value::Map(map) = json_to_value(&foreign).unwrap() else { panic!() };
let Value::Seq(items) = &map["a"] else { panic!() };

assert_eq!(items[0], Value::Int(1));
assert_eq!(items[1], Value::F64(2.5));
assert_eq!(items[2], Value::Null);
```

A plain JSON integer becomes `Value::Int`, a plain number that is not an integer becomes `Value::F64` and a plain object becomes `Value::Map`. An object is treated as a tag only when its `$` key holds a string; an unrecognised tag name is an error rather than a map.

## Carrying Wide Integers

Integers inside 2^53 - 1 ride as JSON numbers. Wider ones ride as decimal strings, because a JSON number that far out is not exact in a browser.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::value_to_json;

assert_eq!(value_to_json(&Value::int(9_007_199_254_740_991i64)).to_string(), "9007199254740991");
assert_eq!(
  value_to_json(&Value::int(9_007_199_254_740_992i64)).to_string(),
  "{\"$\":\"i\",\"v\":\"9007199254740992\"}"
);
```

`Value::UInt` is only reached above `i128::MAX`; anything smaller is normalised into `Value::Int` by the core constructors and by the encoder, so it goes out under `i` rather than `u`.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::value_to_json;

assert_eq!(
  value_to_json(&Value::uint(u128::MAX)).to_string(),
  "{\"$\":\"u\",\"v\":\"340282366920938463463374607431768211455\"}"
);
```

## Carrying Floats and Non-Finite Values

`f64` reaches the wire as a bare number only when it is finite with a non-zero fraction. An integral `f64` is tagged so it does not come back as an integer. The three non-finite values ride as symbols.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::{json_to_value, value_to_json};

assert_eq!(value_to_json(&Value::F64(2.5)).to_string(), "2.5");
assert_eq!(value_to_json(&Value::F64(1.0)).to_string(), "{\"$\":\"f\",\"v\":1.0}");
assert_eq!(value_to_json(&Value::F64(f64::NEG_INFINITY)).to_string(), "{\"$\":\"f\",\"v\":\"-inf\"}");

let decoded = json_to_value(&value_to_json(&Value::F64(1.0))).unwrap();
assert!(matches!(decoded, Value::F64(_)));
```

`f32` is always tagged, since nothing in JSON distinguishes it from `f64`. Decoding a tagged number under `f32` narrows it with an `as f32` cast.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::value_to_json;

assert_eq!(value_to_json(&Value::F32(2.5)).to_string(), "{\"$\":\"f32\",\"v\":2.5}");
assert_eq!(value_to_json(&Value::F32(f32::NAN)).to_string(), "{\"$\":\"f32\",\"v\":\"nan\"}");
```

`nan` compares unequal to itself, so a round trip over a non-finite value is checked through the core fingerprint rather than with `assert_eq!`:

```rust
use snapfire_fsr_core::{Fingerprint, Value};
use snapfire_fsr_payload::{json_to_value, value_to_json};

let value = Value::F64(f64::NAN);
let decoded = json_to_value(&value_to_json(&value)).unwrap();
assert_eq!(value.fingerprint(), decoded.fingerprint());
```

## Carrying Bytes and Typed Arrays

Bytes go out as standard base64 under `b`.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::value_to_json;

assert_eq!(value_to_json(&Value::Bytes(vec![0, 1, 2, 255])).to_string(), "{\"$\":\"b\",\"v\":\"AAEC/w==\"}");
```

A typed array goes out under `ta`, with `k` naming the element kind and `v` holding the elements as little-endian bytes in base64. The kind is one of `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32` or `f64`. The browser maps each onto the matching `TypedArray` constructor.

```rust
use snapfire_fsr_core::{TypedArray, Value};
use snapfire_fsr_payload::value_to_json;

let json = value_to_json(&Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
assert_eq!(json.to_string(), "{\"$\":\"ta\",\"k\":\"f64\",\"v\":\"AAAAAAAA8D8AAAAAAAAEQAAAAAAAAAhA\"}");
```

Decoding rejects a byte length that is not a whole number of elements and it rejects a kind it does not know.

## Escaping a Map With a Dollar Key

A `Value::Map` normally becomes a JSON object. When the map itself holds the key `$`, the object form would read back as a tag, so the whole map is written as an array of key and value pairs under `m`.

```rust
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_payload::value_to_json;

let mut plain = ValueMap::new();
plain.insert("name".to_owned(), Value::str("web-1"));
assert_eq!(value_to_json(&Value::Map(plain)).to_string(), "{\"name\":\"web-1\"}");

let mut tricky = ValueMap::new();
tricky.insert("$".to_owned(), Value::str("not a tag"));
tricky.insert("other".to_owned(), Value::int(1i64));
assert_eq!(
  value_to_json(&Value::Map(tricky)).to_string(),
  "{\"$\":\"m\",\"v\":[[\"$\",\"not a tag\"],[\"other\",1]]}"
);
```

The escape triggers on the key alone, wherever the `$` key sits in the map. A string that merely starts with `$` needs no escape and stays a plain JSON string.

## Encoding Variants and Refs

A variant carries its tag under `t` and its payload under `p` when it has one. The key is absent for a payload-free variant rather than present and null.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::value_to_json;

let down = Value::Variant { tag: "Down".into(), payload: None };
assert_eq!(value_to_json(&down).to_string(), "{\"$\":\"var\",\"t\":\"Down\"}");

let retrying = Value::Variant { tag: "Retrying".into(), payload: Some(Box::new(Value::int(3i64))) };
assert_eq!(value_to_json(&retrying).to_string(), "{\"$\":\"var\",\"t\":\"Retrying\",\"p\":3}");
```

A ref carries its kind under `k` (either `action` or `module`) and its identifier under `id`. These are how a server hands the browser a callable action or a mountable module inside props.

```rust
use snapfire_fsr_core::{RefKind, Value};
use snapfire_fsr_payload::value_to_json;

assert_eq!(
  value_to_json(&Value::action_ref("saveServer")).to_string(),
  "{\"$\":\"ref\",\"k\":\"action\",\"id\":\"saveServer\"}"
);
assert_eq!(
  value_to_json(&Value::Ref { kind: RefKind::Module, id: "components/Star.tsx#default".into() }).to_string(),
  "{\"$\":\"ref\",\"k\":\"module\",\"id\":\"components/Star.tsx#default\"}"
);
```

## Serializing a Node Tree to HTML

`html_serialize` walks one tree and returns markup. `Node::Text` is escaped, `Node::Raw` is written through untouched.

```rust
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::html_serialize;

let node = Node::Seq(vec![Node::text("a < b & c > d")]);
assert_eq!(html_serialize(&node), "a &lt; b &amp; c &gt; d");
```

A `Node::Client` becomes an `<sf-i>` element carrying its id and module, then a sibling script holding the props. Inside the element go the server-rendered `ssr` tree when there is one, otherwise the children.

```rust
use snapfire_fsr_core::{ModuleId, Node, ValueMap};
use snapfire_fsr_payload::html_serialize;

let node = Node::Client {
  module: ModuleId::new("components/X.tsx", "default"),
  props: ValueMap::new(),
  children: Vec::new(),
  ssr: Some(Box::new(Node::raw("<svg></svg>"))),
};
assert!(html_serialize(&node).starts_with(
  "<sf-i id=\"sf-i0\" data-sf-module=\"components/X.tsx#default\"><svg></svg></sf-i>"
));
```

Props are encoded with `value_to_json` and every `<` in the result is rewritten as the JSON escape `\u003c`, so a string prop cannot close the script tag it sits in. Nothing else in the props JSON is altered and a reader gets the `<` back when it parses the script body.

```rust
use snapfire_fsr_core::{ModuleId, Node, Value, ValueMap};
use snapfire_fsr_payload::html_serialize;

let mut props = ValueMap::new();
props.insert("payload".to_owned(), Value::str("</script><script>alert(1)</script>"));
let node = Node::Client {
  module: ModuleId::new("components/X.tsx", "default"),
  props,
  children: Vec::new(),
  ssr: None,
};

let html = html_serialize(&node);
let after_open = html.split_once("data-sf-props=\"sf-i0\">").unwrap().1;
let inner = after_open.rsplit_once("</script>").unwrap().0;
assert!(!inner.contains("</script>"));
```

A `Node::Pending` emits its fallback inside a slot-addressed element, which is what a later chunk replaces.

```rust
use snapfire_fsr_core::{Node, SlotId};
use snapfire_fsr_payload::html_serialize;

let node = Node::Pending { slot: SlotId(1), fallback: Box::new(Node::raw("<div class=skl></div>")) };
assert_eq!(html_serialize(&node), "<div data-sf-slot=\"1\"><div class=skl></div></div>");
```

## Keeping Island Ids Unique Across Chunks

Island ids are allocated from a counter in tree order, starting at zero.

```rust
use snapfire_fsr_core::{ModuleId, Node, ValueMap};
use snapfire_fsr_payload::html_serialize;

let island = |name: &str| Node::Client {
  module: ModuleId::new(format!("components/{name}.tsx"), "default"),
  props: ValueMap::new(),
  children: Vec::new(),
  ssr: None,
};

let html = html_serialize(&Node::Seq(vec![island("A"), island("B")]));
assert!(html.find("sf-i0").unwrap() < html.find("sf-i1").unwrap());
```

`html_serialize` is a one-shot form that starts a fresh counter every call, so calling it twice in one response would emit `sf-i0` twice. A streamed response serializes the initial tree and each late slot separately, so hold one `HtmlSession` across all of them and call `serialize` per chunk. Reach for `html_serialize` for a single self-contained tree and for `HtmlSession` whenever a second chunk of the same response is possible.

```rust
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::HtmlSession;

let mut session = HtmlSession::new();
let first = session.serialize(&initial_tree);
let later = session.serialize(&resolved_slot_tree);
```

## Encoding Nodes as Wire Rows

`node_to_row_json` maps each node onto a JSON array whose first element is a one-letter kind.

| Kind | Node | Row |
| --- | --- | --- |
| `t` | `Node::Text` | `["t", "the text"]` |
| `r` | `Node::Raw` | `["r", "<main>"]` |
| `q` | `Node::Seq` | `["q", [ ...child rows ]]` |
| `c` | `Node::Client` | `["c", {"m": "path#export", "p": {...props}, "ch": [ ...child rows ], "s": null}]` |
| `p` | `Node::Pending` | `["p", 1, ...fallback row]` |

A `c` row carries the module id in its `Display` form, path then `#` then export. Its `p` field is the props map through `value_to_json`, so every tag above can appear inside it. Its `ch` field is the children; its `s` field is the server-rendered tree or `null`.

```rust
use snapfire_fsr_core::{ModuleId, Node, ValueMap};
use snapfire_fsr_payload::node_to_row_json;

let node = Node::Client {
  module: ModuleId::new("components/ServerChart.tsx", "default"),
  props: ValueMap::new(),
  children: vec![Node::text("chart caption")],
  ssr: None,
};

assert_eq!(
  node_to_row_json(&node).to_string(),
  "[\"c\",{\"m\":\"components/ServerChart.tsx#default\",\"p\":{},\"ch\":[[\"t\",\"chart caption\"]],\"s\":null}]"
);
```

`serialize_page` wraps a whole tree as a complete response: a `V` row announcing the format, then the `N` row holding the tree.

```rust
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::serialize_page;

let wire = serialize_page(&Node::text("hi"));
assert_eq!(wire, "V {\"fmt\":1,\"enc\":\"json\"}\nN [\"t\",\"hi\"]\n");
```

## Decoding Wire Rows Back to Nodes

`row_json_to_node` is the inverse and is what a Rust-side reader or a test uses to check a tree survived.

```rust
use snapfire_fsr_core::{Fingerprint, ModuleId, Node, SlotId, Value, ValueMap};
use snapfire_fsr_payload::{node_to_row_json, row_json_to_node};

let mut props = ValueMap::new();
props.insert("onSave".to_owned(), Value::action_ref("saveServer"));
let page = Node::Seq(vec![
  Node::raw("<main>"),
  Node::Client {
    module: ModuleId::new("components/ServerChart.tsx", "default"),
    props,
    children: vec![Node::text("chart caption")],
    ssr: Some(Box::new(Node::raw("<svg></svg>"))),
  },
  Node::Pending { slot: SlotId(1), fallback: Box::new(Node::raw("<div class=skl></div>")) },
  Node::raw("</main>"),
]);

let row = node_to_row_json(&page);
let reparsed: serde_json::Value = serde_json::from_str(&row.to_string()).unwrap();
let decoded = row_json_to_node(&reparsed).unwrap();
assert_eq!(page.fingerprint(), decoded.fingerprint());
```

Decoding a `c` row is the strict part: `m` must parse as `path#export`, `p` must be present and must decode to a map, `ch` is treated as empty when absent and `s` is treated as absent when it is missing or `null`.

## Streaming Slot Resolutions

`serialize_page` covers a response with nothing deferred. When slots resolve after the tree has gone out, the reader expects one `S` row per resolution, carrying the slot id then the row for its content. `snapfire_fsr_runtime` writes these and it also inserts a `G` sidecar row between `N` and the first `S` for the navigator; this crate emits only `V` and `N`.

```rust
use serde_json::json;
use snapfire_fsr_payload::{node_to_row_json, FORMAT_VERSION};

let mut out = String::new();
out.push_str(&format!("V {}\n", json!({ "fmt": FORMAT_VERSION, "enc": "json" })));
out.push_str(&format!("N {}\n", node_to_row_json(&tree)));
for resolved in resolutions {
  out.push_str(&format!("S {} {}\n", resolved.slot.0, node_to_row_json(&resolved.node)));
}
```

The HTML equivalent sends each resolution as an inert template the page script moves into the slot. It is here that one `HtmlSession` has to span the whole response.

```rust
use snapfire_fsr_payload::HtmlSession;

let mut session = HtmlSession::new();
let mut out = String::new();
out.push_str(&session.serialize(&tree));
for resolved in resolutions {
  let body = session.serialize(&resolved.node);
  out.push_str(&format!("<template data-sf-fill=\"{}\">{body}</template>", resolved.slot.0));
}
```

## Checking the Format Version

`FORMAT_VERSION` is the integer written into the `V` row; a reader compares against it before trusting the rows that follow.

```rust
use snapfire_fsr_payload::FORMAT_VERSION;

let announced: u32 = 1;
assert_eq!(announced, FORMAT_VERSION);
```

The `V` row also carries `enc`, which is `json` for the row protocol described here. A reader that meets an `enc` it does not know should stop rather than guess at the rows.

## Choosing Between HTML and Rows

Both encodings carry the same tree; the choice is about who consumes it. Serve HTML when the browser has no client runtime yet, when the response has to be readable without JavaScript or when the first paint matters more than the handoff; islands boot from the `data-sf-props` scripts afterwards. Serve rows when the client runtime is already live, for instance a navigation the navigator handles, since the reader can build the tree directly instead of parsing markup and re-reading props out of the DOM.

```rust
use snapfire_fsr_payload::{html_serialize, serialize_page};

let body = if wants_wire { serialize_page(&tree) } else { html_serialize(&tree) };
```

The two encodings share `value_to_json` for props, so a value behaves the same either way. The golden tests pin both byte for byte against the same tree.

## Error Handling

Encoding cannot fail. Decoding returns `Result<_, DecodeError>`; `DecodeError` is a newtype over the message, so the message is the whole of the information.

```rust
use snapfire_fsr_payload::{row_json_to_node, DecodeError};

match row_json_to_node(&row) {
  Ok(node) => handle(node),
  Err(DecodeError(why)) => tracing::warn!(target: "fsr::payload", %why, "dropping malformed row"),
}
```

`DecodeError` implements `Display` as `payload json decode: {message}` and `std::error::Error`, so it composes with the usual error plumbing.

```rust
use snapfire_fsr_payload::{json_to_value, DecodeError};

fn read_props(json: &serde_json::Value) -> Result<snapfire_fsr_core::Value, Box<dyn std::error::Error>> {
  Ok(json_to_value(json)?)
}
```

What the messages report:

| Category | Example message |
| --- | --- |
| Unknown tag name | ``unknown tag `x` `` |
| Missing tag field | ``tag `ta` missing field `k` `` |
| Wrong field type | ``tag `b` field `v` must be a string`` |
| Malformed tag payload | ``tag `i` holds a non-integer``, ``tag `b` holds invalid base64`` |
| Unknown symbol | ``tag `f` unknown symbol `huge` `` |
| Typed array problems | ``unknown typed array kind `i128` ``, ``typed array `i32` byte length 7 not a multiple of 4`` |
| Map escape problems | ``tag `m` entries must be pairs``, ``tag `m` keys must be strings`` |
| Unknown ref kind | ``unknown ref kind `route` `` |
| Malformed rows | ``node row must be an array``, ``node row missing kind``, ``unknown node row kind `z` `` |
| Row field problems | ``` `c` row needs `m` ```, ``` `c` row `p` must decode to a map ```, ``` `p` row needs a slot id ``` |
