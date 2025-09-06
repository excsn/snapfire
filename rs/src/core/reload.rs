use crate::error::{Result, SnapFireError};
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
      // ... event handler logic remains the same ...
      let event = match res {
        Ok(event) => event,
        Err(e) => {
          log::error!("File watch error: {:?}", e);
          return;
        }
      };

      if !(event.kind.is_modify() || event.kind.is_create()) {
        return;
      }

      for path in &event.paths {
        match path.extension().and_then(|s| s.to_str()) {
          Some("html") | Some("tera") | Some("jinja") => {
            log::info!("📝 Template change detected: {:?}", path);
            if let Err(e) = tera_clone.write().full_reload() {
              log::error!("Failed to reload templates: {}", e);
            }
            let _ = broadcaster_clone.send(ReloadMessage::Reload);
            return;
          }
          Some("css") => {
            log::info!("🎨 CSS change detected: {:?}", path);
            let _ = broadcaster_clone.send(ReloadMessage::ReloadCss);
            return;
          }
          _ => (),
        }
      }
    })?;

    // Use our new, robust function to get the path to watch.
    let template_watch_path = base_path_from_glob(template_glob);
    log::debug!("Watching template path: {}", template_watch_path);
    watcher
      .watch(std::path::Path::new(template_watch_path), RecursiveMode::Recursive)
      .map_err(SnapFireError::Watcher)?;

    // Watch all specified static asset paths.
    for path in &static_paths {
      if std::path::Path::new(path).exists() {
        watcher
          .watch(path.as_ref(), RecursiveMode::Recursive)
          .map_err(SnapFireError::Watcher)?;
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

/// Extracts the non-glob base path from a glob pattern.
///
/// This is necessary because `notify` cannot watch a glob pattern directly.
/// We need to find the deepest parent directory that does not contain
/// any special glob characters.
fn base_path_from_glob(glob: &str) -> &str {
  // Find the first occurrence of a glob character
  if let Some(first_glob_char_index) = glob.find(&['*', '?', '{', '[']) {
    // Take the slice of the string before that character
    let before_glob = &glob[..first_glob_char_index];
    // Find the last directory separator in that slice
    if let Some(last_separator_index) = before_glob.rfind('/') {
      // The base path is everything up to that separator
      &glob[..last_separator_index]
    } else {
      // No separator found before the glob, so watch the current directory
      "."
    }
  } else {
    // No glob characters found, the whole string is a path.
    // We still need to check if it's a file or directory.
    let path = std::path::Path::new(glob);
    if path.is_dir() {
      glob
    } else {
      path.parent().map_or(".", |p| p.to_str().unwrap_or("."))
    }
  }
}
