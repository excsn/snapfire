# SnapFire

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](LICENSE)
![Crates.io](https://img.shields.io/crates/v/snapfire?style=flat-square)
![Docs.rs](https://img.shields.io/docsrs/snapfire?style=flat-square)

An ergonomic web templating engine with live-reload, featuring first-class support for **Tera 2** and **Actix Web**. It gives you a fluent builder for wiring Tera into an Actix application and a zero-overhead live-reload system that pushes template and stylesheet edits straight to the browser. Full instructions live in the [usage guide](README.USAGE.md) and every public item is listed in the [API reference](API_REFERENCE.md).

Upgrading from 0.4? SnapFire 0.5 moves to Tera 2, which changes both the Rust API and the template language. See [MIGRATION.md](MIGRATION.md).

## Install

```toml
[dependencies]
snapfire = "0.5"
actix-web = "4"
tera = "2"
```

| Feature | Default | What it adds |
| --- | --- | --- |
| `devel` | off | The file watcher, the live-reload WebSocket route and the script-injecting middleware. Compiled out entirely when off. |

SnapFire enables `glob_fs` and `fast` on `tera`. Cargo features are additive, so your own `tera` dependency gets them too.

## What to reach for

| You want to | Use |
| --- | --- |
| Serve a template from an Actix handler | `TeraWeb::render` |
| Make a value available to every template | `TeraWebBuilder::add_global` |
| Register a filter, function, test or component | `TeraWebBuilder::configure_tera` |
| Reload the browser when a template changes | The `devel` feature, `InjectSnapFireScript` and `configure_routes` |
| Swap stylesheets without navigating | `TeraWebBuilder::watch_static` |
| Move the reload WebSocket off its default path | `TeraWebBuilder::ws_path` |
| Place the reload script yourself, under a CSP nonce | `TeraWebBuilder::auto_inject_script` and `TeraWeb::reload_script` |
| Handle a load, parse or render failure | `SnapFireError` |

## Examples

| Example | Shows | Run |
| --- | --- | --- |
| `build_diagnostics` | What Tera 2 rejects at build time and why. No server. | `cargo run -p snapfire --example build_diagnostics` |
| `custom_filters` | Custom filters, a function and a component. | `cargo run -p snapfire --example custom_filters --features devel` |
| `inheritance` | `extends`, `block` overrides and globals across three routes. | `cargo run -p snapfire --example inheritance --features devel` |
| `live_reload` | Template and CSS reload, with a custom WebSocket path. | `cargo run -p snapfire --example live_reload --features devel` |

## Status

Active. The Rust implementation is the complete one; see the [project repository](https://github.com/excsn/snapfire) for the wider project.

## License

This project is licensed under the **Mozilla Public License 2.0**. See the [LICENSE](LICENSE) file for details.
