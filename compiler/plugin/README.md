# snapfire_compiler_wire

License: same as the workspace. Status: active, protocol 2.

The wire contract between `snapfirec` and a framework compiler plugin: the types both sides serialize as one JSON object per line over stdin and stdout. A plugin depends on this crate to speak the protocol; `snapfirec` depends on it to read the answers. The usage guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_compiler_wire = "0.1"
```

No features.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Announce a plugin and what it compiles | `Hello`, written first, once |
| Read a batch of units to compile | `Request`, one per line on stdin |
| Answer a batch | `Response`, one `Outcome` per unit in order |
| Hand back a compiled module and its styles | `Outcome::Ok(Compiled)` |
| Refuse a unit with a place | `Outcome::Failed { diagnostics }` |
| Ask for a file beside the source | `Outcome::Needs { files }`, answered through `Unit::files` |
| Say a compile is a warning rather than a stop | `Diagnostic::warning` |
| Refuse a host speaking another protocol | compare `Hello::protocol` with `PROTOCOL` |

## Status

Protocol 2 since 2026-09-12: `Unit::files` and `Outcome::Needs`. A host and a plugin on different versions refuse each other by name.
