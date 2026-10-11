//! `HostBuilder`: what a host is given before `build` and `assemble`, which turns the configuration, the plan, the contracts and the mounts into the tables a host serves from.

use super::*;

/// A `#[service]` block the builder was given: its contract, merged with
/// what the contracts directory holds, plus the transport that answers it.
pub(crate) struct RustService {
  pub(crate) name: String,
  pub(crate) rust_type: String,
  pub(crate) contract: Contract,
  pub(crate) transport: Arc<dyn Transport>,
}

pub struct HostBuilder {
  /// The trace collector, when one was installed. Served under development.
  pub(crate) traces: Option<trace::Traces>,
  pub(crate) artifact: Artifact,
  pub(crate) loader: Option<Loader>,
  pub(crate) app: Option<AppBuilder>,
  pub(crate) services: Option<Arc<Services>>,
  pub(crate) rust_services: Vec<RustService>,
  pub(crate) csrf: Option<Arc<dyn CsrfScheme>>,
  pub(crate) codec: Option<Arc<dyn CookieCodec>>,
  pub(crate) keyring: Option<Arc<Keyring>>,
  pub(crate) transport_override: Option<Arc<dyn Transport>>,
  pub(crate) store: Option<Arc<dyn SessionStore>>,
  pub(crate) shell: Option<Arc<dyn Evaluator>>,
  pub(crate) prerendered: Option<PathBuf>,
  pub(crate) identity: Option<Arc<dyn IdentityProvider>>,
  pub(crate) reloader: Option<Reloader>,
  pub(crate) sites_mounter: Option<SitesMounter>,
  pub(crate) topic_rule: Option<TopicRule>,
  #[cfg(feature = "ws")]
  pub(crate) sockets: Option<Arc<socket::Sockets>>,
  #[cfg(feature = "ws")]
  pub(crate) socket_handler: Option<socket::SocketHandler>,
  pub(crate) mounts: Vec<Mount>,
  /// Overrides `server.http2` for a host built in Rust.
  pub(crate) http2: Option<bool>,
  /// Whether a route, source, action, handler, evaluator, extension, native
  /// module or `#[service]` was registered on the builder, which a rebuild
  /// from the artifact alone would drop.
  pub(crate) hand_built: bool,
  pub(crate) pending: Option<HostError>,
}

impl std::fmt::Debug for HostBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("HostBuilder")
      .field("root", &self.artifact.config.root)
      .finish_non_exhaustive()
  }
}

impl HostBuilder {
  pub(crate) fn app_mut(&mut self, f: impl FnOnce(AppBuilder) -> AppBuilder) -> &mut Self {
    if let Some(app) = self.app.take() {
      self.app = Some(f(app));
    }
    self.hand_built = true;
    self
  }

  /// Replaces every client transport with one, keeping the contract the
  /// configuration names. Tests use it; so does a host that reaches its
  /// services some other way.
  pub fn services_over(mut self, transport: Arc<dyn Transport>) -> Self {
    self.transport_override = Some(transport);
    self
  }

  pub fn services(mut self, services: Arc<Services>) -> Self {
    self.services = Some(services);
    self
  }

  /// A service written in Rust, a `#[service]` block, served in process
  /// under its own name through the same registry, interceptors and cache a
  /// client goes through. Its contract joins the contracts directory's; a
  /// `fsr build` that read the block writes the same contract there and one
  /// that disagrees refuses to boot. `services_over` leaves it in place.
  pub fn service<T>(mut self, service: Arc<T>) -> Self
  where
    T: Transport + DeclaredService + 'static,
  {
    self.hand_built = true;
    self.rust_services.push(RustService {
      name: T::NAME.to_owned(),
      rust_type: std::any::type_name::<T>().to_owned(),
      contract: T::contract(),
      transport: service,
    });
    self
  }

  /// The CSRF scheme, in place of the one `session.csrf_scheme` names: one
  /// of the three `snapfire_fsr_session` ships or the application's own
  /// `CsrfScheme`.
  pub fn csrf(mut self, scheme: Arc<dyn CsrfScheme>) -> Self {
    self.csrf = Some(scheme);
    self
  }

  /// The cookie codec, in place of `HmacCodec` over the configuration's keys:
  /// the application's own signing or encryption of the session id.
  /// `session.key` and `session.previous_keys` then feed only a `derived`
  /// CSRF scheme. A builder given one is not rebuildable by a sites reload.
  pub fn codec(mut self, codec: Arc<dyn CookieCodec>) -> Self {
    self.codec = Some(codec);
    self
  }

  /// The keyring the stock codec and a `derived` scheme sign with, held and
  /// rotated by the application, in place of one built from `session.key`
  /// and `session.previous_keys`; a reload leaves it alone. A builder given
  /// one is not rebuildable by a sites reload.
  pub fn keyring(mut self, ring: Arc<Keyring>) -> Self {
    self.keyring = Some(ring);
    self
  }

