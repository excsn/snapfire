# SnapFire

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](LICENSE)
![Crates.io](https://img.shields.io/crates/v/snapfire?style=flat-square)
![Docs.rs](https://img.shields.io/docsrs/snapfire?style=flat-square)

An ergonomic web templating engine with live-reload, featuring first-class support for **Tera** and **Actix Web**.

SnapFire is designed to provide a seamless and productive development experience for building server-rendered web applications in Rust. It offers a simple, fluent API for integrating the powerful Tera templating engine into an Actix Web application, and its standout feature is a zero-overhead, "it-just-works" live-reload system for development.

## Features

-   ✅ **Simple & Ergonomic API:** A clean builder pattern for easy setup and configuration.
-   ✅ **Full Tera Integration:** Use all of Tera's features, including template inheritance, macros, and custom filters.
-   ✅ **Live Reload for Development:** Changes to templates or static assets (`.css`) are automatically pushed to the browser, providing instant feedback without a full page refresh.
-   ✅ **Production Optimized:** All development features (file watcher, WebSocket, middleware) are compiled out in release builds by default, ensuring zero performance overhead.
-   ✅ **Robust & Configurable:** Sensible defaults for a great out-of-the-box experience, with powerful overrides for custom setups.

## Quickstart

### 1. Add `snapfire` to your dependencies

```toml
# Cargo.toml
[dependencies]
snapfire = "0.4.0" # Replace with the latest version
actix-web = "4"
tera = "1"
env_logger = "0.11"
```

### 2. Set up your Actix Web `main.rs`

This example shows a simple server with two pages and live-reload enabled for development.

```rust
// src/main.rs
use actix_web::{web, App, HttpServer, Responder};
use snapfire::{TeraWeb, Template};
use tera::Context;

// An Actix handler that renders a template.
async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
    let mut context = Context::new();
    context.insert("page_title", "Welcome");
    // The `render` method returns a `Template` struct, which is a Responder.
    app_state.render("index.html", context)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // 1. Configure and build the SnapFire state.
    let app_state = TeraWeb::builder("templates/**/*.html")
        .add_global("site_name", "My Awesome Site")
        // In dev mode, also watch the static directory for CSS changes.
        .watch_static("static")
        .build()
        .expect("Failed to build TeraWeb app");

    log::info!("🚀 Starting server at http://127.0.0.1:3000");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            // The middleware injects the reload script in dev mode.
            // It is compiled away in release builds.
            .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
            .route("/", web::get().to(index))
            // The configure method adds the WebSocket route in dev mode.
            // It is a no-op in release builds.
            .configure(|cfg| app_state.configure_routes(cfg))
    })
    .bind(("127.0.0.1", 3000))?
    .run()
    .await
}
```

### 3. Create your templates

Create a `templates/` directory with an `index.html` file.

```html
<!-- templates/index.html -->
<!DOCTYPE html>
<html lang="en">
<head>
  <title>{{ site_name }} | {{ page_title }}</title>
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <h1>Hello from SnapFire!</h1>
</body>
</html>
```

### 4. Run your app!

```sh
# This will run with the "devel" feature enabled.
cargo run --features devel
```

Now, open your browser to `http://127.0.0.1:3000`. Try changing your `index.html` file—the browser will instantly reload!

## Production Builds

The live-reload functionality is enabled by a Cargo feature called `devel`. To build your application for production, you must disable this feature to remove the file watcher, WebSocket server, and script injection middleware.

Your `configure_routes` and `InjectSnapFireScript` calls are automatically compiled to no-ops in this case, so you don't need to add any `#[cfg]` attributes to your own code.

## Configuration

SnapFire's `TeraWebBuilder` provides a fluent API for configuration.

```rust
let app_state = TeraWeb::builder("templates/**/*.html")
    // Add global variables available to all templates.
    .add_global("site_name", "My Site")
    .add_global("version", "1.2.3")

    // For power-users: get direct access to the Tera instance
    // before it's finalized to register custom filters, functions, etc.
    .configure_tera(|tera| {
        tera.register_filter("my_custom_filter", my_filter_fn);
    })
    
    // --- Dev-Reload Specific Configuration ---

    // Watch an additional directory for changes.
    .watch_static("assets/css")

    // Customize the WebSocket URL.
    .ws_path("/_internal/dev/ws")

    // Disable automatic script injection.
    .auto_inject_script(false)

    .build()?;
```

## License

This project is licensed under the **Mozilla Public License 2.0**. See the [LICENSE](LICENSE) file for details.