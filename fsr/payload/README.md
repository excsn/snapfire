# snapfire_fsr_payload

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_payload.svg)](https://crates.io/crates/snapfire_fsr_payload)
[![Docs.rs](https://docs.rs/snapfire_fsr_payload/badge.svg)](https://docs.rs/snapfire_fsr_payload)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Writes what an FSR server sends to the browser: JSON that keeps every value intact, HTML with its islands marked and the line-by-line stream the browser reads while the response is still arriving.

It works on the values and page trees from [`snapfire_fsr_core`](../core/README.md). A value can hold things plain JSON loses, such as 128-bit integers, `NaN`, bytes and typed arrays. This crate tags those so they come back exactly as they went in. The browser side that reads the same tags and lines is `@snapfire/fsr/client` in [`fsr/client`](../client/README.md).

Most applications never depend on this crate. The FSR runtime and host call it for you. Use it directly when you write your own host, read FSR responses from Rust or test what a page sends.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_payload = "0"
```

## Quick start

```rust
use snapfire_fsr_core::{Node, Value};
use snapfire_fsr_payload::{html_serialize, json_to_value, serialize_page, value_to_json};

let value = Value::Int(1 << 80);
let json = value_to_json(&value);
assert_eq!(json_to_value(&json)?, value);

let page = Node::text("Hello <world>");
assert_eq!(html_serialize(&page), "Hello &lt;world&gt;");
let wire = serialize_page(&page);
assert!(wire.starts_with("V {\"fmt\":3,\"enc\":\"json\"}\n"));
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Encode a `Value` as JSON nothing is lost through | `value_to_json` |
| Read JSON back into a `Value`, whether this crate wrote it or not | `json_to_value` |
| Render one `Node` tree to a finished HTML string | `html_serialize` |
| Render several chunks of one response with island ids that stay unique | `HtmlSession` |
| Encode a `Node` tree as a single wire row | `node_to_row_json` |
| Read a wire row back into a `Node` | `row_json_to_node` |
| Write a complete non-streamed page in the wire format | `serialize_page` |
| Check which wire format a reader is being handed | `FORMAT_VERSION` |
| Find out why a decode failed | `DecodeError` |
