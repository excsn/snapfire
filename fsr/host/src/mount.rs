use super::*;

/// A built application as the host reads it: its configuration, its plan and
/// the contracts its build wrote. `Host::from` reads one through a `Loader`
/// and a `Mount` carries one.
#[derive(Debug, Clone)]
pub struct Artifact {
  pub config: Config,
  pub plan: String,
  pub contract: Option<Contract>,
}

impl Artifact {
  /// Reads the plan file and the contracts directory `config` names, the
  /// latter merged with `Contract::merge` file by file when it exists.
  pub fn of(config: Config) -> Result<Self, HostError> {
    let plan_path = config.resolve(&config.server.plan);
    let plan = std::fs::read_to_string(&plan_path).map_err(|e| HostError::Io(plan_path, e))?;
    let contract = read_contracts(&config.resolve(&config.server.contracts))?;
    Ok(Artifact { config, plan, contract })
  }
}

/// A site's artifact as the host mounts it: the site's own configuration,
/// read through the shell's ladder, its plan and its contracts, all already
/// namespaced by its build. Where it came from is the caller's business;
/// `artifact`, `version` and `hash` are carried into the report.
pub struct Mount {
  pub name: String,
  pub version: String,
  pub hash: String,
  pub allow_engine: bool,
  /// The site as read from its directory, `artifact.config.root`, by
  /// `Loader::mount` on the shell's loader or a loader of the caller's own.
  pub artifact: Artifact,
}

impl Mount {
  pub fn new(
    name: impl Into<String>,
    version: impl Into<String>,
    hash: impl Into<String>,
    allow_engine: bool,
    artifact: Artifact,
  ) -> Self {
    Self {
      name: name.into(),
      version: version.into(),
      hash: hash.into(),
      allow_engine,
      artifact,
    }
  }
}

/// The identity flow the host serves under `/auth/`, plus the application's
/// login page, where `begin` sends the browser and where a GET seeds the
/// flow so a typed URL can still post to the callback.
pub(crate) struct Mounted {
  pub(crate) auth: Auth,
  pub(crate) login_path: String,
}

/// Every `*.json` in `dir` in name order, merged into one contract; `None`
/// when there is no such directory. A type or service defined twice names the
/// file that repeats it.
pub(crate) fn read_contracts(dir: &std::path::Path) -> Result<Option<Contract>, HostError> {
  if !dir.is_dir() {
    return Ok(None);
  }
  let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| HostError::Io(dir.to_path_buf(), e))?
    .flatten()
    .map(|e| e.path())
    .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "json"))
    .collect();
  files.sort();
  let mut contract = Contract::new();
  for path in files {
    let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
    let part = Contract::from_json(&text).map_err(|e| HostError::Contract(path.clone(), e.to_string()))?;
    contract
      .merge(part, &path.file_name().unwrap_or_default().to_string_lossy())
      .map_err(|e| HostError::Contract(path.clone(), e.to_string()))?;
  }
  Ok(Some(contract))
}

/// A `mock` client's transport from its responses file: an object of method
/// name to the response in the payload's JSON encoding or to
/// `{"$fail": {"kind": "<failure kind>", "message": "..."}}` for a failure.
pub(crate) fn mock_transport(
  config: &Config,
  name: &str,
  client: &ClientConfig,
  prefix: &str,
) -> Result<Option<(Arc<dyn Transport>, String)>, HostError> {
  if !client.is_mock() {
    return Ok(None);
  }
  let file = client.responses_file(name);
  let path = config.resolve(&file);
  let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
  let json: serde_json::Value =
    serde_json::from_str(&text).map_err(|e| HostError::Config(path.clone(), e.to_string()))?;
  let Some(entries) = json.as_object() else {
    return Err(HostError::Config(
      path,
      "expected an object of method name to response".to_owned(),
    ));
  };
  let mut mock = MockTransport::new();
  for (method, response) in entries {
    let key = format!("{prefix}{name}.{method}");
    match response.get("$fail") {
      Some(fail) => {
        let kind = match fail.get("kind").and_then(|k| k.as_str()).unwrap_or("internal") {
          "unauthorized" => FailureKind::Unauthorized,
          "not_found" => FailureKind::NotFound,
          "invalid" => FailureKind::Invalid,
          "conflict" => FailureKind::Conflict,
          "timeout" => FailureKind::Timeout,
          "unavailable" => FailureKind::Unavailable,
          "internal" => FailureKind::Internal,
          other => {
            return Err(HostError::Config(
              path,
              format!("{method}: unknown failure kind `{other}`"),
            ));
          }
        };
        let message = fail
          .get("message")
          .and_then(|m| m.as_str())
          .unwrap_or("mocked failure")
          .to_owned();
        mock = mock.fails(key, kind, message);
      }
      None => {
        let value = snapfire_fsr_payload::json_to_value(response)
          .map_err(|e| HostError::Config(path.clone(), format!("{method}: {e}")))?;
        mock = mock.returns(key, value);
      }
    }
  }
  Ok(Some((Arc::new(mock), file)))
}

