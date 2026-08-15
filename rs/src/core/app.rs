use crate::error::{Result, SnapFireError};

use parking_lot::RwLock;
use serde::Serialize;
use std::sync::Arc;
use tera::{Context, Tera};

#[cfg(feature = "devel")]
use crate::core::reload::DevReloader;

pub(crate) const DEFAULT_WS_PATH: &str = "/_snapfire/ws";

/// A framework-agnostic representation of a template to be rendered.
///
/// This struct holds all the necessary information for a render operation.
/// It is created by the `TeraWeb::render` method. Web framework integration
/// layers (like `snapfire::actix`) implement their native response traits on this struct.
pub struct Template {
  pub(crate) app_state: TeraWeb,
  pub(crate) template_name: String,
  pub(crate) context: Context,
}

/// The primary application state for SnapFire, designed to be shared across threads.
///
/// It holds the Tera templating engine and all configuration. It is created using
/// the `TeraWeb::builder()` method and shared with Actix handlers via `web::Data`.
#[derive(Clone)]
pub struct TeraWeb {
  /// The Tera instance, wrapped for thread-safe access and mutability (for reloads).
  pub(crate) tera: Arc<RwLock<Tera>>,
  /// The pre-built global context, shared across all requests.
  pub(crate) global_context: Arc<Context>,
  /// The live-reload controller, present only when the `devel` feature is enabled.
  #[cfg(feature = "devel")]
  pub(crate) reloader: Arc<DevReloader>,
  /// Read by `InjectSnapFireScript` on every HTML response. It lives here rather than
  /// on the reloader because injection is a response-rewriting concern, and the
  /// middleware reaches this struct through Actix app data.
  #[cfg(feature = "devel")]
  pub(crate) auto_inject_script: bool,
}

impl std::fmt::Debug for TeraWeb {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let mut s = f.debug_struct("TeraWeb");
    s.field("global_context", &self.global_context);
    #[cfg(feature = "devel")]
    {
      s.field("reloader", &self.reloader);
      s.field("auto_inject_script", &self.auto_inject_script);
    }
    s.finish_non_exhaustive()
  }
}

impl TeraWeb {
  /// Creates a new `TeraWebBuilder` to configure and build a `TeraWeb` instance.
  ///
  /// This is the main entry point for using the library.
  ///
  /// # Arguments
  ///
  /// * `templates_glob` - A glob pattern (e.g., "templates/**/*.html") for Tera to find templates.
  pub fn builder(templates_glob: &str) -> TeraWebBuilder {
    TeraWebBuilder::new(templates_glob)
  }

  /// The internal, framework-agnostic rendering function.
  ///
  /// This takes a template name and a user-provided context, merges it with the
  /// global context and renders the template to a string. Keys present in both
  /// contexts take their value from the user-provided one.
  pub(crate) fn render_with_context(&self, tpl: &str, user_context: Context) -> Result<String> {
    let tera = self.tera.read();

    let mut final_context = (*self.global_context).clone();
    final_context.extend(user_context);

    let body = tera.render(tpl, &final_context).map_err(SnapFireError::Tera)?;

    Ok(body)
  }

  /// Prepares a template for rendering.
  ///
  /// This method is synchronous and returns a `Template` struct, which can then
  /// be returned from an Actix handler. The actual rendering is performed
  /// asynchronously by the framework when the response is being sent.
  pub fn render(&self, tpl: &str, context: Context) -> Template {
    Template {
      app_state: self.clone(),
      template_name: tpl.to_string(),
      context,
    }
  }

