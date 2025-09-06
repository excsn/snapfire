use crate::error::{Result, SnapfireError};

use parking_lot::RwLock;
use serde::Serialize;
use std::sync::Arc;
use tera::{Context, Tera};

#[cfg(feature = "dev-reload")]
use crate::core::reload::DevReloader;

/// A framework-agnostic representation of a template to be rendered.
///
/// This struct holds all the necessary information for a render operation.
/// It is created by the `TeraWeb::render` method. Web framework integration
/// layers can then use this struct to implement their native response traits.
pub struct Template {
  // It remains pub(crate) to hide implementation details.
  pub(crate) app_state: TeraWeb,
  pub(crate) template_name: String,
  pub(crate) context: Context,
}

/// The primary application state for Snapfire, designed to be shared across threads.
///
/// It holds the Tera templating engine and all configuration. It is created using
/// the `TeraWeb::builder()` method.
#[derive(Clone, Debug)]
pub struct TeraWeb {
  /// The Tera instance, wrapped for thread-safe access and mutability (for reloads).
  pub(crate) tera: Arc<RwLock<Tera>>,
  /// The pre-built global context, shared across all requests.
  pub(crate) global_context: Arc<Context>,
  /// The live-reload controller, present only when the `dev-reload` feature is enabled.
  #[cfg(feature = "dev-reload")]
  pub(crate) reloader: Arc<DevReloader>,
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
  /// global context, and renders the template to a string.
  pub(crate) fn render_with_context(&self, tpl: &str, user_context: Context) -> Result<String> {
    let tera = self.tera.read();

    // 1. Start with a clone of our base globals.
    let mut final_context = (*self.global_context).clone();

    // 2. Extend it with the context the user supplied.
    //    The user's values will overwrite the globals, which is correct.
    final_context.extend(user_context);

    // 3. Render.
    let body = tera.render(tpl, &final_context).map_err(SnapfireError::Tera)?;

    Ok(body)
  }

  // The `render` method now lives in the CORE. It is a simple,
  // synchronous constructor for the `Template` struct.
  pub fn render(&self, tpl: &str, context: Context) -> Template {
    Template {
      app_state: self.clone(),
      template_name: tpl.to_string(),
      context,
    }
  }

  #[cfg(feature = "dev-reload")]
  pub(crate) fn get_reloader_broadcaster(&self) -> tokio::sync::broadcast::Sender<crate::core::reload::ReloadMessage> {
    self.reloader.broadcaster.clone()
  }
}

/// A builder for creating a configured `TeraWeb` instance.
pub struct TeraWebBuilder {
  templates_glob: String,
  globals: Context,
  // A closure to run on the Tera instance for advanced configuration.
  // We use `Box<dyn...>` to store the closure in the struct.
  tera_configurator: Option<Box<dyn FnOnce(&mut Tera)>>,
  #[cfg(feature = "dev-reload")]
  static_paths_to_watch: Vec<String>,
  #[cfg(feature = "dev-reload")]
  ws_path: String,
  #[cfg(feature = "dev-reload")]
  auto_inject_script: bool,
}

impl TeraWebBuilder {
  /// Creates a new builder with a specified template glob pattern.
  pub(crate) fn new(templates_glob: &str) -> Self {
    Self {
      templates_glob: templates_glob.to_string(),
      globals: Context::new(),
      tera_configurator: None,
      #[cfg(feature = "dev-reload")]
      static_paths_to_watch: Vec::new(),
      #[cfg(feature = "dev-reload")]
      ws_path: "/_snapfire/ws".to_string(),
      #[cfg(feature = "dev-reload")]
      auto_inject_script: true,
    }
  }

  /// Adds a global variable that will be available to all templates.
  ///
  /// This can be called multiple times to add multiple globals.
  ///
  /// # Arguments
  ///
  /// * `key` - The name of the variable in the template (e.g., "site_name").
  /// * `value` - Any value that can be serialized (e.g., a string, a number, a struct).
  pub fn add_global<S: Into<String>, T: Serialize>(mut self, key: S, value: T) -> Self {
    self.globals.insert(&key.into(), &value);
    self
  }

