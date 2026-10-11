use super::*;

/// What the host bound: the application's report plus the services it reaches
/// and the static roots it serves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostReport {
  pub app: Report,
  /// Whether a served connection negotiates HTTP/2 as well as HTTP/1.1.
  pub http2: bool,
  /// What the listener presents, when `[server.tls]` is configured.
  pub tls: Option<TlsReport>,
  /// Service, `http`, `grpc`, `mock` or `rust`, base URL, responses file or Rust type.
  pub services: Vec<(String, String, String)>,
  /// The client the sessions live behind, when the store is `service`.
  pub session: Option<String>,
  /// Method and policy for every method the data cache answers.
  pub cached: Vec<(String, String)>,
  /// Method and tags for every method that drops cached answers.
  pub writers: Vec<(String, String)>,
  pub statics: Vec<(String, PathBuf)>,
  /// Patterns `[[route]]` declares that nothing registered on this host: the stock host registers no Rust route, so an application's declarations show here until its own binary answers them.
  pub unanswered_routes: Vec<String>,
  /// The prefix the embedded client answers, how many modules it holds, what
  /// they come to and whether the build served is the minified one.
  /// `None` when a static root claims the prefix instead.
  pub client: Option<(&'static str, usize, usize, bool)>,
  /// Where prerendered documents are read from, when configured.
  pub prerender: Option<PathBuf>,
  /// How many of `app.warmable`'s keys the prerender directory answered at
  /// boot. Zero with sources listed means the warm pass has not run.
  pub warmed: usize,
  /// How many memo entries the prerender directory's renders file answered
  /// at boot. Zero with subtrees listed means the render pass has not run.
  pub rendered: usize,
  /// The render memo's capacity and lifetime, when configured.
  pub cache: Option<(u64, String)>,
  /// Whether the document carries the live-refresh script and the host
  /// answers `/__fsr/events` and `/__fsr/changed`.
  pub dev: bool,
  /// The configured locales, the default first; empty without a `[locales]` section.
  pub locales: Vec<String>,
  /// The hosts a request's `Host` is matched against, `server.hosts`; empty
  /// when the header is never read.
  pub hosts: Vec<String>,
  /// `document.origin`, which a canonical or alternate href is absolute against.
  pub origin: Option<String>,
  /// The identity provider and its login page, when one is mounted.
  pub auth: Option<(String, String)>,
  /// Client, custody key: which clients send a bearer token.
  pub bearer: Vec<(String, String)>,
  /// The native pairs registered beside the standard library, by name.
  pub extensions: Vec<String>,
  /// Per locale with a catalog under `locales/`, how many keys its file holds.
  pub catalogs: Vec<(String, usize)>,
  /// The application's own `[site]`: its name and prefix, when it is one.
  pub site: Option<(String, String)>,
  /// The sites mounted under this host: name, prefix, artifact, version and hash.
  pub sites: Vec<SiteReport>,
  /// The `script-src` source covering the document's inline import map, which
  /// is the only executable inline script a page carries. `None` without an
  /// import map. A deployment writing a Content-Security-Policy copies this in;
  /// it changes whenever the map does, so it is read from a boot rather than
  /// computed from any one file on disk.
  pub import_map_csp: Option<String>,
  /// The `Content-Security-Policy` every HTML response carries, `document.csp`
  /// with the import map's source already substituted; `None` when none is set.
  pub csp: Option<String>,
  /// The same for `Content-Security-Policy-Report-Only`, which a browser
  /// reports against and never enforces.
  pub csp_report_only: Option<String>,
  pub config: Vec<PathBuf>,
  pub inferred: Vec<String>,
  /// `[public]` as key and value, what `ctx.config` answers.
  pub public: Vec<(String, String)>,
  /// Top-level configuration keys the host does not own and left alone.
  pub ignored: Vec<String>,
}

/// The configured certificate, what the handshake offers and what re-reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsReport {
  pub cert: PathBuf,
  pub key: PathBuf,
  pub alpn: Vec<String>,
  /// The signal that re-reads the files; `None` for `none`.
  pub reload: Option<String>,
}

