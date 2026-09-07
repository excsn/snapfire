use crate::error::{Result, SnapFireError};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::RwLock;
use std::sync::Arc;
use tera::Tera;
use fibre::spmc::topic::{AsyncTopicReceiver, channel_async};

const CLIENT_SCRIPT: &str = include_str!("../../resources/injected.js");
const WS_PATH_PLACEHOLDER: &str = "__SNAPFIRE_WS_PATH__";

/// Returns the browser-side live-reload client, with `ws_path` substituted for the
/// placeholder the script ships with.
pub(crate) fn client_script(ws_path: &str) -> String {
  CLIENT_SCRIPT.replace(WS_PATH_PLACEHOLDER, ws_path)
}

/// A message sent from the reloader to all connected clients.
#[derive(Debug, Clone)]
pub(crate) enum ReloadMessage {
  /// Instructs the client to do a full page reload.
  Reload,
  /// Instructs the client to only reload CSS stylesheets.
  ReloadCss,
}

/// How many messages a connection may fall behind before it loses one. A full
/// mailbox already holds a reload, so the one that is dropped would have asked
/// for what is already pending.
const MAILBOX: usize = 16;

/// The one key every reload rides on. The channel is a topic channel for its
/// fan-out, not because there is more than one thing to say.
type Key = ();

/// The core, framework-agnostic live-reload controller.
///
/// It spawns a background task to watch for file changes and holds a
/// broadcast channel to send messages to connected clients.
#[derive(Debug)]
pub(crate) struct DevReloader {
  /// Cloned once per connection: a clone takes a mailbox of its own and
  /// starts empty, so a client that connects now is not told about a reload
  /// that happened before it arrived.
  pub(crate) listener: AsyncTopicReceiver<Key, ReloadMessage>,
  // Held only to keep the watcher alive: dropping it stops the background task.
  _watcher: RecommendedWatcher,
  pub(crate) ws_path: String,
}

impl DevReloader {
  /// Creates a new `DevReloader` and starts the file watching task.
  pub(crate) fn start(
    tera: Arc<RwLock<Tera>>,
    template_glob: &str,
    static_paths: Vec<String>,
    ws_path: String,
  ) -> Result<Self> {
    // The sending half is not `Clone` and nothing outside the watcher sends,
    // so it moves into the closure and lives as long as the watcher does.
    let (broadcaster, listener) = channel_async(MAILBOX);

    let tera_clone = tera.clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
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
            let _ = broadcaster.send((), ReloadMessage::Reload);
            return;
          }
          Some("css") => {
            log::info!("🎨 CSS change detected: {:?}", path);
            let _ = broadcaster.send((), ReloadMessage::ReloadCss);
            return;
          }
          _ => (),
        }
      }
    })?;

    let template_watch_path = base_path_from_glob(template_glob);
    log::debug!("Watching template path: {}", template_watch_path);
    watcher
      .watch(std::path::Path::new(template_watch_path), RecursiveMode::Recursive)
      .map_err(SnapFireError::Watcher)?;

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
      listener,
      _watcher: watcher,
      ws_path,
    })
  }
}

/// Extracts the non-glob base path from a glob pattern.
///
/// This is necessary because `notify` cannot watch a glob pattern directly.
/// We need to find the deepest parent directory that does not contain
/// any special glob characters.
fn base_path_from_glob(glob: &str) -> &str {
  if let Some(first_glob_char_index) = glob.find(&['*', '?', '{', '[']) {
    let before_glob = &glob[..first_glob_char_index];
    if let Some(last_separator_index) = before_glob.rfind('/') {
      &glob[..last_separator_index]
    } else {
      "."
    }
  } else {
    let path = std::path::Path::new(glob);
    if path.is_dir() {
      glob
    } else {
      path.parent().map_or(".", |p| p.to_str().unwrap_or("."))
    }
  }
}
