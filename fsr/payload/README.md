# snapfire_fsr_payload

MPL-2.0. Pre-release, unpublished, part of the Snapfire FSR workspace.

The encoding layer of Snapfire FSR. It turns the `Value` and `Node` vocabulary of `snapfire_fsr_core` into the three forms a response can leave the server in: the lossless tagged JSON pair, the HTML string with island markers and props, then the line-oriented row protocol the browser client reads while the response is still open. The browser half that decodes the same tags and rows is `@snapfire/fsr-client` under `fsr/client`. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_payload = { path = "../payload" }
```

The crate declares no Cargo features. `serde_json` is pulled in with `preserve_order`, so the key order of a `ValueMap` survives encoding. `base64` carries the standard alphabet used for bytes and typed arrays.

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

## Status

Pre-release and not published to crates.io; it is consumed by path from the other FSR crates. `snapfire_fsr_runtime` builds both the streamed HTML response and the streamed wire response on top of it, which is how the `advanced_tera_app` example under `fsr/examples/` exercises it end to end. The crate carries 16 integration tests: 7 golden tests pinning the exact HTML and wire bytes plus 9 round trip tests over the value model. A `criterion` bench under `benches/encode.rs` covers value encoding, wire pages and HTML pages. No stability guarantee is offered on the encodings yet, but `FORMAT_VERSION` names the one currently emitted.
