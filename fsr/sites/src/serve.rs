//! The stock host over an application with no crate of its own: what `fsr serve` runs in development and what the `fsr-server` binary runs in production.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_fsr_host::config::{Config, MountConfig};
use snapfire_fsr_host::trace::{LoggingGuard, Traces};
use snapfire_fsr_host::{Host, HostBuilder, HostError};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ServeError(pub String);

/// Where the host reads its configuration from: the directory beside the app
/// when it holds `config/` or an `app.toml`, else the app directory itself.
pub fn project_root(app: &Path) -> PathBuf {
  let app = app.canonicalize().unwrap_or_else(|_| app.to_path_buf());
  match app.parent() {
    Some(parent) if parent.join("config").is_dir() || parent.join("app.toml").is_file() || parent.join("app.yaml").is_file() => parent.to_path_buf(),
    _ => app,
  }
}

/// The builder the stock host builds from, with `mounts` laid over the
/// configuration's `[sites]` table: a row per name, naming the path, the
/// table's own row of that name replaced. The builder reloads the same way.
pub fn builder_with_mounts(root: &Path, mounts: &[(String, PathBuf)]) -> Result<HostBuilder, ServeError> {
  let build = {
    let root = root.to_path_buf();
    let mounts = mounts.to_vec();
    move || -> Result<HostBuilder, HostError> {
      let mut config = Config::load(&root)?;
      let section = config.sites.get_or_insert_with(Default::default);
      for (name, path) in &mounts {
        section.mounts.insert(name.clone(), MountConfig::new(Some(path.display().to_string()), None, false));
      }
      Host::from_config(config)
    }
  };
  let builder = build().map_err(|e| ServeError(e.to_string()))?;
  Ok(builder.reloader(move || build().map(crate::mountable).and_then(|b| crate::mount_all(b).map_err(|e| HostError::Mount("sites".to_owned(), e.to_string())))))
}

/// The stock host over `app`, refusing a configuration that names a different
/// app directory, with the handle to the trace collector when one was
/// installed and the logging guard to hold while it serves. `serve` sets the
/// process's logging: the project's `fibre_logging.yaml`, else warnings and
/// errors to stderr. Traces are collected under `server.dev` or when the
/// configuration exposes or inlines them.
pub fn host_of(app: &Path, serve: bool, mounts: &[(String, PathBuf)]) -> Result<(Host, Option<Traces>, Option<LoggingGuard>), ServeError> {
  let given = app.canonicalize().map_err(|e| ServeError(format!("{}: {e}", app.display())))?;
  let root = project_root(&given);
  let builder = match mounts.is_empty() {
    true => Host::from(&root).map_err(|e| ServeError(e.to_string()))?,
    false => builder_with_mounts(&root, mounts)?,
  };
  let config = builder.config();
  let configured = config.app.canonicalize().unwrap_or_else(|_| config.app.clone());
  if configured != given {
    return Err(ServeError(format!("{} names {} as the app directory, not {}", root.display(), config.app.display(), given.display())));
  }
  let (traces, logging) = match serve {
    true => {
      let (traces, logging, why) = snapfire_fsr_host::trace::observe_project(&root, config.dev() || config.trace.as_ref().is_some_and(|t| !t.expose.is_empty() || !t.inline.is_empty()));
      if let Some(why) = why {
        eprintln!("logging: {why}; warnings and errors go to stderr");
      }
      (traces, logging)
    }
    false => (None, None),
  };
  let builder = crate::mountable(builder);
  let builder = crate::mount_all(builder).map_err(|e| ServeError(e.to_string()))?;
  let host = builder.traces(traces.clone()).build().map_err(|e| ServeError(e.to_string()))?;
  Ok((host, traces, logging))
}

/// Serves `host` on `listen` until the process ends, the `[sites]` table read
/// from `root` watched. `bound` is told the address the listener took.
pub async fn listen(host: Arc<Host>, root: PathBuf, listen: &str, bound: impl FnOnce(&str) -> std::io::Result<()>) -> std::io::Result<()> {
  let poll = Config::load(&root).ok().as_ref().and_then(crate::poll_of);
  let listener = tokio::net::TcpListener::bind(listen).await?;
  let at = listener.local_addr().map_or_else(|_| listen.to_owned(), |addr| addr.to_string());
  bound(&at)?;
  crate::watch(host.clone(), root, poll);
  host.serve_listener(listener).await
}
