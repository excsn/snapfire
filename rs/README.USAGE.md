# Usage Guide: snapfire

This guide covers building a `TeraWeb` application state, rendering Tera 2 templates from Actix handlers, extending Tera with your own filters, functions, tests and components and running the live-reload development server.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Building the Application State](#building-the-application-state)
* [Rendering from a Handler](#rendering-from-a-handler)
* [Adding Global Variables](#adding-global-variables)
* [Registering a Custom Filter](#registering-a-custom-filter)
* [Registering a Function](#registering-a-function)
* [Registering a Test](#registering-a-test)
* [Defining and Calling a Component](#defining-and-calling-a-component)
* [Enabling Live Reload](#enabling-live-reload)
  * [Watching Static Assets](#watching-static-assets)
  * [Changing the WebSocket Path](#changing-the-websocket-path)
  * [Turning Off Automatic Injection](#turning-off-automatic-injection)
* [Building for Production](#building-for-production)
* [Running the Examples](#running-the-examples)
* [Error Handling](#error-handling)

## Core Concepts

* **`TeraWeb`** - The application state. Holds the Tera engine and the global context, is cheap to `clone` and is shared with handlers through `web::Data`.
* **`TeraWebBuilder`** - The only way to construct a `TeraWeb`. Created by `TeraWeb::builder(glob)`.
* **Templates glob** - The pattern Tera expands to find template files, for example `templates/**/*.html`. A template's name is its path relative to the non-glob part of that pattern.
* **`Template`** - What `TeraWeb::render` returns. It records the template name and context and implements Actix's `Responder`; the render itself happens when Actix builds the response.
* **Global context** - Values added with `add_global`, merged into every render.
* **Request context** - The `tera::Context` you pass to `render`. Its keys win over globals of the same name.
* **`configure_tera`** - The escape hatch that hands you `&mut tera::Tera` to register filters, functions, tests and components.
* **Parse-time resolution** - Tera 2 resolves every name a template references while parsing it. Registration must therefore happen before templates load, which is why `configure_tera` runs first and why an unknown name fails `build()` rather than a later render.
* **Filter** - `fn(Arg, Kwargs, &State) -> Res`, applied with `{{ value | name }}`.
* **Function** - `fn(Kwargs, &State) -> Res`, called with `{{ name(arg=1) }}`.
* **Test** - `fn(Arg, Kwargs, &State) -> bool`, used with `{% if value is name %}`.
* **Component** - Tera 2's replacement for macros. Defined in a template with `{% component %}` and called with `{{ <name/> }}`.
* **`devel` feature** - Compiles in the file watcher, the WebSocket route and the script-injecting middleware. Off by default, so release builds carry none of it.
* **Reload script** - A small JavaScript snippet the middleware inserts before `</body>` on HTML responses. It opens the reload WebSocket.
* **Static watch path** - A directory registered with `watch_static`. A `.css` change there swaps stylesheets in place; anything else triggers a full page reload.

## Quick Start

```toml
# Cargo.toml
[dependencies]
snapfire = "0.5"
actix-web = "4"
tera = "2"
env_logger = "0.11"
log = "0.4"

[features]
devel = ["snapfire/devel"]
```

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
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  let app_state = TeraWeb::builder("templates/**/*.html")
    .add_global("site_name", "My Site")
    .watch_static("static")
    .build()
    .expect("Failed to build TeraWeb app");

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

```html
<!-- templates/index.html -->
<!DOCTYPE html>
<html lang="en">
<head><title>{{ site_name }} | {{ page_title }}</title></head>
<body><h1>Hello from SnapFire!</h1></body>
</html>
```

```sh
cargo run --features devel
```

The `wrap` and `configure` calls compile to no-ops without `devel`, so the same `main.rs` builds for production unchanged.

## Building the Application State

`build()` loads the glob and returns a `Result`. Build once at startup and clone the state into each worker.

```rust
let app_state = TeraWeb::builder("templates/**/*.html").build()?;
```

Template names are relative to the fixed part of the glob, so `templates/**/*.html` makes `templates/pages/about.html` available as `pages/about.html`.

```rust
app_state.render("pages/about.html", Context::new());
```

An absolute glob keeps the app runnable from any working directory:

```rust
let mut glob = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
glob.push("templates/**/*.html");
let app_state = TeraWeb::builder(glob.to_str().unwrap()).build()?;
```

## Rendering from a Handler

`render` is synchronous and does no work; it returns a `Template` that Actix renders while building the response.

```rust
use actix_web::{Responder, web};
use snapfire::TeraWeb;
use tera::Context;

async fn profile(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Profile");
  app_state.render("profile.html", context)
}
```

Any `serde::Serialize` type can go into the context:

```rust
#[derive(serde::Serialize)]
struct User {
  name: String,
  email: String,
}

let mut context = Context::new();
context.insert("user", &User {
  name: "Alice".to_string(),
  email: "alice@example.com".to_string(),
});
```

Pick the template at runtime by returning the same type from both arms:

```rust
match found {
  Some(post) => app_state.render("post.html", context),
  None => app_state.render("not_found.html", context),
}
```

## Adding Global Variables

Globals are merged into every render. A request context key of the same name overrides the global.

```rust
let app_state = TeraWeb::builder("templates/**/*.html")
  .add_global("site_name", "My Site")
  .add_global("version", env!("CARGO_PKG_VERSION"))
  .build()?;
```

```html
<footer>{{ site_name }} v{{ version }}</footer>
```

## Registering a Custom Filter

A filter is any `Fn(Arg, Kwargs, &State) -> Res`. `Arg` is converted for you from the piped value and `Res` is either a value or a `TeraResult`.

```rust
use tera::{Kwargs, State};

fn upcase(value: &str, _: Kwargs, _: &State) -> String {
  value.to_uppercase()
}

let app_state = TeraWeb::builder("templates/**/*.html")
  .configure_tera(|tera| {
    tera.register_filter("upcase", upcase);
  })
  .build()?;
```

```html
<h1>{{ site_name | upcase }}</h1>
```

Read named arguments off `Kwargs` and return `TeraResult` when reading them can fail:

```rust
use tera::{Kwargs, State, TeraResult};

fn money(value: i64, kwargs: Kwargs, _: &State) -> TeraResult<String> {
  let symbol: Option<&str> = kwargs.get("symbol")?;
  Ok(format!("{}{}.{:02}", symbol.unwrap_or("$"), value / 100, value % 100))
}
```

```html
<p>{{ product.cents | money(symbol="£") }}</p>
```

Use `get` for an optional argument and `must_get` for a required one:

```rust
let symbol: Option<&str> = kwargs.get("symbol")?;
let width: i64 = kwargs.must_get("width")?;
```

Closures work wherever a function does:

```rust
.configure_tera(|tera| {
  tera.register_filter("double", |x: i64, _: Kwargs, _: &State| x * 2);
})
```

## Registering a Function

A function takes only kwargs. `Kwargs::deserialize` turns the whole argument set into a struct.

```rust
use tera::{Kwargs, State, TeraResult};

#[derive(serde::Deserialize)]
struct Product {
  cents: i64,
}

#[derive(serde::Deserialize)]
struct TotalArgs {
  of: Vec<Product>,
}

fn total(kwargs: Kwargs, _: &State) -> TeraResult<i64> {
  let args: TotalArgs = kwargs.deserialize()?;
  Ok(args.of.iter().map(|p| p.cents).sum())
}

.configure_tera(|tera| {
  tera.register_function("total", total);
})
```

```html
<p>Total: {{ total(of=products) | money }}</p>
```

## Registering a Test

A test returns `bool` and is used after `is`.

```rust
.configure_tera(|tera| {
  tera.register_test("odd", |x: i64, _: Kwargs, _: &State| x % 2 != 0);
})
```

```html
{% if index is odd %}<tr class="alt">{% endif %}
```

## Defining and Calling a Component

Components replace Tera 1's macros. Define one in any template inside the glob, then call it from any other.

```html
<!-- templates/components.html -->
{% component price_tag(name, cents) %}
<li><strong>{{ name | upcase }}</strong> {{ cents | money }}</li>
{% endcomponent price_tag %}
```

```html
<!-- templates/index.html -->
<ul>
  {% for product in products %}
    {{ <price_tag name={product.name} cents={product.cents}/> }}
  {% endfor %}
</ul>
```

Literal arguments are quoted; expressions go in braces.

```html
{{ <price_tag name="widget" cents={1250}/> }}
```

Give a parameter a default to make it optional:

```html
{% component button(label, variant="primary") %}
<button class="btn btn-{{ variant }}">{{ label }}</button>
{% endcomponent button %}
```

A component can take a body, available inside as `body`:

```html
{% component card(title) %}<section><h2>{{ title }}</h2>{{ body }}</section>{% endcomponent card %}
```

```html
{% <card title="Details"> %}<p>Anything here.</p>{% </card> %}
```

## Enabling Live Reload

Everything in this section requires the `devel` feature. Add a feature to your own crate that forwards to it, so one flag switches the whole app:

```toml
[features]
devel = ["snapfire/devel"]
```

```sh
cargo run --features devel
```

Three pieces have to be present. The builder starts the watcher, the middleware injects the client script and `configure_routes` mounts the WebSocket:

```rust
let app_state = TeraWeb::builder("templates/**/*.html").build()?;

App::new()
  .app_data(web::Data::new(app_state.clone()))
  .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
  .configure(|cfg| app_state.configure_routes(cfg))
```

The middleware reads its settings from the `TeraWeb` in `app_data`, so registering the state is what wires the two together.

### Watching Static Assets

`watch_static` can be called more than once. A `.css` change swaps stylesheets without navigating; any other watched change reloads the page.

```rust
let app_state = TeraWeb::builder("templates/**/*.html")
  .watch_static("static")
  .watch_static("assets/css")
  .build()?;
```

Template changes are picked up from the glob's own directory without registering it.

### Changing the WebSocket Path

The default is `/_snapfire/ws`. Setting it moves both the route and the URL baked into the injected script.

```rust
let app_state = TeraWeb::builder("templates/**/*.html")
  .ws_path("/_dev/socket")
  .build()?;
```

### Turning Off Automatic Injection

Injection applies to every `text/html` response, targets the end of `<body>`, appends to the end when there is no such tag, and emits an inline `<script>` with no CSP `nonce`. Turn it off when that is wrong for your application, most often under a strict Content-Security-Policy or on an endpoint returning HTML fragments.

```rust
let app_state = TeraWeb::builder("templates/**/*.html")
  .auto_inject_script(false)
  .build()?;
```

The watcher and the WebSocket route keep running, so the server half of live reload is intact and the page has to connect for itself. `TeraWeb::reload_script` hands you the stock client for that, as JavaScript source with the configured path already substituted and no `<script>` tag around it, so you own the element:

```rust
async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("csp_nonce", &nonce);
  context.insert("reload_script", &app_state.reload_script());
  app_state.render("index.html", context)
}
```

```html
<script nonce="{{ csp_nonce }}">{{ reload_script | safe }}</script>
```

Without the `devel` feature the call returns an empty string, so the same template renders an empty element in release rather than needing a conditional.

Only do this with injection turned off. Embedding the script while `auto_inject_script` is still `true` gives the page two reload clients and two WebSocket connections.

Writing your own client instead is a matter of opening the path and handling two messages, `reload` and `reload-css`:

```js
const ws = new WebSocket(`ws://${window.location.host}/_snapfire/ws`);
ws.onmessage = (event) => {
  if (event.data === 'reload') {
    window.location.reload();
  } else if (event.data === 'reload-css') {
    document.querySelectorAll("link[rel='stylesheet']").forEach((link) => {
      const url = new URL(link.href);
      url.searchParams.set('_', Date.now());
      link.href = url.href;
    });
  }
};
```

## Building for Production

Leave `devel` off. The watcher, the WebSocket route and the injection middleware are all compiled out and `wrap`/`configure_routes` become no-ops, so no `#[cfg]` is needed in your own code.

```sh
cargo build --release
```

## Running the Examples

```sh
cargo run -p snapfire --example build_diagnostics
cargo run -p snapfire --example custom_filters --features devel
cargo run -p snapfire --example inheritance --features devel
cargo run -p snapfire --example live_reload --features devel
```

## Error Handling

Fallible operations return `snapfire::Result<T>`, an alias for `Result<T, SnapFireError>`.

```rust
pub enum SnapFireError {
  Tera(tera::Error),
  Io(std::io::Error),
  Serialization(String),
  #[cfg(feature = "devel")]
  Watcher(notify::Error),
}
```

`build()` is where most failures surface. Because Tera 2 resolves names while parsing, a template that references a filter, function, test or component you never registered fails here rather than at render time.

```rust
match TeraWeb::builder("templates/**/*.html").build() {
  Ok(app_state) => app_state,
  Err(SnapFireError::Tera(e)) => panic!("template failed to load or parse: {e}"),
  Err(SnapFireError::Watcher(e)) => panic!("could not watch the template directory: {e}"),
  Err(e) => panic!("{e}"),
}
```

Tera 2 reports parse errors with the offending source line:

```text
Tera error: error: Unknown filter `upcase`
 --> index.html:1:11
  |
1 | {{ name | upcase }}
  |           ^^^^^^
```

A glob that matches no files is not an error; `build()` succeeds with no templates and the failure appears when a missing template is rendered.

Render-time failures reach Actix through `Responder`, which logs them and returns `500 Internal Server Error`. Reading an undefined variable is one of them, since Tera 2 errors instead of substituting an empty string.
