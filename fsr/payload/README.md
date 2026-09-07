# snapfire_fsr_payload

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_payload.svg)](https://crates.io/crates/snapfire_fsr_payload)
[![Docs.rs](https://docs.rs/snapfire_fsr_payload/badge.svg)](https://docs.rs/snapfire_fsr_payload)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The encoding layer of SnapFire FSR. It turns the `Value` and `Node` vocabulary of `snapfire_fsr_core` into the three forms a response can leave the server in: the lossless tagged JSON pair, the HTML string with island markers and props, then the line-oriented row protocol the browser client reads while the response is still open. The browser half that decodes the same tags and rows is `@snapfire/fsr-client` under `fsr/client`. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_payload = "0.5"
```

`serde_json` is pulled in with `preserve_order`, so the key order of a `ValueMap` survives encoding. `base64` carries the standard alphabet used for bytes and typed arrays.

## What to reach for

| Problem | Piece |
| --- | --- |
| Encode a `Value` as JSON nothing is lost through | `value_to_json` |
| Read JSON back into a `Value`, tagged or foreign | `json_to_value` |
| Render one `Node` tree to a finished HTML string | `html_serialize` |
| Render several chunks of one response with island ids that stay unique | `HtmlSession` |
| Encode a `Node` tree as a single wire row | `node_to_row_json` |
| Read a wire row back into a `Node` | `row_json_to_node` |
| Write a complete non-streamed page in the wire format | `serialize_page` |
| State which wire format a reader is being handed | `FORMAT_VERSION` |
| Say why a decode failed | `DecodeError` |
