//! `fsr serve`: the stock host over an application, for the project with no
//! Rust beside it. `Host::from` on the project root is the whole of it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, HostError};

use crate::BuildError;

#[derive(Debug, Default)]
pub struct ServeOptions {
  /// Overrides the configured `server.listen`.
  pub listen: Option<String>,
}

/// Where the host reads its configuration from: the directory beside the app
/// when it holds `config/` or an `app.toml`, else the app directory itself.
pub fn project_root(app: &Path) -> PathBuf {
  let app = app.canonicalize().unwrap_or_else(|_| app.to_path_buf());
  match app.parent() {
    Some(parent) if parent.join("config").is_dir() || parent.join("app.toml").is_file() || parent.join("app.yaml").is_file() => parent.to_path_buf(),
    _ => app,
  }
}

/// Builds the host for `app` and serves it until the process ends.
pub fn run(app: &Path, options: ServeOptions) -> Result<(), BuildError> {
  let host = Arc::new(host_for(app)?);
  print!("{}", host.report());
  let listen = options.listen.unwrap_or_else(|| host.listen().to_owned());
  let scheme = match host.report().tls.is_some() {
    true => "https",
    false => "http",
  };
  println!("fsr server on {scheme}://{listen}/");
  let root = project_root(app);
  let poll = Config::load(&root).ok().and_then(|config| snapfire_fsr_sites::poll_of(&config));
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| BuildError::Serve(format!("runtime: {e}")))?;
  runtime.block_on(async {
    snapfire_fsr_sites::watch(host.clone(), root, poll);
    host.serve(&listen).await
  })
  .map_err(|e| BuildError::Serve(format!("serve {listen}: {e}")))
}

/// The stock host over `app`, refusing a configuration that names a different app directory.
/// Renders every prerenderable route of the stock host into `out`, else
/// `server.prerender` from the configuration, else `dist/prerender` under the app.
pub fn prerender(app: &Path, out: Option<&Path>) -> Result<Vec<(String, PathBuf)>, BuildError> {
  let host = host_for(app)?;
  let out = match out {
    Some(out) => out.to_path_buf(),
    None => host.report().prerender.clone().unwrap_or_else(|| app.join("dist/prerender")),
  };
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| BuildError::Serve(e.to_string()))?;
  runtime.block_on(host.prerender(&out)).map_err(|e| BuildError::Serve(e.to_string()))
}

pub fn host_for(app: &Path) -> Result<Host, BuildError> {
  let given = app.canonicalize().map_err(|e| BuildError::Io(app.to_path_buf(), e))?;
  let root = project_root(&given);
  let config = Config::load(&root).map_err(|e| BuildError::Serve(e.to_string()))?;
  let configured = config.app.canonicalize().unwrap_or_else(|_| config.app.clone());
  if configured != given {
    return Err(BuildError::Serve(format!("{} names {} as the app directory, not {}", root.display(), config.app.display(), given.display())));
  }
  let builder = Host::from_config(config).map_err(|e| BuildError::Serve(e.to_string()))?;
  let builder = snapfire_fsr_sites::mount_all(builder).map_err(|e| BuildError::Serve(e.to_string()))?;
  builder
    .reloader(move || snapfire_fsr_sites::mount_all(Host::from(&root)?).map_err(|e| HostError::Value("sites".to_owned(), e.to_string())))
    .build()
    .map_err(|e| BuildError::Serve(e.to_string()))
}
