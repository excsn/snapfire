# snapfire_compiler_wire

[![Crates.io](https://img.shields.io/crates/v/snapfire_compiler_wire.svg)](https://crates.io/crates/snapfire_compiler_wire)
[![Docs.rs](https://docs.rs/snapfire_compiler_wire/badge.svg)](https://docs.rs/snapfire_compiler_wire)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The messages `snapfirec` and a framework compiler plugin exchange. Each message is one JSON object on one line, over the plugin's stdin and stdout.

Depend on it when you write a plugin that compiles a component format for `snapfirec`. A tool that runs such plugins depends on it too. `snapfirec` and `fsr` use it on the host side. An application never needs it.

The `driven` module covers a second conversation. It holds the lines `snapfirec --driven` exchanges with the tool driving it. It also holds the asset map that tool hands the compiler.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_compiler_wire = "0"
```

No features.

## What to reach for

| Problem | Piece |
| --- | --- |
| Announce a plugin and what it compiles | `Hello`, written first, once |
| Read a batch of units to compile | `Request`, one per line on stdin |
| Answer a batch | `Response`, one `Outcome` per unit in order |
| Hand back a compiled module and its styles | `Outcome::Ok(Compiled)` |
| Refuse a unit with a place | `Outcome::Failed { diagnostics }` |
| Ask for a file beside the source | `Outcome::Needs { files }`, answered through `Unit::files` |
| Say a compile is a warning rather than a stop | `Diagnostic::warning` |
| Name what the framework found, as Svelte's warning codes do | `Diagnostic::code` |
| Describe a component's template and script without compiling it | Answer a `Request` whose `kind` is `Describe` with `Outcome::Described(Described)` |
| Spawn a plugin and talk to it | `host::Worker`, which `snapfirec` and `fsr` both use |
| Refuse a host speaking another protocol | Compare `Hello::protocol` with `PROTOCOL` |
| Know which extensions go to a plugin and what each binary is called | `EXTENSIONS`, `SUFFIXES`, `claimed`, `claimed_name`, `binary_for`, `install_hint` |
| Drive `snapfirec --driven` and refuse one speaking another protocol | `driven::hello`, `driven::parse_hello`, `driven::PROTOCOL` and the line constants |
| Hand the compiler the URLs of every image and font you serve | `driven::AssetMap`, written as JSON and named with `--asset-map` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](../LICENSE) file for the full license text.