  pub fn session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
    self.store = Some(store);
    self
  }

  /// Who may follow which topic on `/_sf/live`, asked once per topic as a
  /// stream opens; a refusal is 403 naming the topic. Without a rule any
  /// topic may be followed by anyone, which is right for a board on a wall
  /// and wrong for a room.
  pub fn topics<F>(mut self, rule: F) -> Self
  where
    F: Fn(&str, &SessionCell, Option<&Identity>) -> bool + Send + Sync + 'static,
  {
    self.topic_rule = Some(Arc::new(rule));
    self
  }

  /// The socket registry to use, for an application that must hold it before
  /// the host exists: a transport of its own pushes into the same one the
  /// host serves from. Without this the host makes its own.
  #[cfg(feature = "ws")]
  pub fn sockets(mut self, sockets: Arc<socket::Sockets>) -> Self {
    self.sockets = Some(sockets);
    self
  }

  /// What the application makes of what a page sends over `/_sf/socket`:
  /// called when a connection joins a topic, once per row it sends and when
  /// it leaves and whatever it answers goes out to that topic as store rows.
  /// Without one the endpoint is a 404. `HostBuilder::topics` still decides
  /// who may open the socket at all.
  #[cfg(feature = "ws")]
  pub fn socket<F>(mut self, handler: F) -> Self
  where
    F: Fn(&socket::Who, socket::On) -> socket::Reply + Send + Sync + 'static,
  {
    self.socket_handler = Some(Arc::new(handler));
    self
  }

  /// Negotiates HTTP/2 as well as HTTP/1.1 on a served connection, which
  /// `server.http2` also sets. The listener carries no TLS, so this is h2c.
  pub fn http2(mut self, on: bool) -> Self {
    self.http2 = Some(on);
    self
  }

  /// The identity provider behind `/auth/login`, `/auth/callback` and
  /// `/auth/logout`, in place of the one `[auth]` names. The login page is
  /// `auth.login` when the section is written, `/login` otherwise.
  pub fn identity(mut self, provider: Arc<dyn IdentityProvider>) -> Self {
    self.identity = Some(provider);
    self
  }

  /// The Rust half of a native pair: `name` is `module.member`, the name its
  /// `native(..)` declaration under `ext/` gives and `reach` what that
  /// declaration says. A plan calling a name nothing registers refuses to build.
  pub fn extension<F>(mut self, name: impl Into<String>, reach: Reach, f: F) -> Self
  where
    F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static,
  {
    let name = name.into();
    self.app_mut(move |app| app.extension(name, reach, f));
    self
  }

  /// The evaluator for the document module, replacing the stock shell.
  /// Where prerendered documents are read from, overriding `server.prerender`.
  pub fn prerendered(mut self, dir: impl Into<PathBuf>) -> Self {
    self.prerendered = Some(dir.into());
    self
  }

  pub fn shell(mut self, evaluator: Arc<dyn Evaluator>) -> Self {
    self.shell = Some(evaluator);
    self
  }

  pub fn route(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.app_mut(|app| app.route(pattern, plan));
    self
  }

  pub fn not_found(mut self, plan: impl IntoPlan) -> Self {
    self.app_mut(|app| app.not_found(plan));
    self
  }

  pub fn middleware<F, Fut>(mut self, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.middleware(f));
    self
  }

  pub fn middleware_override<F, Fut>(mut self, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.middleware_override(f));
    self
  }

  pub fn handler<F, Fut>(mut self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.handler(method, pattern, f));
    self
  }

  pub fn handler_override<F, Fut>(mut self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.handler_override(method, pattern, f));
    self
  }

  pub fn route_override(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.app_mut(|app| app.route_override(pattern, plan));
    self
  }

  pub fn source<F, Fut>(mut self, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    self.app_mut(|app| app.source(name, f));
    self
  }

  pub fn source_override<F, Fut>(mut self, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    self.app_mut(|app| app.source_override(name, f));
    self
  }

  /// The trace collector this host serves `/__fsr/traces` from under
  /// development. `trace::install` returns what goes here.
  pub fn traces(mut self, traces: Option<trace::Traces>) -> Self {
    self.traces = traces;
    self
  }

  /// Registers the application's own Rust under the name a body reaches it
  /// with, `ctx.native.<name>.<method>()`.
  pub fn native(mut self, name: impl Into<String>, module: Arc<dyn snapfire_fsr_runtime::Native>) -> Self {
    let name = name.into();
    self.app_mut(move |app| app.native(name, module));
    self
  }

  pub fn source_impl(mut self, name: impl Into<String>, source: Arc<dyn DataSource>) -> Self {
    self.app_mut(|app| app.source_impl(name, source));
    self
  }

  /// Describes the segment whose data source is `name`, title and
  /// description, once its data has loaded; the innermost described segment
  /// on a plan wins.
  pub fn meta(mut self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self {
    self.app_mut(|app| app.meta(name, meta));
    self
  }

  pub fn action<F, Fut>(mut self, id: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.action(id, f));
    self
  }

  /// One handler of an island a template renders. `module` is what the
  /// placement names and `name` is what its markup binds with
  /// `data-sf-on="click:<name>"`; the handler answers with the state the
  /// module is rendered from next.
  pub fn island_handler<F, Fut>(mut self, module: impl Into<String>, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, IslandEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.island_handler(module, name, f));
    self
  }

  pub fn action_override<F, Fut>(mut self, id: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.action_override(id, f));
    self
  }

  pub fn evaluator<P>(mut self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self
  where
    P: Fn(&ModuleId) -> bool + Send + Sync + 'static,
  {
    self.app_mut(|app| app.evaluator(predicate, evaluator));
    self
  }

  /// Mounts a site under the prefix its own `[site]` names: its routes join
  /// the shell's under the shell's root layout, its ids stay prefixed, its
  /// clients register under `<name>:`, its middleware runs after the shell's.
  pub fn mount(mut self, mount: Mount) -> Self {
    self.mounts.push(mount);
    self
  }

  /// The configuration this builder was made from.
  pub fn config(&self) -> &Config {
    &self.artifact.config
  }

  /// How the artifact was read, `None` for a builder made from one in memory.
  /// A sites mounter reads each site through `Loader::mount` on it, so a
  /// site's secrets decrypt the way the shell's do.
  pub fn loader(&self) -> Option<&Loader> {
    self.loader.as_ref()
  }

  /// How `Host::reload` rebuilds the tables: a builder for the application as
  /// it stands on disk, with whatever this builder was given added again.
  /// How the sites are mounted again on [`Host::reload_sites`]. The step is
  /// handed a builder the host made from the shell's boot inputs and returns it
  /// with the sites of the moment mounted; it never makes a builder of its own,
  /// so it cannot reread the shell's configuration or plan.
  pub fn sites_mounter<F>(mut self, f: F) -> Self
  where
    F: Fn(HostBuilder) -> Result<HostBuilder, HostError> + Send + Sync + 'static,
  {
    self.sites_mounter = Some(Box::new(f));
    self
  }

  pub fn reloader<F>(mut self, f: F) -> Self
  where
    F: Fn() -> Result<HostBuilder, HostError> + Send + Sync + 'static,
  {
    self.reloader = Some(Box::new(f));
    self
  }

  pub fn build(mut self) -> Result<Host, HostError> {
    let traces = self.traces.take();
    let reloader = self.reloader.take();
    let chosen_csrf = self.csrf.take();
    let chosen_codec = self.codec.take();
    let chosen_ring = self.keyring.take();
    let sites_mounter = self.sites_mounter.take();
    // A sites reload rebuilds the shell from these three alone, so a builder
    // carrying anything else they cannot reproduce keeps none of them and the
    // call is refused by name rather than quietly dropping what it held.
    let rebuildable = self.services.is_none()
      && self.transport_override.is_none()
      && self.store.is_none()
      && self.shell.is_none()
      && self.prerendered.is_none()
      && self.identity.is_none()
      && chosen_csrf.is_none()
      && chosen_codec.is_none()
      && chosen_ring.is_none()
      && !self.hand_built;
    let shell_inputs = rebuildable.then(|| Arc::new(self.artifact.clone()));
    let loader = self.loader.take();
    let mounted = !self.mounts.is_empty();
    let store = self.store.take();
    let topic_rule = self.topic_rule.take();
    #[cfg(feature = "ws")]
    let socket_handler = self.socket_handler.take();
    #[cfg(feature = "ws")]
    let sockets = self.sockets.take().unwrap_or_else(|| Arc::new(socket::Sockets::new()));
    let http2 = self.http2;
    let (tables, mut config) = self.assemble()?;
    if config.session.key.is_none() && config.dev() {
      config.session.key = Some(snapfire_fsr_session::random_token());
    }
    if config.session.key.as_deref() == Some(config::SESSION_KEY_PLACEHOLDER) {
      return Err(placeholder_session_key(&config));
    }
    let ttl = config.session_ttl()?;
    let store: Arc<dyn SessionStore> = match store {
      Some(store) => store,
      None => match config.session.store.as_str() {
        "memory" => Arc::new(MemorySessionStore::new(config.session.capacity)),
        "service" => {
          let client = format!(
            "{}{}",
            config.site.as_ref().map(SiteSection::prefix).unwrap_or_default(),
            config.session.client.clone().unwrap_or_default()
          );
          Arc::new(ServiceSessionStore::new(tables.app.services.clone(), client, ttl))
        }
        other => return Err(HostError::Value("session.store".to_owned(), other.to_owned())),
      },
    };
    let (ring, owned_ring) = match chosen_ring {
      Some(ring) => (ring, None),
      None => {
        if config.session.key.is_none() {
          return Err(missing_session_key(&config));
        }
        let ring = Arc::new(Keyring::from_keys(session_keys(&config)));
        (ring.clone(), Some(ring))
      }
    };
    let scheme: Arc<dyn CsrfScheme> = match chosen_csrf {
      Some(scheme) => scheme,
      None => match config.session.csrf_scheme.as_str() {
        "derived" => Arc::new(snapfire_fsr_session::Derived::over(ring.clone())),
        "session" => Arc::new(snapfire_fsr_session::PerSession::new()),
        _ => Arc::new(snapfire_fsr_session::SingleUse::new(config.session.csrf_outstanding as usize)),
      },
    };
    let codec: Arc<dyn CookieCodec> = match chosen_codec {
      Some(codec) => codec,
      None => Arc::new(HmacCodec::over(ring.clone())),
    };
    let sessions = Sessions::with_codec(
      store,
      config.session.key.as_deref().unwrap_or_default().as_bytes(),
      codec,
      SessionConfig {
        ttl,
        secure: config.session.secure,
        ..SessionConfig::default()
      },
    )
    .with_csrf(scheme);
    let changed = config.dev().then(|| Reload::new(16));
    let http2 = http2.unwrap_or(config.server.http2);
    #[cfg(not(feature = "tls"))]
    if config.server.tls.is_some() {
      return Err(HostError::Config(
        config.root.clone(),
        "[server.tls] needs the host's `tls` feature, which is off by default".to_owned(),
      ));
    }
    #[cfg(feature = "tls")]
    let (tls, tls_reload) = match &config.server.tls {
      Some(section) => {
        let reload = match section.reload.as_str() {
          "none" => None,
          name @ ("hup" | "usr1" | "usr2") => Some(name.to_owned()),
          other => {
            let why = format!("server.tls.reload `{other}` is not a signal; hup, usr1, usr2 or none");
            return Err(HostError::Config(config.root.clone(), why));
          }
        };
        let (cert, key) = section.files(&config.root);
        let alpn = section.alpn.clone().unwrap_or_else(|| default_alpn(http2));
        (Some(Arc::new(tls::Tls::load(cert, key, alpn)?)), reload)
      }
      None => (None, None),
    };
    Ok(Host {
      traces,
      live: parking_lot::RwLock::new(Arc::new(tables)),
      sessions,
      changed,
      topics: Topics::new(64),
      topic_rule,
      #[cfg(feature = "ws")]
      sockets,
      #[cfg(feature = "ws")]
      socket_handler,
      reloader,
      loader,
      shell_inputs,
      mounted,
      sites_mounter,
      #[cfg(feature = "sites_reload")]
      sites_reload: parking_lot::Mutex::new(()),
      csrf_always: config.session.csrf == "always",
      mount_secret: config.site.as_ref().and_then(|site| site.secret.clone()),
      session_shape: session_shape(&config),
      keyring: owned_ring,
      trace_ring: ring,
      max_body: config.server.max_body,
      max_upload: config.server.max_upload,
      hosts: config.server.hosts.iter().map(|h| h.to_lowercase()).collect(),
      origin: config.origin()?,
      http2,
      #[cfg(feature = "tls")]
      tls,
      #[cfg(feature = "tls")]
      tls_reload,
      report_listen: config.server.listen,
    })
  }

  /// Everything but the sessions: the tables a request reads, checked the
  /// way a boot checks them and the configuration they came from.
  pub(crate) fn assemble(mut self) -> Result<(Tables, Config), HostError> {
    if let Some(e) = self.pending.take() {
      return Err(e);
    }
    let Artifact { config, plan, contract: mut own_contract } = self.artifact;

    config.session_ttl()?;
    if !matches!(config.session.store.as_str(), "memory" | "service") {
      return Err(HostError::Value(
        "session.store".to_owned(),
        config.session.store.clone(),
      ));
    }
    leaks(&config, &plan)?;

    let mut service_rows = Vec::new();
    let mut bearer_rows: Vec<(String, String)> = Vec::new();
    let mut contract = own_contract.clone().unwrap_or_default();
    let mut transports: Vec<(String, Arc<dyn Transport>)> = Vec::new();
    let build_clients = self.services.is_none();
    if build_clients {
      clients_of(
        &config,
        self.transport_override.is_none(),
        &mut contract,
        &mut transports,
        &mut service_rows,
        &mut bearer_rows,
      )?;
    }
    let mut rust_transports: Vec<(String, Arc<dyn Transport>)> = Vec::new();
    for rust in std::mem::take(&mut self.rust_services) {
      contract.adopt(rust.contract, &rust.rust_type).map_err(|e| {
        HostError::Service(rust.name.clone(), format!("{e}; the contracts directory and the Rust disagree, so run fsr build"))
      })?;
      service_rows.push((rust.name.clone(), "rust".to_owned(), rust.rust_type));
      rust_transports.push((rust.name, rust.transport));
    }

    let manifest =
      Manifest::from_text(&plan).map_err(|e| HostError::Config(config.resolve(&config.server.plan), e.to_string()))?;
    let shell_module = config.document.shell.clone();
    let mut app = self.app.take().expect("the builder holds its app until build");
    let mut taken: Vec<String> = manifest.routes.iter().map(|r| r.pattern.clone()).collect();
    let mut sites = Vec::new();
    let mut site_reports = Vec::new();
    let mut statics: Vec<StaticRootResolved> = config
      .statics
      .iter()
      .map(|root| StaticRootResolved {
        route: root.route.trim_end_matches('/').to_owned(),
        dir: config.resolve(&root.dir),
      })
      .collect();
    if let Some(dir) = &config.shell_vendor {
      if !statics.iter().any(|s| s.route == "/static/js/vendor") {
        statics.push(StaticRootResolved {
          route: "/static/js/vendor".to_owned(),
          dir: dir.clone(),
        });
      }
    }
    let serve_client = !statics.iter().any(|s| s.route == client::ROUTE);
    let client_minified = config.document.client.minified(config.dev());
    let mut import_map = match &config.document.import_map {
      Some(rel) => {
        let path = config.resolve(rel);
        let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?;
        Some(bundle_urls(&client_urls(&text, serve_client && client_minified), minified_bundle(&config)))
      }
      None => None,
    };
    let preload = match config.document.module_preload {
      true => preload_set(&config, import_map.as_deref(), minified_bundle(&config)),
      false => Vec::new(),
    };
    for mount in std::mem::take(&mut self.mounts) {
      let site = mount.artifact.config.site.clone().ok_or_else(|| {
        HostError::Mount(
          mount.name.clone(),
          "the artifact's configuration has no [site] section".to_owned(),
        )
      })?;
      if site.name != mount.name {
        return Err(HostError::Mount(
          mount.name.clone(),
          format!("the artifact is the site `{}`", site.name),
        ));
      }
      if sites.iter().any(|s: &SiteTables| s.at == site.at)
        || taken
          .iter()
          .any(|r| *r == site.at || r.starts_with(&format!("{}/", site.at)))
      {
        return Err(HostError::Mount(
          mount.name.clone(),
          format!("`{}` is already served", site.at),
        ));
      }
      let mut site_manifest =
        Manifest::from_text(&mount.artifact.plan).map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      let engine_rows: Vec<String> = site_manifest
        .sources
        .iter()
        .filter(|r| r.owner == RowOwner::Engine)
        .map(|r| r.id.clone())
        .chain(
          site_manifest
            .actions
            .iter()
            .filter(|r| r.owner == RowOwner::Engine)
            .map(|r| r.id.clone()),
        )
        .chain(
          site_manifest
            .handlers
            .iter()
            .filter(|r| r.owner == RowOwner::Engine)
            .map(|r| r.id.clone()),
        )
        .collect();
      if !engine_rows.is_empty() && !mount.allow_engine {
        return Err(HostError::Mount(
          mount.name.clone(),
          format!(
            "engine-owned rows {}; set allow_engine = true to mount them",
            engine_rows.join(", ")
          ),
        ));
      }
      leaks(&mount.artifact.config, &mount.artifact.plan).map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      let middleware = site_manifest.middleware.take().map(snapfire_fsr::middleware_from);
      graft(&manifest, &mut site_manifest, &shell_module);
      site_manifest.not_found = None;
      taken.extend(site_manifest.routes.iter().map(|r| r.pattern.clone()));
      app
        .mount_manifest(&site_manifest.to_json())
        .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      if let Some(site_contract) = &mount.artifact.contract {
        contract
          .merge(site_contract.clone(), &format!("site {}", mount.name))
          .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      }
      if build_clients {
        clients_of(
          &mount.artifact.config,
          self.transport_override.is_none(),
          &mut contract,
          &mut transports,
          &mut service_rows,
          &mut bearer_rows,
        )
        .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      }
      let mut ignored = Vec::new();
      for root in &mount.artifact.config.statics {
        let route = root.route.trim_end_matches('/').to_owned();
        if route.starts_with(&site.at) && !statics.iter().any(|s| s.route == route) {
          statics.push(StaticRootResolved {
            route,
            dir: mount.artifact.config.resolve(&root.dir),
          });
        } else {
          ignored.push(format!("static {route}"));
        }
      }
      for section in ["session", "auth", "locales", "cache"] {
        let set = match section {
          "session" => true,
          "auth" => mount.artifact.config.auth.is_some(),
          "locales" => mount.artifact.config.locales.is_some(),
          _ => mount.artifact.config.cache.is_some(),
        };
        if set {
          ignored.push(section.to_owned());
        }
      }
      if let Some(rel) = &mount.artifact.config.document.import_map {
        let path = mount.artifact.config.resolve(rel);
        let theirs = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?;
        let merged = merge_import_maps(import_map.as_deref(), &theirs);
        let merged = bundle_urls(&merged, minified_under(mount.artifact.config.bundle.as_ref(), config.dev()));
        import_map = Some(client_urls(&merged, serve_client && client_minified));
      }
      site_reports.push(SiteReport {
        name: mount.name.clone(),
        at: site.at.clone(),
        artifact: mount.artifact.config.root.clone(),
        version: mount.version.clone(),
        hash: mount.hash.clone(),
        over: Vec::new(),
        ignored,
      });
      let site_bundle = minified_under(mount.artifact.config.bundle.as_ref(), config.dev());
      sites.push(SiteTables {
        name: mount.name.clone(),
        at: site.at.clone(),
        middleware,
        styles: minified_urls(mount.artifact.config.document.styles.clone().unwrap_or_default(), site_bundle),
        entry: mount.artifact.config.document.entry.as_deref().map(|entry| minified_url(entry, site_bundle)),
        preload: match config.document.module_preload {
          true => {
            let mut theirs = preload_set(&mount.artifact.config, import_map.as_deref(), site_bundle);
            theirs.retain(|url| !preload.contains(url));
            theirs
          }
          false => Vec::new(),
        },
        remote: None,
      });
    }
    for (name, row) in config.sites.iter().flat_map(|section| section.mounts.iter()).filter(|(_, row)| row.is_remote()) {
      let remote = RemoteSite::new(name, row)?;
      let at = remote.at().to_owned();
      if sites.iter().any(|s: &SiteTables| s.at == at) || taken.iter().any(|r| *r == at || r.starts_with(&format!("{at}/"))) {
        return Err(HostError::Mount(name.clone(), format!("`{at}` is already served")));
      }
      let module = remote_module(name);
      let mut plan = PlanFileNode {
        id: 0,
        module: shell_module.clone(),
        source: None,
        deferred: false,
        fallback: None,
        error: None,
        error_kinds: Vec::new(),
        cache_key: None,
        children: vec![PlanChild { slot: "content".to_owned(), node: remote_plan(&manifest, &shell_module, &module) }],
        keep: Vec::new(),
      };
      renumber(&mut plan, &mut 0);
      let routes = Manifest::new(vec![
        RouteEntry { pattern: at.clone(), plan: plan.clone() },
        RouteEntry { pattern: format!("{at}/{{*rest}}"), plan },
      ])
      .routes()
      .map_err(|e| HostError::Mount(name.clone(), e.to_string()))?;
      for (pattern, plan) in routes {
        taken.push(pattern.clone());
        app = app.route(pattern, plan);
      }
      app = app.remote(module, Arc::new(remote.clone()));
      site_reports.push(SiteReport {
        name: name.clone(),
        at: at.clone(),
        artifact: PathBuf::new(),
        version: "-".to_owned(),
        hash: "-".to_owned(),
        over: remote.addresses().to_vec(),
        ignored: Vec::new(),
      });
      sites.push(SiteTables { name: name.clone(), at, middleware: None, styles: Vec::new(), entry: None, preload: Vec::new(), remote: Some(remote) });
    }
    sites.sort_by(|a, b| b.at.len().cmp(&a.at.len()).then(a.at.cmp(&b.at)));
    let app_contract = if site_reports.is_empty() {
      own_contract.take()
    } else {
      Some(contract.clone())
    };

    let services = match self.services {
      Some(services) => services,
      None => {
        let mut builder = Services::builder()
          .contract(contract)
          .intercept(Arc::new(TraceInterceptor::new()))
          .intercept(Arc::new(IdentityInterceptor::new()));
        if let Some(cache) = &config.cache {
          if let Some(data) = &cache.data {
            builder = builder.data_cache(data.capacity.unwrap_or(cache.capacity));
          }
        }
        let mut by_key: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for (client, key) in &bearer_rows {
          by_key.entry(key.clone()).or_default().push(client.clone());
        }
        for (key, clients) in by_key {
          builder = builder.intercept(Arc::new(CredentialInterceptor::bearer(key).only(clients)));
        }
        match &self.transport_override {
          Some(transport) => builder = builder.default_transport(transport.clone()),
          None => {
            for (name, transport) in transports {
              builder = builder.transport(name, transport);
            }
          }
        }
        for (name, transport) in rust_transports {
          builder = builder.transport(name, transport);
        }
        builder
          .try_build()
          .map_err(|e| HostError::Value("cache.data".to_owned(), e.to_string()))?
      }
    };

    let shell_path = config.document.shell.split('#').next().unwrap_or("shell").to_owned();
    let shell: Arc<dyn Evaluator> = self.shell.take().unwrap_or_else(|| Arc::new(shell::DocumentShell { slot_order: config.store.slot_order.clone() }));
    app = app.slot_order(config.store.slot_order.clone());
    if let Some(contract) = app_contract {
      app = app.contract(contract);
    }
    let (live, cache_row): (Arc<dyn NodeCache>, Option<(u64, String)>) = match (config.cache_ttl()?, &config.cache) {
      (Some(ttl), Some(section)) => (Arc::new(FibreCache::bounded(section.capacity, ttl)), Some((section.capacity, section.ttl.clone()))),
      _ => (Arc::new(NoCache), None),
    };
    let locales = match &config.locales {
      Some(section) => Locales::from_section(section).map_err(|e| {
        HostError::Config(
          config.sources.first().cloned().unwrap_or_else(|| config.root.clone()),
          e,
        )
      })?,
      None => Locales::single(),
    };
    let catalogs = Arc::new(
      locale::load_catalogs(&config.app, &locales.default).map_err(|e| HostError::Config(config.root.clone(), e))?,
    );
    if !catalogs.is_empty() {
      app = app.catalogs(catalogs.clone());
    }
    let catalog_rows = catalogs.rows();
    app = app.bearer_services(bearer_rows.iter().map(|(client, _)| client.clone()));
    let prerendered = self
      .prerendered
      .take()
      .or_else(|| config.server.prerender.as_deref().map(|rel| config.resolve(rel)));
    let warm = Arc::new(WarmLoads::new(
      prerendered
        .as_ref()
        .map(|dir| warm_from_file(&dir.join(LOADS_FILE)))
        .unwrap_or_default(),
    ));
    let warmed_count = warm.len();
    app = app.loads(warm.clone());
    let renders = Arc::new(WarmRenders::new(
      prerendered
        .as_ref()
        .map(|dir| renders_from_file(&dir.join(RENDERS_FILE)))
        .unwrap_or_default(),
      live,
    ));
    let rendered_count = renders.len();
    app = app.cache(renders.clone());
    let extension_rows: Vec<String> = app
      .extensions()
      .names()
      .into_iter()
      .filter(|name| {
        !snapfire_fsr_ir::STANDARD
          .iter()
          .any(|(m, n, _)| format!("{m}.{n}") == *name)
      })
      .collect();
    let app = app.services(services);
    #[cfg(not(feature = "tera"))]
    let stock_templates: Option<Vec<String>> = None;
    let assets_manifest = crate::assets::AssetsManifest::read(&config.app);
    #[cfg(feature = "tera")]
    let (app, stock_templates) = match tera::evaluator(&config.app, assets_manifest.as_ref())? {
      Some((evaluator, names)) if !app.covers(&ModuleId::new("probe.tera", "default")) => (app.evaluator(is_template_module, Arc::new(evaluator)), Some(names)),
      _ => (app, None),
    };
    let app = app.evaluator(move |m: &ModuleId| m.path == shell_path, shell).build()?;
    let unanswered_routes: Vec<String> = config.routes.iter().filter(|declared| !app.report.routes.iter().any(|(pattern, _)| pattern == &declared.pattern)).map(|declared| declared.pattern.clone()).collect();
    for (index, _) in app.patterns.iter().enumerate() {
      let Some(plan) = app.resolver.resolve(snapfire_fsr_runtime::EntryId(index as u32), &Params::new()) else { continue };
      templates_answered(&plan, &app.runtime.evaluators, stock_templates.as_deref())?;
    }
    for plan in app.intercepts.all().chain(app.not_found.iter()) {
      templates_answered(plan, &app.runtime.evaluators, stock_templates.as_deref())?;
    }

    let styles = minified_urls(config.document.styles.clone().unwrap_or_default(), minified_bundle(&config));
    let entry = config.document.entry.as_deref().map(|entry| minified_url(entry, minified_bundle(&config)));
    let mut head = shell::head(
      config.document.title.literal(),
      &styles,
      import_map.as_deref(),
      &preload,
      entry.as_deref(),
    );
    head.template = config.document.title_template.as_ref().map(|template| template.literal().to_owned());
    head.head = config.document.head_meta()?.head;
    let font_css = assets_manifest.as_ref().map(|m| m.fonts.css.clone()).filter(|css| !css.is_empty());
    if let Some(manifest) = &assets_manifest {
      head.head.extend(asset_head_rows(manifest));
    }
    head.origin = config.origin()?;
    let dev = config.dev();
    let dev_bundle = dev.then(|| config.app.join("dist/.snapfire-build.json"));

    // The development refresh script is inline and carries the bundle id it was
    // rendered against, so its text changes per request and no source computed
    // at boot covers it.
    // The host owns the sources only it can know: the inline import map's hash,
    // and in development the nonce on its own refresh script, whose text carries
    // the bundle id it was rendered against and so has no stable hash.
    let dev_nonce = dev.then(dev_nonce);
    let empty_icon = config.document.head.iter().any(|t| t.get("href").is_some_and(|href| href == "data:,"));
    let compose = |declared: &config::Csp| {
      let mut policy = declared.clone();
      if empty_icon {
        policy.widen("img-src", "data:");
      }
      if let Some(map) = import_map.as_deref() {
        policy.widen("script-src", import_map_csp(map));
      }
      if let Some(nonce) = &dev_nonce {
        policy.widen("script-src", format!("'nonce-{nonce}'"));
      }
      if let Some(css) = &font_css {
        policy.widen("style-src", import_map_csp(css));
      }
      // A rendered `style` attribute has no hash a boot-time policy can name,
      // and a fragment swapped in by client navigation is checked against the
      // first document's policy, so attributes are admitted on their own
      // directive; `<style>` and `<script>` stay as declared.
      if !policy.declares("style-src-attr") && (policy.declares("style-src") || policy.declares("default-src")) {
        policy.add("style-src-attr", "'unsafe-inline'");
      }
      if let Some(manifest) = &assets_manifest {
        for (directive, source) in asset_csp_sources(manifest) {
          policy.widen(directive, source);
        }
      }
      policy.header().and_then(|text| HeaderValue::from_str(&text).ok())
    };
    let csp = config.document.csp.as_ref().and_then(&compose);
    let csp_report_only = config.document.csp_report_only.as_ref().and_then(&compose);
    let static_rows: Vec<(String, PathBuf)> = statics.iter().map(|s| (s.route.clone(), s.dir.clone())).collect();
    // Longest route first, so the most specific root answers a path whatever
    // order the file, the inference and the mounts named them in.
    statics.sort_by(|a, b| b.route.len().cmp(&a.route.len()));
    let public: ValueMap = {
      let mut fields = snapfire_fsr_core::Fields::default();
      for (key, value) in &config.public {
        fields.insert(key.clone(), value.to_value());
      }
      fields.into()
    };
    let statics: Vec<(String, ServeDir)> = statics.into_iter().map(|s| (s.route, ServeDir::new(s.dir))).collect();
    let static_cache = match config.server.static_max_age {
      None => Some(HeaderValue::from_static("no-cache")),
      Some(0) => None,
      Some(seconds) => HeaderValue::from_str(&format!("public, max-age={seconds}")).ok(),
    };

    let locale_rows = match &config.locales {
      Some(_) => {
        let mut rows = vec![locales.default.clone()];
        rows.extend(locales.supported.iter().filter(|t| **t != locales.default).cloned());
        rows
      }
      None => Vec::new(),
    };
    let prefix = config.site.as_ref().map(SiteSection::prefix).unwrap_or_default();
    let auth = match (self.identity.take(), &config.auth) {
      (Some(provider), section) => {
        let login_path = section
          .as_ref()
          .map(|s| s.login.clone())
          .unwrap_or_else(|| "/login".to_owned());
        Some((
          Mounted {
            auth: Auth::new(provider),
            login_path,
          },
          "custom".to_owned(),
        ))
      }
      (None, Some(section)) => {
        let provider: Arc<dyn IdentityProvider> = match section.provider.as_str() {
          "file" => {
            let users = config
              .config_dir()
              .join(section.users.as_deref().unwrap_or("auth.toml"));
            Arc::new(DevProvider::from_toml(&section.login, &users).map_err(|e| HostError::Config(users.clone(), e))?)
          }
          "service" => {
            let client = format!("{prefix}{}", section.client.clone().unwrap_or_default());
            Arc::new(ServiceProvider::new(
              app.services.clone(),
              client,
              section.login.clone(),
            ))
          }
          other => return Err(HostError::Value("auth.provider".to_owned(), other.to_owned())),
        };
        let name = match &section.client {
          Some(client) if section.provider == "service" => format!("service via {client}"),
          _ => section.provider.clone(),
        };
        Some((
          Mounted {
            auth: Auth::new(provider),
            login_path: section.login.clone(),
          },
          name,
        ))
      }
      (None, None) => None,
    };
    let auth_row = auth
      .as_ref()
      .map(|(mounted, name)| (name.clone(), mounted.login_path.clone()));
    let auth = auth.map(|(mounted, _)| mounted);
    let report = HostReport {
      app: app.report.clone(),
      unanswered_routes,
      http2: self.http2.unwrap_or(config.server.http2),
      tls: config.server.tls.as_ref().map(|section| {
        let (cert, key) = section.files(&config.root);
        TlsReport {
          cert,
          key,
          alpn: section
            .alpn
            .clone()
            .unwrap_or_else(|| default_alpn(self.http2.unwrap_or(config.server.http2))),
          reload: (section.reload != "none").then(|| section.reload.clone()),
        }
      }),
      services: service_rows,
      session: (config.session.store == "service").then(|| config.session.client.clone().unwrap_or_default()),
      cached: app
        .services
        .data_cache()
        .map(|cache| {
          cache
            .policies()
            .into_iter()
            .map(|(method, f)| {
              let mut policy = format!("ttl {} {}", f.ttl, f.scope.as_str());
              if let Some(stale) = &f.stale {
                policy.push_str(&format!(", stale {stale}"));
              }
              if !f.tags.is_empty() {
                policy.push_str(&format!(" [{}]", f.tags.join(", ")));
              }
              (method, policy)
            })
            .collect()
        })
        .unwrap_or_default(),
      writers: app
        .services
        .data_cache()
        .map(|cache| {
          cache
            .writers()
            .into_iter()
            .map(|(method, tags)| (method, format!("[{}]", tags.join(", "))))
            .collect()
        })
        .unwrap_or_default(),
      shared: app.services.data_cache().map(shared_tags).unwrap_or_default(),
      statics: static_rows,
      client: serve_client.then(|| (client::ROUTE, client::FILES.len(), client::bytes(client_minified), client_minified)),
      prerender: prerendered.clone(),
      warmed: warmed_count,
      rendered: rendered_count,
      cache: cache_row,
      dev,
      locales: locale_rows,
      hosts: config.server.hosts.iter().map(|h| h.to_lowercase()).collect(),
      origin: config.origin()?,
      catalogs: catalog_rows,
      auth: auth_row,
      bearer: bearer_rows,
      extensions: extension_rows,
      site: config.site.as_ref().map(|s| (s.name.clone(), s.at.clone())),
      sites: site_reports,
      import_map_csp: import_map.as_deref().map(import_map_csp),
      csp: csp.as_ref().and_then(|v| v.to_str().ok()).map(str::to_owned),
      csp_report_only: csp_report_only.as_ref().and_then(|v| v.to_str().ok()).map(str::to_owned),
      config: config.sources.clone(),
      inferred: config.inferred.clone(),
      public: config.public.iter().map(|(k, v)| (k.clone(), v.to_string())).collect(),
      ignored: config.ignored.clone(),
    };
    Ok((
      Tables {
        app,
        head,
        title: config.document.title.clone(),
        title_template: config.document.title_template.clone(),
        public,
        origin: config.origin()?,
        dev_bundle,
        statics,
        favicon: config.favicon.clone(),
        static_cache,
        client: serve_client,
        client_minified,
        csp,
        csp_report_only,
        dev_nonce,
        trace_expose: config.trace.as_ref().map(|t| t.expose.clone()).unwrap_or_default(),
        prerendered,
        warm,
        renders,
        locales,
        catalogs,
        auth,
        sites,
        own: config.site.as_ref().map(|site| OwnSite { name: site.name.clone(), at: site.at.clone(), entry: entry.clone(), styles: styles.clone() }),
        report: Arc::new(report),
      },
      config,
    ))
  }
}

