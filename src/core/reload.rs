use crate::error::{Result, SnapfireError};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::RwLock;
use std::sync::Arc;
use tera::Tera;
use tokio::sync::broadcast;

/// A message sent from the reloader to all connected clients.
#[derive(Debug, Clone)]
pub(crate) enum ReloadMessage {
  /// Instructs the client to do a full page reload.
  Reload,
  /// Instructs the client to only reload CSS stylesheets.
  ReloadCss,
}

/// The core, framework-agnostic live-reload controller.
///
/// It spawns a background task to watch for file changes and holds a
/// broadcast channel to send messages to connected clients.
#[derive(Debug)]
pub(crate) struct DevReloader {
  // We only store the sender. Receivers are created on demand.
  pub(crate) broadcaster: broadcast::Sender<ReloadMessage>,
  // The watcher is held in the struct to keep it alive. When `DevReloader`
  // is dropped, the watcher is dropped, and the background task will exit.
  _watcher: RecommendedWatcher,
  // Publicly expose the configuration for the Actix layer to use.
  pub(crate) ws_path: String,
  pub(crate) auto_inject_script: bool,
}

impl DevReloader {
  /// Creates a new `DevReloader` and starts the file watching task.
  pub(crate) fn start(
    tera: Arc<RwLock<Tera>>,
    template_glob: &str,
    static_paths: Vec<String>,
    ws_path: String,
    auto_inject_script: bool,
  ) -> Result<Self> {
    let (tx, _rx) = broadcast::channel(16);
    let broadcaster = tx.clone();

    // The watcher needs its own clones to move into the event handler.
    let tera_clone = tera.clone();
    let broadcaster_clone = broadcaster.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
      let event = match res {
        Ok(event) => event,
        Err(e) => {
          log::error!("File watch error: {:?}", e);
          return;
        }
      };

      // Check if the event is for a file modification or creation.
      if !(event.kind.is_modify() || event.kind.is_create()) {
        return;
      }

      for path in &event.paths {
        match path.extension().and_then(|s| s.to_str()) {
          Some("html") | Some("tera") | Some("jinja") => {
            log::info!("📝 Template change detected: {:?}", path);
            // Perform a blocking reload on the Tera instance.
            if let Err(e) = tera_clone.write().full_reload() {
              log::error!("Failed to reload templates: {}", e);
            }
            // Notify clients to do a full page reload.
            let _ = broadcaster_clone.send(ReloadMessage::Reload);
            // We found a template, no need to check other paths in this event.
            return;
          }
          Some("css") => {
            log::info!("🎨 CSS change detected: {:?}", path);
            // Notify clients to only reload CSS.
            let _ = broadcaster_clone.send(ReloadMessage::ReloadCss);
            // We found CSS, no need to check other paths in this event.
            return;
          }
          _ => (),
        }
      }
    })?;

    // Watch the template directory/glob.
    // We must watch the parent directory of the glob to detect new file creations.
    let template_parent = std::path::Path::new(template_glob).parent().ok_or_else(|| {
      SnapfireError::Watcher(
        // This is a simplified way to create a compatible error
        notify::Error::new(notify::ErrorKind::PathNotFound),
      )
    })?;

    watcher
      .watch(template_parent, RecursiveMode::Recursive)
      .map_err(SnapfireError::Watcher)?;

    // Watch all specified static asset paths.
    for path in &static_paths {
      if std::path::Path::new(path).exists() {
        watcher
          .watch(path.as_ref(), RecursiveMode::Recursive)
          .map_err(SnapfireError::Watcher)?;
      } else {
        log::warn!("Static path to watch does not exist, skipping: {}", path);
      }
    }

    Ok(Self {
      broadcaster,
      _watcher: watcher,
      ws_path,
      auto_inject_script,
    })
  }
}