/// A client's contract as the host merges it: prefixed with the site's name
/// when the application is a site, since the build prefixed the bodies.
pub(crate) fn site_contract(contract: &Contract, config: &Config) -> Contract {
  match &config.site {
    Some(site) => contract.namespaced(&site.name),
    None => contract.clone(),
  }
}

pub(crate) struct StaticRootResolved {
  pub(crate) route: String,
  pub(crate) dir: PathBuf,
}

/// The clients of one configuration: their contracts merged in, one
/// transport each unless the caller overrides transports and the report
/// rows. Names carry the configuration's site prefix, the build's spelling.
pub(crate) fn clients_of(
  config: &Config,
  build_transports: bool,
  contract: &mut Contract,
  transports: &mut Vec<(String, Arc<dyn Transport>)>,
  service_rows: &mut Vec<(String, String, String)>,
  bearer_rows: &mut Vec<(String, String)>,
) -> Result<(), HostError> {
  let prefix = config.site.as_ref().map(SiteSection::prefix).unwrap_or_default();
  for (name, client) in &config.clients {
    let named = format!("{prefix}{name}");
    if let Some(key) = client.bearer.as_ref().and_then(BearerKey::key) {
      bearer_rows.push((named.clone(), key.to_owned()));
    }
    let document = client
      .document
      .clone()
      .unwrap_or_else(|| format!("clients/{name}.openapi.json"));
    let path = config.resolve(&document);
    if document.ends_with(".proto") {
      let imported =
        snapfire_fsr_service::import_proto(&path, name).map_err(|error| HostError::Import { document, error })?;
      let site_contract = site_contract(&imported.contract, config);
      contract.types.extend(site_contract.types.clone());
      contract.services.extend(site_contract.services.clone());
      if let Some((transport, file)) = mock_transport(config, name, client, &prefix)? {
        transports.push((named.clone(), transport));
        service_rows.push((named.clone(), "mock".to_owned(), file));
        continue;
      }
      let base_url = client.base_url.clone().unwrap_or_default();
      if build_transports {
        let transport = snapfire_fsr_service::GrpcTransport::new(&base_url, &imported)
          .map_err(|e| HostError::Transport(name.clone(), e))?;
        transports.push((named.clone(), Arc::new(transport)));
      }
      service_rows.push((named.clone(), "grpc".to_owned(), base_url));
      continue;
    }
    let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
    let imported = snapfire_fsr_service::import(&text, name).map_err(|error| HostError::Import { document, error })?;
    let site_contract = site_contract(&imported.contract, config);
    contract.types.extend(site_contract.types.clone());
    contract.services.extend(site_contract.services.clone());
    if let Some((transport, file)) = mock_transport(config, name, client, &prefix)? {
      transports.push((named.clone(), transport));
      service_rows.push((named.clone(), "mock".to_owned(), file));
      continue;
    }
    let base_url = client.base_url.clone().unwrap_or_default();
    let mut transport = HttpTransport::new(&base_url);
    for (path, route) in &imported.routes {
      transport = transport.route(path.clone(), route.clone());
    }
    transports.push((named.clone(), Arc::new(transport)));
    service_rows.push((named.clone(), "http".to_owned(), base_url));
  }
  Ok(())
}

/// The shell's import map with a site's entries added where the shell has
/// none: the shell pins the runtime, a site brings only what it adds.
pub(crate) fn merge_import_maps(shell: Option<&str>, site: &str) -> String {
  let mut merged: serde_json::Value = shell
    .and_then(|s| serde_json::from_str(s).ok())
    .unwrap_or_else(|| serde_json::json!({ "imports": {} }));
  let theirs: serde_json::Value = serde_json::from_str(site).unwrap_or_else(|_| serde_json::json!({ "imports": {} }));
  if let (Some(ours), Some(theirs)) = (
    merged.get_mut("imports").and_then(|i| i.as_object_mut()),
    theirs.get("imports").and_then(|i| i.as_object()),
  ) {
    for (key, value) in theirs {
      ours.entry(key.clone()).or_insert_with(|| value.clone());
    }
  }
  merged.to_string()
}

/// The shell's root layout: the node under the document's content slot when
/// it is `routes/layout.tsx#default`.
pub(crate) fn shell_root_layout(shell: &Manifest, shell_module: &str) -> Option<PlanFileNode> {
  shell.routes.iter().chain(shell.intercepts.iter()).find_map(|entry| {
    if entry.plan.module != shell_module {
      return None;
    }
    let content = entry.plan.children.iter().find(|c| c.slot == "content")?;
    (content.node.module == "routes/layout.tsx#default").then(|| content.node.clone())
  })
}