/// Every `@` tag in the cache's policies and writers, each with the methods
/// cached under it and the methods that drop it, in tag order.
fn shared_tags(cache: &snapfire_fsr_service::DataCache) -> Vec<(String, Vec<String>, Vec<String>)> {
  let mut by_tag: std::collections::BTreeMap<String, (Vec<String>, Vec<String>)> = Default::default();
  for (method, freshness) in cache.policies() {
    for tag in freshness.tags.iter().filter(|tag| tag.starts_with('@')) {
      by_tag.entry(tag.clone()).or_default().0.push(method.clone());
    }
  }
  for (method, tags) in cache.writers() {
    for tag in tags.iter().filter(|tag| tag.starts_with('@')) {
      by_tag.entry(tag.clone()).or_default().1.push(method.clone());
    }
  }
  by_tag.into_iter().map(|(tag, (cached, dropped))| (tag, cached, dropped)).collect()
}

/// The module id the shell's plan gives a site in its own process, which no
/// artifact can carry: `<name>:@remote#site`.
pub(crate) fn remote_module(name: &str) -> String {
  format!("{name}:@remote#site")
}

/// The site's node under the shell's root layout, as `graft` nests a site in
/// this process; under the document directly without one.
fn remote_plan(shell: &Manifest, shell_module: &str, module: &str) -> PlanFileNode {
  let node = PlanFileNode {
    id: 0,
    module: module.to_owned(),
    source: None,
    deferred: false,
    fallback: None,
    error: None,
    error_kinds: Vec::new(),
    cache_key: None,
    children: Vec::new(),
    keep: Vec::new(),
  };
  match shell_root_layout(shell, shell_module) {
    Some(mut layout) => {
      for child in &mut layout.children {
        if child.slot == "content" {
          child.node = node.clone();
        }
      }
      layout
    }
    None => node,
  }
}