  /// Returns the live-reload client as JavaScript source, for embedding in a template.
  ///
  /// This is the same script [`InjectSnapFireScript`] injects, with the configured
  /// [`ws_path`](TeraWebBuilder::ws_path) already substituted. It is the source alone
  /// and carries no `<script>` tag, so the caller controls the element and can attach
  /// a Content-Security-Policy `nonce`.
  ///
  /// Pair it with [`auto_inject_script(false)`](TeraWebBuilder::auto_inject_script).
  /// Embedding it while injection is still enabled gives the page two reload clients
  /// and two WebSocket connections.
  ///
  /// Without the `devel` feature there is no reload client and this returns an empty
  /// string, so a template that embeds it renders an empty element in release builds.
  ///
  /// [`InjectSnapFireScript`]: crate::actix::dev::InjectSnapFireScript
  ///
  /// # Examples
  ///
  /// ```no_run
  /// # use snapfire::TeraWeb;
  /// # use tera::Context;
  /// # let app_state: TeraWeb = unimplemented!();
  /// let mut context = Context::new();
  /// context.insert("reload_script", &app_state.reload_script());
  /// ```
  ///
  /// ```html
  /// <script nonce="{{ csp_nonce }}">{{ reload_script | safe }}</script>
  /// ```
  pub fn reload_script(&self) -> String {
    #[cfg(feature = "devel")]
    {
      crate::core::reload::client_script(&self.reloader.ws_path)
    }
    #[cfg(not(feature = "devel"))]
    {
      String::new()
    }
  }

  #[cfg(feature = "devel")]
  pub(crate) fn get_reloader_broadcaster(&self) -> tokio::sync::broadcast::Sender<crate::core::reload::ReloadMessage> {
    self.reloader.broadcaster.clone()
  }
}

/// A builder for creating a configured `TeraWeb` instance.
pub struct TeraWebBuilder {
  templates_glob: String,
  globals: Context,
  tera_configurator: Option<Box<dyn FnOnce(&mut Tera)>>,
  static_paths_to_watch: Vec<String>,
  ws_path: String,
  auto_inject_script: bool,
}

impl TeraWebBuilder {
  /// Creates a new builder with a specified template glob pattern.
  pub(crate) fn new(templates_glob: &str) -> Self {
    Self {
      templates_glob: templates_glob.to_string(),
      globals: Context::new(),
      tera_configurator: None,
      static_paths_to_watch: Vec::new(),
      ws_path: DEFAULT_WS_PATH.to_string(),
      auto_inject_script: true,
    }
  }

  /// Adds a global variable that will be available to all templates rendered
  /// by this `TeraWeb` instance.
  ///
  /// This can be called multiple times to add multiple globals. If a key is
  /// added that already exists, the old value will be overwritten.
  ///
  /// # Arguments
  ///
  /// * `key` - The name of the variable in the template (e.g., "site_name").
  /// * `value` - Any value that can be serialized (e.g., a string, a number, a struct).
  pub fn add_global<S: Into<String>, T: Serialize>(mut self, key: S, value: T) -> Self {
    self.globals.insert(key.into(), &value);
    self
  }

  /// Provides a closure to run for advanced configuration of the `Tera` instance.
  ///
  /// This is the escape hatch for power users to register custom filters, functions,
  /// tests and components, or to change Tera settings.
  ///
  /// The closure runs against an empty engine, before the template glob is loaded.
  /// Tera resolves the names a template references while parsing it, so anything a
  /// template uses must be registered here or [`build`](Self::build) fails.
  pub fn configure_tera<F>(mut self, configurator: F) -> Self
  where
    F: FnOnce(&mut Tera) + 'static,
  {
    self.tera_configurator = Some(Box::new(configurator));
    self
  }

  /// Sets the path for the devel WebSocket endpoint.
  ///
  /// Defaults to `/_snapfire/ws`.
  pub fn ws_path(mut self, path: &str) -> Self {
    self.ws_path = path.to_string();
    self
  }