/// Nests every route and intercept of a site under the shell's root layout,
/// so one tree carries the shell's header above the site's pages. Without a
/// root layout the site's subtree sits under the document directly.
pub(crate) fn graft(shell: &Manifest, site: &mut Manifest, shell_module: &str) {
  let layout = shell_root_layout(shell, shell_module);
  let regraft = |entry: &mut RouteEntry, keep_rest: bool| {
    let Some(content) = entry
      .plan
      .children
      .iter()
      .find(|c| c.slot == "content")
      .map(|c| c.node.clone())
    else {
      return;
    };
    let inner = match &layout {
      Some(layout) => {
        let mut grafted = layout.clone();
        for child in &mut grafted.children {
          if child.slot == "content" {
            child.node = content.clone();
          }
        }
        if keep_rest {
          grafted.keep = grafted
            .children
            .iter()
            .filter(|c| c.slot != "content")
            .map(|c| c.slot.clone())
            .collect();
        }
        grafted
      }
      None => content,
    };
    let mut plan = PlanFileNode {
      id: 0,
      module: shell_module.to_owned(),
      source: None,
      deferred: false,
      fallback: None,
      error: None,
      error_kinds: Vec::new(),
      cache_key: None,
      children: vec![PlanChild {
        slot: "content".to_owned(),
        node: inner,
      }],
      keep: Vec::new(),
    };
    renumber(&mut plan, &mut 0);
    entry.plan = plan;
  };
  for entry in &mut site.routes {
    regraft(entry, false);
  }
  for entry in &mut site.intercepts {
    regraft(entry, true);
  }
}

/// Refuses a bundle that carries a server module. The plan's sources,
/// actions and handlers name their modules, `middleware.ts` is implicit,
/// and `app/<path>.ts` bundles to `dist/<path>.js`; any such output in the
/// build facts or any output importing one, is a leak. No facts file means
/// no bundle to check.
pub(crate) fn leaks(config: &Config, plan: &str) -> Result<(), HostError> {
  let facts = config.app.join("dist/.snapfire-build.json");
  let Ok(text) = std::fs::read_to_string(&facts) else {
    return Ok(());
  };
  let json: serde_json::Value =
    serde_json::from_str(&text).map_err(|e| HostError::Config(facts.clone(), e.to_string()))?;
  let manifest = Manifest::from_text(plan)
    .map_err(|e| HostError::Config(config.resolve(&config.server.plan), e.to_string()))?;
  let plan = serde_json::to_value(&manifest).expect("a manifest serializes");
  let found = leaked_outputs(&plan, &json);
  if found.is_empty() {
    Ok(())
  } else {
    Err(HostError::Leak(found.join(", ")))
  }
}

pub(crate) fn server_output(module: &str) -> String {
  let stem = module
    .strip_suffix(".tsx")
    .or_else(|| module.strip_suffix(".ts"))
    .unwrap_or(module);
  format!("{stem}.js")
}

/// Every bundle output that is a server module or imports one, each with
/// the reason.
pub(crate) fn leaked_outputs(plan: &serde_json::Value, facts: &serde_json::Value) -> Vec<String> {
  let mut server: std::collections::BTreeMap<String, String> = Default::default();
  for (table, what) in [
    ("sources", "a loader"),
    ("actions", "an actions module"),
    ("handlers", "a route handler"),
  ] {
    for row in plan[table].as_array().into_iter().flatten() {
      if let Some(module) = row["module"].as_str() {
        server.insert(server_output(module), format!("{what}, {module}"));
      }
    }
  }
  if !plan["middleware"].is_null() {
    server.insert("middleware.js".to_owned(), "the middleware, middleware.ts".to_owned());
  }
  let mut found = Vec::new();
  for output in facts["outputs"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|o| o.as_str())
  {
    if let Some(reason) = server.get(output) {
      found.push(format!("{output} is {reason}"));
    }
  }
  if let Some(graph) = facts["graph"].as_object() {
    for (importer, imports) in graph {
      for imported in imports.as_array().into_iter().flatten().filter_map(|i| i.as_str()) {
        if server.contains_key(imported) {
          found.push(format!("{importer} imports {imported}"));
        }
      }
    }
  }
  found
}