/// The `[session]` settings as one string, compared across a reload.
pub(crate) fn session_shape(config: &Config) -> String {
  let s = &config.session;
  format!(
    "{} {:?} {} {} {} {:?}",
    s.store, s.client, s.ttl, s.secure, s.csrf, s.capacity
  )
}

/// What a trace token signs. The prefix keeps a token from verifying as any
/// other value the same ring signs, a session id among them.
pub(crate) fn trace_token_input(id: u64) -> String {
  format!("fsr.trace:{id}")
}

/// Whether `path` is `prefix` or sits beneath it; `/` covers every path.
pub(crate) fn under_prefix(path: &str, prefix: &str) -> bool {
  let prefix = prefix.trim_end_matches('/');
  prefix.is_empty() || path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

pub(crate) fn placeholder_session_key(config: &Config) -> HostError {
  HostError::Config(
    config.config_dir(),
    "`session.key` is still the placeholder; set C5_SESSION__KEY or replace it with a generated key".to_owned(),
  )
}

pub(crate) fn missing_session_key(config: &Config) -> HostError {
  HostError::Config(
    config.config_dir(),
    "`session.key` is not set outside development; set C5_SESSION__KEY or write it in a configuration file the host reads".to_owned(),
  )
}

/// `session.key` first, then `session.previous_keys` in order.
pub(crate) fn session_keys(config: &Config) -> Vec<Vec<u8>> {
  config
    .session
    .key
    .iter()
    .chain(config.session.previous_keys.iter())
    .map(|k| k.as_bytes().to_vec())
    .collect()
}