  /// Enables or disables automatic injection of the live-reload JavaScript.
  ///
  /// Defaults to `true`, which lets `InjectSnapFireScript` rewrite every `text/html`
  /// response to carry the reload client. Set it to `false` when that rewriting is
  /// wrong for your application:
  ///
  /// * **A strict Content-Security-Policy.** The injected `<script>` is inline and
  ///   carries no `nonce`, so a CSP without `unsafe-inline` blocks it.
  /// * **Partial HTML responses.** Injection applies to anything typed `text/html`,
  ///   and a response with no `</body>` tag gets the script appended to the end. A
  ///   fragment endpoint therefore ships a reload client with every fragment.
  /// * **Placement.** Injection always targets the end of `<body>`. Nothing else is
  ///   configurable, including ordering against your own scripts.
  /// * **Streaming.** Injection buffers the whole response body in order to find the
  ///   closing tag.
  ///
  /// Turning it off leaves the watcher and the WebSocket route running, so the server
  /// half of live reload still works and it is up to the page to connect. Embed
  /// [`TeraWeb::reload_script`] in a `<script>` element of your own to get the stock
  /// client back under your control, or open [`ws_path`](Self::ws_path) yourself and
  /// act on the two messages the server broadcasts: `reload` and `reload-css`.
  ///
  /// Has no effect without the `devel` feature, where nothing is injected regardless.
  pub fn auto_inject_script(mut self, enabled: bool) -> Self {
    self.auto_inject_script = enabled;
    self
  }

  /// Adds a path to a static directory to watch for changes.
  ///
  /// This is typically used for CSS files. Can be called multiple times.
  pub fn watch_static(mut self, path: &str) -> Self {
    self.static_paths_to_watch.push(path.to_string());
    self
  }

