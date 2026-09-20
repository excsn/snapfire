# snapfire_compiler_wire

License: same as the workspace. Status: active, protocol 3.

The wire contract between `snapfirec` and a framework compiler plugin: the types both sides serialize as one JSON object per line over stdin and stdout. A plugin depends on this crate to speak the protocol; `snapfirec` depends on it to read the answers. The usage guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_compiler_wire = "0"
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
| Describe a component for a host that lowers it rather than shipping it | answer a `Request` whose `kind` is `Describe` with `Outcome::Described(Described)` |
| Spawn a plugin and talk to it | `host::Worker`, which `snapfirec` and `fsr` both use |
| Refuse a host speaking another protocol | compare `Hello::protocol` with `PROTOCOL` |
| Know which extensions go to a plugin and what each binary is called | `EXTENSIONS`, `claimed`, `binary_for`, `install_hint` |

## Status

Protocol 3 since 2026-09-20: `Request::kind` with `Describe`, `Outcome::Described` and the `host` module. Protocol 2 added `Unit::files` and `Outcome::Needs`. A host and a plugin on different versions refuse each other by name.
