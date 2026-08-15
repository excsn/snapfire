# **`snapfire` API Reference**

This document provides a detailed, unambiguous reference for all public API elements of the `snapfire` crate, suitable for developers or automated systems with no prior knowledge of the library.

## **1. Introduction & Core Concepts**

*   **Core Concept:** `snapfire` is a library that integrates the `tera` templating engine (version 2) with the `actix-web` framework. Its primary goal is to provide a simple API for rendering templates and to offer an optional, automatic live-reload feature for development.

*   **Primary Handle:** The central struct for all operations is `snapfire::TeraWeb`. An instance of this struct is created at application startup, holds all rendering configuration and is shared with all Actix handlers.

*   **Configuration Entry Point:** All configuration and initialization is performed through the `snapfire::TeraWebBuilder`, which is created via `snapfire::TeraWeb::builder()`.

*   **Rendering Pattern:** The library uses a "renderable struct" pattern. Calling `TeraWeb::render()` does not perform the render immediately. Instead, it returns an instance of `snapfire::Template`. This `Template` struct is what you return from an Actix handler. Actix then uses `snapfire`'s implementation of the `Responder` trait on `Template` to perform the actual rendering asynchronously.

*   **Pervasive Types:**
    *   **`snapfire::Result<T>`**: All fallible operations in this library (like `build()`) return this `Result` type, which is an alias for `std::result::Result<T, snapfire::SnapFireError>`.
    *   **`tera::Context`**: When rendering, users must provide a context object of this type, which comes from the `tera` crate.
    *   **`tera::Tera`**: The engine instance, reachable only inside the `configure_tera` closure.
    *   **`tera::Kwargs`, `tera::State`, `tera::TeraResult<T>`, `tera::Value`**: Required to write the filters, functions and tests passed to `tera::Tera::register_filter`, `register_function` and `register_test` inside that closure. A filter is `Fn(Arg, Kwargs, &State) -> Res`, a function is `Fn(Kwargs, &State) -> Res` and a test is `Fn(Arg, Kwargs, &State) -> bool`.

*   **Cargo Features:**
    *   **`devel`** *(off by default)*: Compiles in the file watcher, the live-reload WebSocket route and the injection middleware. Methods marked "Only available when the `devel` feature is enabled" below still exist without it, as no-ops or with the dev behaviour removed, so user code needs no `#[cfg]` attributes.
    *   `snapfire` enables `glob_fs` and `fast` on `tera`. `glob_fs` is mandatory: `TeraWeb::builder` takes a glob and live reload calls `Tera::full_reload`, both of which that feature gates. Cargo features are additive, so a dependent crate's own `tera` dependency also receives them.

*   **Parse-time Name Resolution:** Tera 2 resolves the filters, functions, tests and components a template references while parsing it and errors on an unknown name. Registration therefore has to reach the engine before templates are loaded, which is why `TeraWebBuilder::build` runs the `configure_tera` closure first and why an unregistered name fails `build()` rather than a later render.

## **2. Main Types and Their Public Methods**

### **Struct: `snapfire::TeraWeb`**

The primary application state for SnapFire.

**Trait Implementations**

*   `Clone` – Cloning shares one engine and one global context between all clones; this is how the state reaches every Actix worker.
*   `Debug` – Reports the global context and, under `devel`, the reloader. The Tera engine and the loaded templates are omitted and the output is non-exhaustive.

**Public Methods**

*   **`builder`**
    *   **Signature:** `pub fn builder(templates_glob: &str) -> TeraWebBuilder`
    *   **Description:** Creates a new `TeraWebBuilder` to configure and build a `TeraWeb` instance. This is the main entry point for using the library.
    *   **Parameters:**
        *   `templates_glob`: `&str` – A glob pattern used by `tera` to discover template files. Example: `"templates/**/*.html"`.

*   **`render`**
    *   **Signature:** `pub fn render(&self, tpl: &str, context: tera::Context) -> Template`
    *   **Description:** Prepares a template for rendering by returning a `Template` struct. This method is synchronous.
    *   **Parameters:**
        *   `tpl`: `&str` – The name of the template file to render, relative to the templates directory. Example: `"pages/index.html"`.
        *   `context`: `tera::Context` – The `tera::Context` object containing the variables for this specific render.

*   **`reload_script`**
    *   **Signature:** `pub fn reload_script(&self) -> String`
    *   **Description:** Returns the live-reload client as JavaScript source, for embedding in a template. This is the script `InjectSnapFireScript` injects, with the configured `ws_path` already substituted. It carries no `<script>` tag, so the caller owns the element and can attach a Content-Security-Policy `nonce`. Without the `devel` feature it returns an empty string.
    *   **Pairing:** Intended for use with `auto_inject_script(false)`. Embedding it while injection is enabled gives the page two reload clients and two WebSocket connections.

*   **`configure_routes`**
    *   **Availability:** Only available when the `devel` feature is enabled.
    *   **Signature:** `#[cfg(feature = "devel")] pub fn configure_routes(&self, cfg: &mut actix_web::ServiceConfig)`
    *   **Description:** Configures Actix application routes required for `snapfire`'s development features (specifically, the live-reload WebSocket). In release builds (without the `devel` feature), this method is a no-op.
    *   **Parameters:**
        *   `cfg`: `&mut actix_web::ServiceConfig` – The mutable Actix service configuration that the WebSocket route will be added to.