  /// Consumes the builder to construct the final `TeraWeb` application state.
  ///
  /// This method will initialize the Tera engine and, if the `devel` feature
  /// is enabled, spawn the file watcher.
  ///
  /// # Errors
  ///
  /// Returns [`SnapFireError::Tera`] if a matched template fails to parse or references
  /// a filter, function, test or component that the [`configure_tera`](Self::configure_tera)
  /// closure did not register. A glob that matches no files is not an error: the engine
  /// is built with no templates. With the `devel` feature, returns
  /// [`SnapFireError::Watcher`] if the template directory cannot be watched.
  pub fn build(self) -> Result<TeraWeb> {
    let mut tera = Tera::new();

    // Tera resolves filters, functions, tests and components at parse time, so
    // every registration must happen before the templates are loaded.
    if let Some(configurator) = self.tera_configurator {
      configurator(&mut tera);
    }

    tera.load_from_glob(&self.templates_glob)?;

    let tera = Arc::new(RwLock::new(tera));

    Ok(TeraWeb {
      #[cfg(feature = "devel")]
      reloader: {
        let reloader = DevReloader::start(
          Arc::clone(&tera),
          &self.templates_glob,
          self.static_paths_to_watch,
          self.ws_path,
        )?;
        Arc::new(reloader)
      },
      #[cfg(feature = "devel")]
      auto_inject_script: self.auto_inject_script,
      tera,
      global_context: Arc::new(self.globals),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use tempfile::tempdir;

  async fn setup_test_app(global_key: &str, global_value: &str, template_content: &str) -> TeraWeb {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("index.html");
    fs::write(&template_path, template_content).unwrap();

    let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

    TeraWeb::builder(&glob_path)
      .add_global(global_key, global_value)
      .build()
      .unwrap()
  }

  #[tokio::test]
  async fn test_render_with_global_context() {
    let app = setup_test_app("site_name", "SnapFire Test", "Hello, {{ site_name }}!").await;
    let user_context = Context::new();

    let result = app.render_with_context("index.html", user_context);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, SnapFire Test!");
  }

  #[tokio::test]
  async fn test_render_with_user_context() {
    let app = setup_test_app("site_name", "Global", "Hello, {{ user_name }}!").await;
    let mut user_context = Context::new();
    user_context.insert("user_name", "Alice");

    let result = app.render_with_context("index.html", user_context);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, Alice!");
  }

  #[tokio::test]
  async fn test_user_context_overrides_global_context() {
    let app = setup_test_app("title", "Global Title", "Title: {{ title }}").await;
    let mut user_context = Context::new();
    user_context.insert("title", "Page Title");

    let result = app.render_with_context("index.html", user_context);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Title: Page Title");
  }

  #[test]
  fn test_bad_glob_behavior() {
    let builder = TeraWeb::builder("/invalid/path/that/does/not/exist/*.html");

    #[cfg(feature = "devel")]
    {
      let result = builder.build();
      assert!(result.is_err());
      assert!(matches!(result.unwrap_err(), SnapFireError::Watcher(_)));
    }

    #[cfg(not(feature = "devel"))]
    {
      // A glob matching no files loads no templates and is not an error.
      let app = builder.build().unwrap();
      let result = app.render_with_context("non_existent.html", Context::new());
      assert!(matches!(result.unwrap_err(), SnapFireError::Tera(_)));
    }
  }

  #[test]
  fn test_configure_tera_hook() {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("index.html");
    fs::write(&template_path, "Hello, {{ name | upcase }}!").unwrap();
    let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

    fn upcase_filter(value: &str, _: tera::Kwargs, _: &tera::State) -> String {
      value.to_uppercase()
    }

    let app = TeraWeb::builder(&glob_path)
      .configure_tera(|tera| {
        tera.register_filter("upcase", upcase_filter);
      })
      .build()
      .unwrap();

    let mut context = Context::new();
    context.insert("name", "world");
    let result = app.render_with_context("index.html", context);

    assert_eq!(result.unwrap(), "Hello, WORLD!");
  }

  #[test]
  fn test_build_fails_when_template_uses_unregistered_filter() {
    let temp_dir = tempdir().unwrap();
    fs::write(temp_dir.path().join("index.html"), "Hello, {{ name | upcase }}!").unwrap();
    let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

    let result = TeraWeb::builder(&glob_path).build();

    assert!(matches!(result.unwrap_err(), SnapFireError::Tera(_)));
  }

  #[tokio::test]
  async fn test_undefined_variable_is_an_error() {
    let app = setup_test_app("site_name", "SnapFire Test", "Hello, {{ absent }}!").await;

    let result = app.render_with_context("index.html", Context::new());

    assert!(matches!(result.unwrap_err(), SnapFireError::Tera(_)));
  }

  #[tokio::test]
  async fn test_component_render() {
    let temp_dir = tempdir().unwrap();
    fs::write(
      temp_dir.path().join("components.html"),
      "{% component greeting(name) %}Hi {{ name }}!{% endcomponent greeting %}",
    )
    .unwrap();
    fs::write(temp_dir.path().join("index.html"), r#"{{ <greeting name="Bob"/> }}"#).unwrap();
    let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

    let app = TeraWeb::builder(&glob_path).build().unwrap();

    let result = app.render_with_context("index.html", Context::new());

    assert_eq!(result.unwrap(), "Hi Bob!");
  }

  #[cfg(feature = "devel")]
  #[tokio::test]
  async fn test_reload_preserves_registered_filters() {
    fn upcase_filter(value: &str, _: tera::Kwargs, _: &tera::State) -> String {
      value.to_uppercase()
    }

    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("index.html");
    fs::write(&template_path, "{{ name | upcase }}").unwrap();
    let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

    let app = TeraWeb::builder(&glob_path)
      .configure_tera(|tera| {
        tera.register_filter("upcase", upcase_filter);
      })
      .build()
      .unwrap();

    let render = || {
      let mut context = Context::new();
      context.insert("name", "world");
      app.render_with_context("index.html", context)
    };

    assert_eq!(render().unwrap(), "WORLD");

    fs::write(&template_path, "[{{ name | upcase }}]").unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
      match render() {
        Ok(body) if body == "[WORLD]" => break,
        _ if std::time::Instant::now() >= deadline => panic!("template was never reloaded"),
        _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
      }
    }
  }
}
