# SnapFire

[![Crates.io](https://img.shields.io/crates/v/snapfire.svg)](https://crates.io/crates/snapfire)
[![Docs.rs](https://docs.rs/snapfire/badge.svg)](https://docs.rs/snapfire)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Renders Tera 2 templates from Actix Web handlers and reloads the browser when you edit a template or a stylesheet.

You build one `TeraWeb` at startup and return `render(...)` from a handler. Globals and your own filters, functions, tests and components are set on the builder.

Live reload sits behind the `devel` feature. With it on, a template edit reloads the page and a CSS edit swaps the stylesheets without navigating. With it off, the watcher, the WebSocket route and the middleware are not compiled in. The same `main.rs` still builds.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire = "0"
actix-web = "4"
tera = "2"

[features]
devel = ["snapfire/devel"]
```

SnapFire turns on Tera's `glob_fs` and `fast` features. Cargo features add up across a build, so your own `tera` dependency gets them too.

## Quick start

```rust
use actix_web::{App, HttpServer, Responder, web};
use snapfire::TeraWeb;
use tera::Context;

async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Welcome");
  app_state.render("index.html", context)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let app_state = TeraWeb::builder("templates/**/*.html")
    .add_global("site_name", "My Site")
    .watch_static("static")
    .build()
    .expect("templates load");

  HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state.clone()))
      .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
      .route("/", web::get().to(index))
      .configure(|cfg| app_state.configure_routes(cfg))
  })
  .bind(("127.0.0.1", 3000))?
  .run()
  .await
}
```

Run it with `cargo run --features devel` and edit `templates/index.html` to see the page reload.

## What to reach for

| Problem | Piece |
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

## Upgrading from Tera 1

SnapFire 0.4 used Tera 1. Every release since 0.5 uses Tera 2, which changed both the Rust API and the template language. [MIGRATION.md](MIGRATION.md) covers the move.

## License

This project is licensed under the **Mozilla Public License 2.0**. See the [LICENSE](LICENSE) file for details.