### **Struct: `snapfire::TeraWebBuilder`**

A builder used to configure and create a `TeraWeb` instance.

**Public Methods**

*   **`add_global`**
    *   **Signature:** `pub fn add_global<S: Into<String>, T: serde::Serialize>(mut self, key: S, value: T) -> Self`
    *   **Description:** Adds a variable to the global context, making it available to all templates rendered by this instance.
    *   **Parameters:**
        *   `key`: `S` where `S: Into<String>` – The name of the variable as it will be used in templates.
        *   `value`: `T` where `T: serde::Serialize` – Any value that implements the `serde::Serialize` trait.

*   **`configure_tera`**
    *   **Signature:** `pub fn configure_tera<F>(mut self, configurator: F) -> Self where F: FnOnce(&mut tera::Tera) + 'static`
    *   **Description:** Provides a closure for advanced, direct manipulation of the `tera::Tera` instance. Use this to register custom filters, functions, tests and components. The closure runs before the template glob is loaded, because Tera resolves those names at parse time and errors on an unknown name.
    *   **Parameters:**
        *   `configurator`: `F` where `F: FnOnce(&mut tera::Tera) + 'static` – A closure that receives a mutable reference to the empty `tera::Tera` instance.

*   **`watch_static`**
    *   **Signature:** `pub fn watch_static(mut self, path: &str) -> Self`
    *   **Description:** Adds a static asset directory path for the live-reload watcher to monitor for changes. The method exists in every build so that call sites need no `#[cfg]`; without the `devel` feature the path is recorded and never used.
    *   **Parameters:**
        *   `path`: `&str` – The path to a directory to watch. Example: `"static/css"`.

*   **`ws_path`**
    *   **Signature:** `pub fn ws_path(mut self, path: &str) -> Self`
    *   **Description:** Customizes the URL path for the live-reload WebSocket endpoint. The method exists in every build so that call sites need no `#[cfg]`; without the `devel` feature no route is mounted. The path is used both for the route added by `configure_routes` and for the URL written into the injected client script.
    *   **Parameters:**
        *   `path`: `&str` – The URL path. Defaults to `"/_snapfire/ws"`.

*   **`auto_inject_script`**
    *   **Signature:** `pub fn auto_inject_script(mut self, enabled: bool) -> Self`
    *   **Description:** Controls whether `InjectSnapFireScript` rewrites `text/html` responses to carry the live-reload client. Read by the middleware from the `TeraWeb` registered as Actix app data; with no such app data the middleware injects. Without the `devel` feature nothing is injected regardless.
    *   **Parameters:**
        *   `enabled`: `bool` – Set to `false` to disable injection. Defaults to `true`.
    *   **Why it exists:** Injection is unconditional on `Content-Type: text/html`, always targets the end of `<body>`, appends to the end when there is no such tag, emits an inline `<script>` with no CSP `nonce`, and buffers the whole response body to find the insertion point. Any of those can be wrong for a given application, most commonly a strict Content-Security-Policy or a fragment endpoint that would otherwise ship a reload client with every partial.
    *   **What `false` leaves running:** The file watcher and the WebSocket route are unaffected, so the server half of live reload still works and it is the page's job to connect. Embed `TeraWeb::reload_script` to place the stock client yourself, or open `ws_path` directly and handle the two broadcast messages, `reload` and `reload-css`.

*   **`build`**
    *   **Signature:** `pub fn build(self) -> Result<TeraWeb>`
    *   **Description:** Consumes the builder and attempts to create the final `TeraWeb` instance. This can fail if the template glob is invalid, if a template fails to parse, or if the watcher fails to initialize.

### **Struct: `snapfire::Template`**

A struct representing a render operation. It has no public fields or methods. Its primary interface is its implementation of `actix_web::Responder`.

Rendering to a `String` is not part of the public surface: the render runs inside `Responder::respond_to`, so a `Template` can only be turned into markup by returning it from an Actix handler.

### **Struct: `snapfire::actix::dev::InjectSnapFireScript`**

An Actix middleware. It has no public fields or methods. It is instantiated via `InjectSnapFireScript::default()` and used with `actix_web::App::wrap()`.

## **3. Public Type Aliases**

### **Type Alias: `snapfire::Result`**
*   **Definition:** `pub type Result<T, E = SnapFireError> = std::result::Result<T, E>;`
*   **Description:** The standard `Result` type used throughout the `snapfire` crate.

## **4. Error Handling**

### **Enum: `snapfire::SnapFireError`**

The unified error enum for all fallible operations in the library.

**Enum Variants**

*   **`Tera(tera::Error)`**: Wraps an error from the underlying `tera` crate, raised while loading, parsing or rendering a template. Load and parse failures surface from `TeraWebBuilder::build`; render failures surface through `Responder` and become a `500`.
*   **`Io(std::io::Error)`**: Wraps a standard I/O error.
*   **`Serialization(String)`**: An error occurred during context serialization.
*   **`Watcher(notify::Error)`**: *(Only available when the `devel` feature is enabled).* Wraps an error from the `notify` file watcher crate.