//! `fsr serve`: the stock host over an application with no Rust beside it, in
//! development. It runs `snapfire_fsr_sites::serve`, the code the production
//! binary `fsr-server` runs. The desktop window is the CLI's own.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::trace::Traces;
use snapfire_fsr_host::{Host, HostBuilder};

pub use snapfire_fsr_sites::serve::project_root;

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
  let dev = Config::load(&root).ok().as_ref().is_some_and(Config::dev);
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| BuildError::Serve(format!("runtime: {e}")))?;
  runtime
    .block_on(snapfire_fsr_sites::serve::listen(host, root, &listen, |at| {
      bound(at, tls).map_err(|e| std::io::Error::other(e.to_string()))?;
      println!("fsr server on {scheme}://{at}/");
      if traces.is_some() && dev {
        println!("fsr traces on {scheme}://{at}/__fsr/traces");
      }
      Ok(())
    }))
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
/// configuration's `[sites]` table; see `snapfire_fsr_sites::serve::builder_with_mounts`.
pub fn builder_with_mounts(root: &Path, mounts: &[(String, PathBuf)]) -> Result<HostBuilder, BuildError> {
  snapfire_fsr_sites::serve::builder_with_mounts(root, mounts).map_err(|e| BuildError::Serve(e.to_string()))
}

/// `host_for`, with the trace collector and the logging guard; see `snapfire_fsr_sites::serve::host_of`.
fn host_of(app: &Path, serve: bool, mounts: &[(String, PathBuf)]) -> Result<(Host, Option<Traces>, Option<snapfire_fsr_host::trace::LoggingGuard>), BuildError> {
  snapfire_fsr_sites::serve::host_of(app, serve, mounts).map_err(|e| BuildError::Serve(e.to_string()))
}
