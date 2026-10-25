//! `fsr serve`: the stock host over an application, for the project with no
//! Rust beside it. `Host::from` on the project root is all of it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_fsr_host::config::{Config, MountConfig};
use snapfire_fsr_host::trace::Traces;
use snapfire_fsr_host::{Host, HostBuilder};

use crate::BuildError;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ServeOptions {
  /// Overrides the configured `server.listen`.
  pub listen: Option<String>,
  /// Sites mounted from these paths for this run, by name, each replacing the
  /// table's row of that name or adding one: `fsr dev` serves a site under a
  /// shell artifact this way, with nothing written into the artifact.
  pub mounts: Vec<(String, PathBuf)>,
  /// Prints the whole boot report, `--verbose`; without it, `HostReport::brief`.
  pub verbose: bool,
  /// Opens the application in a native window, `--desktop`.
  #[cfg(feature = "desktop")]
  pub desktop: bool,
}

impl ServeOptions {
  pub fn listen(mut self, listen: Option<String>) -> Self {
    self.listen = listen;
    self
  }

  pub fn mounts(mut self, mounts: Vec<(String, PathBuf)>) -> Self {
    self.mounts = mounts;
    self
  }

  pub fn verbose(mut self, verbose: bool) -> Self {
    self.verbose = verbose;
    self
  }

  #[cfg(feature = "desktop")]
  pub fn desktop(mut self, desktop: bool) -> Self {
    self.desktop = desktop;
    self
  }

  /// The address to bind: `--listen`, else an ephemeral loopback port for a
  /// window, else the configured one.
  pub fn address(&self, configured: &str) -> String {
    #[cfg(feature = "desktop")]
    if self.desktop && self.listen.is_none() {
      return crate::desktop::LOOPBACK.to_owned();
    }
    self.listen.clone().unwrap_or_else(|| configured.to_owned())
  }
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

/// Builds the host for `app` and serves it until the process ends or until
/// its window closes under `--desktop`.
pub fn run(app: &Path, options: ServeOptions) -> Result<(), BuildError> {
  #[cfg(feature = "desktop")]
  if options.desktop {
    let app = app.to_path_buf();
    let section = Config::load(&project_root(&app)).ok().and_then(|config| config.desktop).unwrap_or_default();
    if section.serve == snapfire_fsr_host::config::DesktopServe::Scheme {
      if options.listen.is_some() {
        return Err(BuildError::Serve("desktop: `[desktop] serve = \"scheme\"` listens on no address, so it takes no --listen".to_owned()));
      }
      return scheming(&app, options, section.cookies);
    }
    return crate::desktop::run(&crate::desktop::name(&app), None, move |window| {
      serving(&app, options, move |bound, tls| crate::desktop::url(bound, tls).map(|url| window.open(url)))
    });
  }
  serving(app, options, |_, _| Ok(()))
}

/// `run` under `[desktop] serve = "scheme"`: the host answers the window's
/// `app://` requests in process and listens on nothing.
#[cfg(feature = "desktop")]
fn scheming(app: &Path, options: ServeOptions, cookies: snapfire_fsr_host::config::DesktopCookies) -> Result<(), BuildError> {
  let (host, _traces, logging) = host_of(app, true, &options.mounts)?;
  let host = Arc::new(host);
  if options.verbose {
    print!("{}", host.report());
  } else {
    print!("{}", host.report().brief());
  }
  let plan = app.join(crate::PLAN_FILE);
  let manifest = std::fs::read_to_string(&plan).ok().and_then(|text| snapfire_fsr_plan::Manifest::from_text(&text).ok());
  println!("fsr window on {} through the host in process, with no port", crate::desktop::SCHEME_URL);
  for limit in manifest.as_ref().map(crate::desktop::scheme_limits).unwrap_or_default() {
    println!("  scheme: {limit}");
  }
  let name = crate::desktop::name(app);
  let file = match cookies {
    snapfire_fsr_host::config::DesktopCookies::Memory => None,
    snapfire_fsr_host::config::DesktopCookies::Disk => crate::desktop::cookie_file(&name),
  };
  if let Some(file) = &file {
    println!("  cookies kept in {}", file.display());
  }
  let root = project_root(app);
  let poll = Config::load(&root).ok().as_ref().and_then(snapfire_fsr_sites::poll_of);
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| BuildError::Serve(format!("runtime: {e}")))?;
  let handler = crate::desktop::scheme(host.clone(), runtime.handle().clone(), file, options.verbose);
  crate::desktop::run(&name, Some(handler), move |_window| {
    let _logging = logging;
    runtime.block_on(async move {
      snapfire_fsr_sites::watch(host, root, poll);
      std::future::pending::<()>().await;
    });
    Ok(())
  })
}

/// `run`, telling `bound` the address it bound and whether it serves TLS.
fn serving(app: &Path, options: ServeOptions, bound: impl FnOnce(&str, bool) -> Result<(), BuildError>) -> Result<(), BuildError> {
  let (host, traces, _logging) = host_of(app, true, &options.mounts)?;
  let host = Arc::new(host);
  if options.verbose {
    print!("{}", host.report());
  } else {
    print!("{}", host.report().brief());
  }
  let listen = options.address(host.listen());
  let tls = host.report().tls.is_some();
  let scheme = match tls {
    true => "https",
    false => "http",
  };
  let root = project_root(app);
  let config = Config::load(&root).ok();
  let dev = config.as_ref().is_some_and(Config::dev);
  let poll = config.as_ref().and_then(snapfire_fsr_sites::poll_of);
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| BuildError::Serve(format!("runtime: {e}")))?;
  runtime.block_on(async {
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    let listen = listener.local_addr().map_or_else(|_| listen.clone(), |addr| addr.to_string());
    bound(&listen, tls).map_err(|e| std::io::Error::other(e.to_string()))?;
    println!("fsr server on {scheme}://{listen}/");
    if traces.is_some() && dev {
      println!("fsr traces on {scheme}://{listen}/__fsr/traces");
    }
    snapfire_fsr_sites::watch(host.clone(), root, poll);
    host.serve_listener(listener).await
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
  host_of(app, false, &[]).map(|(host, _, _)| host)
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
        section.mounts.insert(name.clone(), MountConfig::new(Some(path.display().to_string()), None, false));
      }
      Host::from_config(config)
    }
  };
  let builder = build().map_err(|e| BuildError::Serve(e.to_string()))?;
  Ok(builder.reloader(move || build().map(snapfire_fsr_sites::mountable).and_then(|b| snapfire_fsr_sites::mount_all(b).map_err(|e| snapfire_fsr_host::HostError::Mount("sites".to_owned(), e.to_string())))))
}

/// `host_for`, with the handle to the collector when one was installed and
/// the logging guard to hold while it serves. `serve` sets the process's
/// logging: the project's `fibre_logging.yaml`, else warnings and errors to
/// stderr. Only a development host collects. A span costs an atomic load and
/// a branch while nothing keeps it; the endpoint that serves them back is not
/// registered outside development.
fn host_of(app: &Path, serve: bool, mounts: &[(String, PathBuf)]) -> Result<(Host, Option<Traces>, Option<snapfire_fsr_host::trace::LoggingGuard>), BuildError> {
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
  let builder = snapfire_fsr_sites::mountable(builder);
  let builder = snapfire_fsr_sites::mount_all(builder).map_err(|e| BuildError::Serve(e.to_string()))?;
  let host = builder.traces(traces.clone()).build().map_err(|e| BuildError::Serve(e.to_string()))?;
  Ok((host, traces, logging))
}