  /// Provides a closure to run for advanced configuration of the `Tera` instance.
  ///
  /// This is the escape hatch for power users to register custom functions,
  /// filters, testers, or modify Tera settings before the app is finalized.
  pub fn configure_tera<F>(mut self, configurator: F) -> Self
  where
    F: FnOnce(&mut Tera) + 'static,
  {
    self.tera_configurator = Some(Box::new(configurator));
    self
  }

  /// Sets the path for the dev-reload WebSocket endpoint.
  ///
  /// Defaults to `/_snapfire/ws`.
  #[cfg(feature = "dev-reload")]
  pub fn ws_path(mut self, path: &str) -> Self {
    self.ws_path = path.to_string();
    self
  }

  /// Enables or disables the automatic injection of the
  /// live-reload JavaScript.
  ///
  /// Defaults to `true`. Set this to `false` if you want to manually
  /// include the script in your base template.
  #[cfg(feature = "dev-reload")]
  pub fn auto_inject_script(mut self, enabled: bool) -> Self {
    self.auto_inject_script = enabled;
    self
  }

  /// Adds a path to a static directory to watch for changes.
  ///
  /// This is typically used for CSS files. Can be called multiple times.
  #[cfg(feature = "dev-reload")]
  pub fn watch_static(mut self, path: &str) -> Self {
    self.static_paths_to_watch.push(path.to_string());
    self
  }

  /// Consumes the builder to construct the final `TeraWeb` application state.
  ///
  /// This method will initialize the Tera engine and, if the `dev-reload` feature
  /// is enabled, spawn the file watcher.
  pub fn build(self) -> Result<TeraWeb> {
    // 1. Create the initial Tera instance.
    let mut tera = Tera::new(&self.templates_glob)?;

    // 2. Run the power-user configuration closure if it exists.
    if let Some(configurator) = self.tera_configurator {
      configurator(&mut tera);
    }

    // 3. Wrap the Tera instance for thread-safe sharing.
    let tera = Arc::new(RwLock::new(tera));

    // 4. Construct the final TeraWeb state.
    Ok(TeraWeb {
      // Conditionally start the reloader if the `dev-reload` feature is enabled.
      #[cfg(feature = "dev-reload")]
      reloader: {
        let reloader = DevReloader::start(
          Arc::clone(&tera),
          &self.templates_glob,
          self.static_paths_to_watch,
          self.ws_path,
          self.auto_inject_script,
        )?;
        Arc::new(reloader)
      },
      // If `dev-reload` is not enabled, the `reloader` field does not exist.
      // The code in the block above is not compiled.
      tera, // This moves the `tera` Arc into the struct
      global_context: Arc::new(self.globals),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use tempfile::tempdir;

  // Helper function to create a `TeraWeb` instance for testing.
  // It creates a temporary directory for templates.
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
    let app = setup_test_app("site_name", "Snapfire Test", "Hello, {{ site_name }}!").await;
    let user_context = Context::new(); // Empty user context

    let result = app.render_with_context("index.html", user_context);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, Snapfire Test!");
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
    user_context.insert("title", "Page Title"); // This should win

    let result = app.render_with_context("index.html", user_context);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Title: Page Title");
  }

  #[tokio::test]
  async fn test_render_fails_when_template_not_found() {
    // Tera::new() succeeds even with a bad glob, as it loads lazily.
    let app = TeraWeb::builder("/invalid/path/that/does/not/exist/**/*.html")
      .build()
      .unwrap(); // This should NOT fail.

    let user_context = Context::new();
    // The error should happen here, when we try to render a non-existent template.
    let result = app.render_with_context("non_existent.html", user_context);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), SnapfireError::Tera(_)));
  }

  #[test]
  fn test_configure_tera_hook() {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("index.html");
    fs::write(&template_path, "Hello, {{ name | upcase }}!").unwrap();
    let glob_path = temp_dir.path().join("*.html").to_str().unwrap().to_string();

    // A custom filter function
    fn upcase_filter(
      value: &tera::Value,
      _: &std::collections::HashMap<String, tera::Value>,
    ) -> tera::Result<tera::Value> {
      let s = tera::from_value::<String>(value.clone())?;
      Ok(tera::to_value(s.to_uppercase()).unwrap())
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
}
