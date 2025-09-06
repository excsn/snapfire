# **`snapfire` API Reference**

This document provides a detailed, unambiguous reference for all public API elements of the `snapfire` crate, suitable for developers or automated systems with no prior knowledge of the library.

## **1. Introduction & Core Concepts**

*   **Core Concept:** `snapfire` is a library that integrates the `tera` templating engine with the `actix-web` framework. Its primary goal is to provide a simple API for rendering templates and to offer an optional, automatic live-reload feature for development.

*   **Primary Handle:** The central struct for all operations is `snapfire::TeraWeb`. An instance of this struct is created at application startup, holds all rendering configuration, and is shared with all Actix handlers.

*   **Configuration Entry Point:** All configuration and initialization is performed through the `snapfire::TeraWebBuilder`, which is created via `snapfire::TeraWeb::builder()`.

*   **Rendering Pattern:** The library uses a "renderable struct" pattern. Calling `TeraWeb::render()` does not perform the render immediately. Instead, it returns an instance of `snapfire::Template`. This `Template` struct is what you return from an Actix handler. Actix then uses `snapfire`'s implementation of the `Responder` trait on `Template` to perform the actual rendering asynchronously.

*   **Pervasive Types:**
    *   **`snapfire::Result<T>`**: All fallible operations in this library (like `build()`) return this `Result` type, which is an alias for `std::result::Result<T, snapfire::SnapFireError>`.
    *   **`tera::Context`**: When rendering, users must provide a context object of this type, which comes from the `tera` crate.

## **2. Main Types and Their Public Methods**

### **Struct: `snapfire::TeraWeb`**

The primary application state for SnapFire.

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
    *   **Description:** Provides a closure for advanced, direct manipulation of the `tera::Tera` instance before it is finalized. Use this to register custom filters, functions, etc.
    *   **Parameters:**
        *   `configurator`: `F` where `F: FnOnce(&mut tera::Tera) + 'static` – A closure that receives a mutable reference to the newly created `tera::Tera` instance.

*   **`watch_static`**
    *   **Availability:** Only available when the `devel` feature is enabled.
    *   **Signature:** `#[cfg(feature = "devel")] pub fn watch_static(mut self, path: &str) -> Self`
    *   **Description:** Adds a static asset directory path for the live-reload watcher to monitor for changes.
    *   **Parameters:**
        *   `path`: `&str` – The path to a directory to watch. Example: `"static/css"`.

*   **`ws_path`**
    *   **Availability:** Only available when the `devel` feature is enabled.
    *   **Signature:** `#[cfg(feature = "devel")] pub fn ws_path(mut self, path: &str) -> Self`
    *   **Description:** Customizes the URL path for the live-reload WebSocket endpoint.
    *   **Parameters:**
        *   `path`: `&str` – The URL path. Defaults to `"/_snapfire/ws"`.

*   **`auto_inject_script`**
    *   **Availability:** Only available when the `devel` feature is enabled.
    *   **Signature:** `#[cfg(feature = "devel")] pub fn auto_inject_script(mut self, enabled: bool) -> Self`
    *   **Description:** Controls whether the live-reload JavaScript is automatically injected into HTML responses.
    *   **Parameters:**
        *   `enabled`: `bool` – Set to `false` to disable injection. Defaults to `true`.

*   **`build`**
    *   **Signature:** `pub fn build(self) -> Result<TeraWeb>`
    *   **Description:** Consumes the builder and attempts to create the final `TeraWeb` instance. This can fail if the template glob is invalid or if the watcher fails to initialize.

### **Struct: `snapfire::Template`**

A struct representing a render operation. It has no public fields or methods. Its primary interface is its implementation of `actix_web::Responder`.

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

*   **`Tera(tera::Error)`**: Wraps an error from the underlying `tera` crate.
*   **`Io(std::io::Error)`**: Wraps a standard I/O error.
*   **`Serialization(String)`**: An error occurred during context serialization.
*   **`Watcher(notify::Error)`**: *(Only available when the `devel` feature is enabled).* Wraps an error from the `notify` file watcher crate.