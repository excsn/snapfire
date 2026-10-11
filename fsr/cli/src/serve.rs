//! `fsr serve`: the stock host over an application, for the project with no
//! Rust beside it. `Host::from` on the project root is all of it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_fsr_host::config::{Config, MountConfig};
use snapfire_fsr_host::trace::Traces;
use snapfire_fsr_host::{Host, HostBuilder};

use crate::BuildError;

#[derive(Debug, Default)]
pub struct ServeOptions {
  /// Overrides the configured `server.listen`.
  pub listen: Option<String>,
  /// Sites mounted from these paths for this run, by name, each replacing the
  /// table's row of that name or adding one: `fsr dev` serves a site under a
  /// shell artifact this way, with nothing written into the artifact.
  pub mounts: Vec<(String, PathBuf)>,
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
  let (host, traces) = host_of(app, true, &options.mounts)?;
  let host = Arc::new(host);
  print!("{}", host.report());
  let listen = options.listen.unwrap_or_else(|| host.listen().to_owned());
  let scheme = match host.report().tls.is_some() {
    true => "https",
    false => "http",
  };
  println!("fsr server on {scheme}://{listen}/");
  if traces.is_some() {
    println!("fsr traces on {scheme}://{listen}/__fsr/traces");
  }
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
/// Warms every memoizable load and renders every prerenderable route of the
/// stock host into `out`, else `server.prerender` from the configuration, else
/// `dist/prerender` under the app.
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
  host_of(app, false, &[]).map(|(host, _)| host)
}

/// The builder `fsr serve` would build from, with `mounts` laid over the
/// configuration's `[sites]` table: a row per name, naming the path, the
/// table's own row of that name replaced. The builder reloads the same way.
pub fn builder_with_mounts(root: &Path, mounts: &[(String, PathBuf)]) -> Result<HostBuilder, BuildError> {
  let build = {
    let root = root.to_path_buf();
    let mounts = mounts.to_vec();
    move || -> Result<HostBuilder, snapfire_fsr_host::HostError> {
      let mut config = Config::load(&root)?;
      let section = config.sites.get_or_insert_with(Default::default);
      for (name, path) in &mounts {
        section.mounts.insert(name.clone(), MountConfig { artifact: Some(path.display().to_string()), hash: None, allow_engine: false, at: None, over: Vec::new(), timeout: None, retry: None, secret: None });
      }
      Host::from_config(config)
    }
  };
  let builder = build().map_err(|e| BuildError::Serve(e.to_string()))?;
  Ok(builder.reloader(move || build().map(snapfire_fsr_sites::mountable).and_then(|b| snapfire_fsr_sites::mount_all(b).map_err(|e| snapfire_fsr_host::HostError::Mount("sites".to_owned(), e.to_string())))))
}

/// `host_for`, with the handle to the collector when one was installed.
/// Only a development host collects. A span costs an atomic load and a branch
/// while nothing keeps it; the endpoint that serves them back is not registered
/// outside development.
fn host_of(app: &Path, collect: bool, mounts: &[(String, PathBuf)]) -> Result<(Host, Option<Traces>), BuildError> {
  let given = app.canonicalize().map_err(|e| BuildError::Io(app.to_path_buf(), e))?;
  let root = project_root(&given);
  let builder = match mounts.is_empty() {
    true => Host::from(&root).map_err(|e| BuildError::Serve(e.to_string()))?,
    false => builder_with_mounts(&root, mounts)?,
  };
  let config = builder.config();
  let configured = config.app.canonicalize().unwrap_or_else(|_| config.app.clone());
  if configured != given {
    return Err(BuildError::Serve(format!("{} names {} as the app directory, not {}", root.display(), config.app.display(), given.display())));
  }
  let traces = match collect && config.dev() {
    true => snapfire_fsr_host::trace::install(),
    false => None,
  };
  let builder = snapfire_fsr_sites::mountable(builder);
  let builder = snapfire_fsr_sites::mount_all(builder).map_err(|e| BuildError::Serve(e.to_string()))?;
  let host = builder.traces(traces.clone()).build().map_err(|e| BuildError::Serve(e.to_string()))?;
  Ok((host, traces))
}