/// One mounted site in the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteReport {
  pub name: String,
  pub at: String,
  pub artifact: PathBuf,
  pub version: String,
  pub hash: String,
  /// Rows of the site's configuration the shell ignored, `session` and the like.
  pub ignored: Vec<String>,
}

impl std::fmt::Display for HostReport {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.app)?;
    for pattern in &self.unanswered_routes {
      writeln!(f, "{:<9} {pattern:<22} declared by [[route]], registered by nothing here", "routes")?;
    }
    if let Some((name, at)) = &self.site {
      writeln!(f, "{:<9} {name:<22} at {at}", "site")?;
    }
    for (i, site) in self.sites.iter().enumerate() {
      let label = if i == 0 { "sites" } else { "" };
      writeln!(
        f,
        "{label:<9} {:<22} at {} from {} {} {}",
        site.name,
        site.at,
        site.artifact.display(),
        site.version,
        site.hash
      )?;
      if !site.ignored.is_empty() {
        writeln!(
          f,
          "{:<9} {:<22} ignored [{}], the shell's",
          "",
          site.name,
          site.ignored.join(", ")
        )?;
      }
    }
    for (i, (service, kind, url)) in self.services.iter().enumerate() {
      let label = if i == 0 { "services" } else { "" };
      writeln!(f, "{label:<9} {service:<22} {kind:<11} {url}")?;
    }
    for (i, (route, dir)) in self.statics.iter().enumerate() {
      let label = if i == 0 { "static" } else { "" };
      writeln!(f, "{label:<9} {route:<22} {}", dir.display())?;
    }
    if let Some((route, files, bytes, minified)) = self.client {
      let build = if minified { "minified" } else { "readable" };
      writeln!(f, "{:<9} {route:<22} {files} modules, {build}, {} KiB from the binary", "client", bytes / 1024)?;
    }
    if let Some(source) = &self.import_map_csp {
      writeln!(f, "{:<9} {:<22} {source}", "csp", "import map")?;
    }
    match (&self.csp, self.dev) {
      (Some(policy), _) => writeln!(f, "{:<9} {:<22} {policy}", "", "sent as")?,
      (None, true) => writeln!(f, "{:<9} {:<22} no policy sent, `dev` is on", "", "sent as")?,
      (None, false) => writeln!(f, "{:<9} {:<22} no policy sent, `document.csp` is unset", "", "sent as")?,
    }
    if let Some(policy) = &self.csp_report_only {
      writeln!(f, "{:<9} {:<22} {policy}", "", "reported as")?;
    }
    for (i, (pattern, anonymous)) in self
      .app
      .prerenderable
      .iter()
      .map(|p| (p, false))
      .chain(self.app.prerenderable_anonymous.iter().map(|p| (p, true)))
      .enumerate()
    {
      let label = if i == 0 { "prerender" } else { "" };
      let who = if anonymous { " for anonymous visitors" } else { "" };
      let each = if self.app.paths.contains(pattern) { " per paths" } else { "" };
      match &self.prerender {
        Some(dir) => writeln!(f, "{label:<9} {pattern:<22} {}{who}{each}", dir.display())?,
        None => writeln!(f, "{label:<9} {pattern:<22} not configured{who}{each}")?,
      }
    }
    for (i, source) in self.app.warmable.iter().enumerate() {
      match (i, self.warmed) {
        (0, 0) => writeln!(f, "{:<9} {source:<22} not warmed", "warm")?,
        (0, n) => writeln!(f, "{:<9} {source:<22} {n} loads memoized", "warm")?,
        _ => writeln!(f, "{:<9} {source}", "")?,
      }
    }
    for (i, (pattern, module)) in self.app.renderable.iter().enumerate() {
      let label = if i == 0 { "render" } else { "" };
      match (i, self.rendered) {
        (0, 0) => writeln!(f, "{label:<9} {pattern:<22} {module} not rendered")?,
        (0, n) => writeln!(f, "{label:<9} {pattern:<22} {module} {n} renders memoized")?,
        _ => writeln!(f, "{label:<9} {pattern:<22} {module}")?,
      }
    }
    if let Some((capacity, ttl)) = &self.cache {
      writeln!(f, "{:<9} {capacity} entries, ttl {ttl}", "cache")?;
    }
    for (i, (method, policy)) in self.cached.iter().enumerate() {
      let label = if i == 0 { "cached" } else { "" };
      writeln!(f, "{label:<9} {method:<22} {policy}")?;
    }
    for (i, (method, tags)) in self.writers.iter().enumerate() {
      let label = if i == 0 { "writes" } else { "" };
      writeln!(f, "{label:<9} {method:<22} {tags}")?;
    }
    if let Some(client) = &self.session {
      writeln!(f, "{:<9} service via {client}", "session")?;
    }
    if self.dev {
      writeln!(
        f,
        "{:<9} live refresh on /__fsr/events, told by POST /__fsr/changed",
        "dev"
      )?;
    }
    if let Some(origin) = &self.origin {
      writeln!(f, "{:<9} {origin}, which canonical and alternate hrefs are absolute against", "origin")?;
    }
    for (i, host) in self.hosts.iter().enumerate() {
      let label = if i == 0 { "hosts" } else { "" };
      writeln!(f, "{label:<9} {host}")?;
    }
    if !self.hosts.is_empty() {
      writeln!(
        f,
        "{:<9} ctx.host reads the request's Host against these; the server in front must set it; a client otherwise names its own",
        ""
      )?;
    }
    if self.http2 {
      match &self.tls {
        Some(tls) => writeln!(
          f,
          "{:<9} beside http/1.1, chosen by alpn [{}]",
          "http2",
          tls.alpn.join(", ")
        )?,
        None => writeln!(
          f,
          "{:<9} h2c beside http/1.1; a browser wants alpn over tls, which a proxy in front terminates",
          "http2"
        )?,
      }
    }
    if let Some(tls) = &self.tls {
      writeln!(f, "{:<9} {} with {}", "tls", tls.cert.display(), tls.key.display())?;
      match &tls.reload {
        Some(signal) => writeln!(f, "{:<9} re-read on SIG{}", "", signal.to_uppercase())?,
        None => writeln!(f, "{:<9} re-read on nothing; a new certificate needs a restart", "")?,
      }
    }
    if let Some((default, others)) = self.locales.split_first() {
      let rest = if others.is_empty() {
        String::new()
      } else {
        format!(", {}", others.join(", "))
      };
      writeln!(f, "{:<9} {default} (default, unprefixed){rest}", "locales")?;
    }
    if let Some((provider, login)) = &self.auth {
      writeln!(
        f,
        "{:<9} {provider}, login page {login}, routes /auth/login, /auth/callback and /auth/logout",
        "auth"
      )?;
      if self.bearer.is_empty() {
        writeln!(f, "{:<9} none; no client carries a token", "bearer")?;
      }
    }
    for (i, (client, key)) in self.bearer.iter().enumerate() {
      let label = if i == 0 { "bearer" } else { "" };
      writeln!(f, "{label:<9} {client:<22} {key}")?;
    }
    for (i, name) in self.extensions.iter().enumerate() {
      let label = if i == 0 { "natives" } else { "" };
      writeln!(f, "{label:<9} {name:<22} rust")?;
    }
    if !self.catalogs.is_empty() {
      let rows: Vec<String> = self
        .catalogs
        .iter()
        .map(|(tag, n)| format!("{tag} {n} key{}", if *n == 1 { "" } else { "s" }))
        .collect();
      writeln!(f, "{:<9} {}", "catalogs", rows.join(", "))?;
    }
    for (i, source) in self.config.iter().enumerate() {
      let label = if i == 0 { "config" } else { "" };
      writeln!(f, "{label:<9} {}", source.display())?;
    }
    for (i, item) in self.inferred.iter().enumerate() {
      let label = if i == 0 { "inferred" } else { "" };
      writeln!(f, "{label:<9} {item}")?;
    }
    for (i, (key, value)) in self.public.iter().enumerate() {
      let label = if i == 0 { "public" } else { "" };
      writeln!(f, "{label:<9} {key:<22} {value}")?;
    }
    if !self.ignored.is_empty() {
      writeln!(f, "{:<9} {}, not the host's; left to the application's own store", "ignored", self.ignored.join(", "))?;
    }
    Ok(())
  }
}
