# API Reference: snapfire

Tera 2 templates rendered from Actix Web handlers, with live reload under the `devel` feature.

## Contents

* [1. Application State](#1-application-state)
  * [`TeraWeb`](#teraweb)
  * [`TeraWebBuilder`](#terawebbuilder)
* [2. Rendering](#2-rendering)
  * [`Template`](#template)
* [3. Live Reload](#3-live-reload)
  * [`InjectSnapFireScript`](#injectsnapfirescript)
  * [WebSocket messages](#websocket-messages)
* [4. Features](#4-features)
  * [`devel`](#devel)
* [5. Error Handling](#5-error-handling)
  * [`SnapFireError`](#snapfireerror)
  * [`Result`](#result)

## 1. Application State

### TeraWeb

The engine, the global context and, under `devel`, the reloader.

* `pub struct TeraWeb`. `Clone`, `Debug`. Clones share one engine and one global context. `Debug` prints the global context and, under `devel`, the reloader and `auto_inject_script`.
* `pub fn builder(templates_glob: &str) -> TeraWebBuilder`: a builder that loads the templates matching `templates_glob`.
* `pub fn render(&self, tpl: &str, context: tera::Context) -> Template`: a pending render of the template named `tpl`. Keys in `context` override globals of the same name.
* `pub fn reload_script(&self) -> String`: the live-reload client as JavaScript source with the `ws_path` substituted, without a `<script>` tag. Empty without `devel`.
* `pub fn configure_routes(&self, cfg: &mut actix_web::web::ServiceConfig)`: adds a `GET` WebSocket route at `ws_path`. A no-op without `devel`.

### TeraWebBuilder

Configures a `TeraWeb`.

* `pub struct TeraWebBuilder`
* `pub fn add_global<S: Into<String>, T: serde::Serialize>(mut self, key: S, value: T) -> Self`: a variable available to every template. A repeated key overwrites the earlier value.
* `pub fn configure_tera<F>(mut self, configurator: F) -> Self where F: FnOnce(&mut tera::Tera) + 'static`: a closure run against the empty engine before the templates load. A filter, function, test or component a template references must be registered here.
* `pub fn watch_static(mut self, path: &str) -> Self`: a directory the reloader watches recursively. Repeatable. A path that does not exist is skipped with a warning. Unused without `devel`.
* `pub fn ws_path(mut self, path: &str) -> Self`: the live-reload WebSocket path. Default `"/_snapfire/ws"`. Unused without `devel`.
* `pub fn auto_inject_script(mut self, enabled: bool) -> Self`: whether `InjectSnapFireScript` injects the client into responses for this state. Default `true`. Unused without `devel`.
* `pub fn build(self) -> Result<TeraWeb>`: runs the `configure_tera` closure, loads the glob and, under `devel`, starts the watcher.
* A glob that matches no files builds an engine with no templates.
* `SnapFireError::Tera` when a template fails to parse or references a name the closure did not register. `SnapFireError::Watcher` when the template directory cannot be watched.

## 2. Rendering

### Template

A render waiting for Actix to send it.

* `pub struct Template`. No public fields or methods.
* `impl actix_web::Responder for Template`, with `Body = BoxBody`: a `200` response with `Content-Type: text/html`, streamed. The render runs in `respond_to`.
* A render failure is logged at `error` and ends the body stream with an internal server error.

## 3. Live Reload

### InjectSnapFireScript

Actix middleware at `snapfire::actix::dev::InjectSnapFireScript`.

* `pub struct InjectSnapFireScript`. `Debug`, `Clone`, `Default`. Use with `App::wrap(InjectSnapFireScript::default())`.
* `impl<S, B> Transform<S, ServiceRequest> for InjectSnapFireScript`
* Under `devel`, a response whose `Content-Type` contains `text/html` is buffered whole and gets `<script data-snapfire-reload="true">` holding the client inserted before the first `</body>`, matched case-insensitively. A response without `</body>` gets it appended.
* The `ws_path` and `auto_inject_script` are read from the `web::Data<TeraWeb>` registered as app data. Without that app data the script is injected with the default path.
* Without `devel`, every response passes through unchanged.

### WebSocket messages

Text frames the server sends on `ws_path` under `devel`.

* `reload`: a `.html`, `.tera` or `.jinja` file was created or modified under the glob's base directory or a `watch_static` directory. The engine has already run `Tera::full_reload`.
* `reload-css`: a `.css` file was created or modified there.
* The stock client reloads the page on `reload` and adds a cache-busting `_` query parameter to every `link[rel='stylesheet']` on `reload-css`. It retries a closed connection once a second, ten times.

## 4. Features

### devel

* Off by default. Adds the file watcher, the WebSocket route, the injecting middleware and the `SnapFireError::Watcher` variant.
* Every method and type listed here exists without it.
* The crate enables `glob_fs` and `fast` on `tera`.

## 5. Error Handling

### SnapFireError

`#[non_exhaustive]`. `Debug`, `std::error::Error`.

* `Tera(tera::Error)`: loading, parsing or rendering a template. Displays as `Tera error: <error>`.
* `Io(std::io::Error)`: displays as `IO error: <error>`.
* `Serialization(String)`: serializing a context. Displays as `Context serialization error: <message>`.
* `Watcher(notify::Error)`: the file watcher. Feature `devel`. Displays as `File watcher error: <error>`.

### Result

* `pub type Result<T, E = SnapFireError> = std::result::Result<T, E>`