impl Host {
  /// Rebuilds the tables and swaps them in; a request in flight finishes on
  /// the tables it started with. The sessions stay: a reload that changes
  /// `[session]` is refused.
  ///
  /// With a reloader, the builder it returns is the rebuild. Without one the
  /// artifact is read again through the loader `Host::from` kept and the
  /// sites mounter runs over it, which is enough for a host built from a
  /// configuration alone; a host given services, a store, a scheme, an
  /// evaluator, an identity provider, a prerendered directory or a mount by
  /// hand needs a reloader that adds those again.
  pub fn reload(&self) -> Result<Arc<HostReport>, HostError> {
    if let Some(reloader) = &self.reloader {
      return self.reload_with(reloader()?);
    }
    let refuse = |why: &str| HostError::Value("reload".to_owned(), why.to_owned());
    let loader = self.loader.as_ref().ok_or_else(|| {
      refuse("the host was built from an artifact in memory, so there is nothing to reread; `HostBuilder::reloader` names how to rebuild")
    })?;
    if self.shell_inputs.is_none() {
      return Err(refuse(
        "the host was built with more than a configuration, so rereading the artifact cannot rebuild it; `HostBuilder::reloader` names how",
      ));
    }
    if self.mounted && self.sites_mounter.is_none() {
      return Err(refuse(
        "the host was given a mount by hand, which rereading the artifact would drop; `HostBuilder::reloader` names how to rebuild",
      ));
    }
    let mut builder = Host::from(loader.clone())?;
    if let Some(mounter) = &self.sites_mounter {
      builder = mounter(builder)?;
    }
    self.reload_with(builder)
  }

  /// Rebuilds the tables with the sites read again and the shell left exactly
  /// as this process booted it, then swaps them in.
  ///
  /// The shell's configuration, plan and contracts are the values held since
  /// boot, so a half-written `plan.json` or an edited configuration on disk
  /// cannot reach the tables through this call: only the artifacts the
  /// `[sites]` table names are read. Everything else is [`Host::reload`]'s: the
  /// tables are rebuilt whole and checked the way a boot checks them, a refused
  /// rebuild leaves the running tables serving and the swap is atomic.
  ///
  /// Needs [`HostBuilder::sites_mounter`]. A host whose builder installed
  /// services, a session store, a CSRF scheme, an evaluator, an identity
  /// provider or a prerendered directory or registered anything on the app by
  /// hand has none, since those cannot be rebuilt from the shell's artifact;
  /// such a host reloads through a reloader instead.
  pub fn reload_sites(&self) -> Result<Arc<HostReport>, HostError> {
    let refuse = |why: &str| HostError::Value("sites.reload".to_owned(), why.to_owned());
    let mounter = self
      .sites_mounter
      .as_ref()
      .ok_or_else(|| refuse("no sites_mounter; `HostBuilder::sites_mounter` names how to mount them"))?;
    let inputs = self.shell_inputs.as_ref().ok_or_else(|| {
      refuse(
        "the host was built with more than a configuration, so its shell cannot be rebuilt from memory; use reload",
      )
    })?;
    let builder = Host::from_artifact((**inputs).clone())?;
    self.reload_with(mounter(builder)?)
  }

  /// `POST /__fsr/sites/reload`: [`Host::reload_sites`] as an answer. One
  /// reload runs at a time, so a caller that repeats the request waits for the
  /// one in flight rather than starting a second read of every artifact.
  ///
  /// The route is an administrative one and the host does not guard it. Keeping
  /// `/__fsr/` off the public internet is the deployment's job, at whatever
  /// sits in front of the host.
  #[cfg(feature = "sites_reload")]
  pub(crate) fn sites_reload_response(&self) -> Response<Body> {
    let _turn = self.sites_reload.lock();
    match self.reload_sites() {
      Ok(report) => {
        let sites: Vec<serde_json::Value> = report
          .sites
          .iter()
          .map(|s| serde_json::json!({ "name": s.name, "at": s.at, "version": s.version, "hash": s.hash }))
          .collect();
        json_response(StatusCode::OK, &serde_json::json!({ "reloaded": true, "sites": sites }))
      }
      // The candidate was refused and the running tables still serve it, so the
      // fault is in what the caller published rather than in the host.
      Err(e) => json_response(
        StatusCode::CONFLICT,
        &serde_json::json!({ "reloaded": false, "error": e.to_string() }),
      ),
    }
  }

  /// `reload` over a builder the caller made.
  pub fn reload_with(&self, builder: HostBuilder) -> Result<Arc<HostReport>, HostError> {
    let (tables, config) = builder.assemble()?;
    let shape = session_shape(&config);
    if shape != self.session_shape {
      return Err(HostError::Value(
        "session".to_owned(),
        "changed since the host was built; restart to apply it".to_owned(),
      ));
    }
    if let Some(ring) = &self.keyring {
      match &config.session.key {
        Some(key) if key == config::SESSION_KEY_PLACEHOLDER => {
          return Err(placeholder_session_key(&config));
        }
        Some(_) => ring.replace(session_keys(&config)),
        None if config.dev() => {}
        None => return Err(missing_session_key(&config)),
      }
    }
    let report = tables.report.clone();
    *self.live.write() = Arc::new(tables);
    self.changed();
    Ok(report)
  }
}
