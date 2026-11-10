use super::*;

impl Loader {
  /// A loader for `path`, one of the four things `locate` accepts, under the deployment the environment names.
  pub fn at(path: impl AsRef<Path>) -> Self {
    Loader {
      path: path.as_ref().to_path_buf(),
      deployment: Deployment::from_env(),
      extras: Vec::new(),
      secrets: None,
      keep_undecrypted: false,
    }
  }

  /// A secret this process cannot decrypt is listed in `Config::undecrypted`
  /// rather than refused, for a check run on a machine that does not hold
  /// the deployment's keys. The host never loads this way.
  pub fn keep_undecrypted(mut self, keep: bool) -> Self {
    self.keep_undecrypted = keep;
    self
  }

  /// The deployment whose ladder is read, over the environment's.
  pub fn deployment(mut self, deployment: Deployment) -> Self {
    self.deployment = deployment;
    self
  }

  /// One more file, loaded after everything the ladder names; a relative path resolves against the configuration directory.
  pub fn extra(mut self, path: impl AsRef<Path>) -> Self {
    self.extras.push(path.as_ref().to_path_buf());
    self
  }

  /// Adjusts how `.c5encval` values decrypt, after the defaults are set:
  /// `base64` and `ecies_x25519` registered, `private_keys` beside the
  /// configuration files as the key directory when it exists and
  /// `C5_SECRETKEY_*` variables read as keys.
  pub fn secrets<F>(mut self, f: F) -> Self
  where
    F: Fn(&mut SecretOptions) + Send + Sync + 'static,
  {
    self.secrets = Some(Arc::new(f));
    self
  }

  /// A loader for an artifact mounted from `path`, reading it under this loader's deployment and secrets and none of its extra files.
  pub fn mount(&self, path: impl AsRef<Path>) -> Self {
    Loader {
      path: path.as_ref().to_path_buf(),
      deployment: self.deployment.clone(),
      extras: Vec::new(),
      secrets: self.secrets.clone(),
      keep_undecrypted: self.keep_undecrypted,
    }
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  /// The files this loader reads, in order.
  pub fn locate(&self) -> Result<Located, HostError> {
    let mut located = locate_with(&self.path, &self.deployment)?;
    for extra in &self.extras {
      located = located.extra(extra);
    }
    Ok(located)
  }

  /// The configuration alone: the located files in order, then `C5_` environment variables over them, secrets decrypted.
  pub fn config(&self) -> Result<Config, HostError> {
    let located = self.locate()?;
    if located.sources.is_empty() {
      return Err(HostError::NoConfig(located.root.clone()));
    }
    let mut options = C5StoreOptions::default();
    options.env_case = c5store::Case::Snake;
    options.secret_opts = self.secret_options(&located.dir);
    let (store, _mgr) = create_c5store(located.sources.clone(), Some(options))
      .map_err(|e| HostError::Config(located.sources[0].clone(), e.to_string()))?;
    let mut config = Config::from_store_keeping(&store, located, self.keep_undecrypted)?;
    config.deployment = Some(self.deployment.clone());
    if !config.dev() {
      if let Some((file, name)) = config.plain_secrets().into_iter().next() {
        let key = format!("secrets.{name}.value");
        return Err(HostError::Config(file, format!("`{key}` is written as plain text; outside development a secret's value comes from {} or is written encrypted", env_name(&key))));
      }
    }
    Ok(config)
  }

  /// The configuration plus the plan file and the contracts directory it names.
  pub fn load(&self) -> Result<crate::Artifact, HostError> {
    crate::Artifact::of(self.config()?)
  }

  pub(super) fn secret_options(&self, config_dir: &Path) -> SecretOptions {
    let mut options = SecretOptions::default();
    options.secret_key_store_configure_fn = Some(Box::new(|store: &mut SecretKeyStore| {
      store.set_decryptor("base64", Box::new(Base64SecretDecryptor {}));
      store.set_decryptor("ecies_x25519", Box::new(EciesX25519SecretDecryptor::new(EciesX25519::new())));
    }));
    let keys = config_dir.join(PRIVATE_KEYS);
    options.secret_keys_path = keys.is_dir().then_some(keys);
    options.load_secret_keys_from_env = true;
    if let Some(hook) = &self.secrets {
      hook(&mut options);
    }
    options
  }
}

impl Config {
  /// `Loader::at(path).config()`.
  pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
    Loader::at(path).config()
  }

  /// The sections out of a store the caller owns, rooted at `root`, with what
  /// the store left out inferred. Nothing here reads the filesystem for
  /// configuration, so an application that already loaded its own store hands
  /// it over: `Config::from_store_at(&store.branch("fsr"), root)`.
  ///
  /// **The caller owns the ladder too.** `config_paths` is not called, so a file
  /// reaches the host only if the store already holds it. Two of those files are
  /// not overlays and a store that omits either is wrong rather than minimal:
  ///
  /// - `app.toml` is the configuration. Without it there is nothing to load.
  /// - `bundle.toml`, whenever it exists, which is in a deploy tree and nowhere
  ///   else. `fsr bundle` writes it to name the paths that moved during layout.
  ///   A host that does not read it resolves every static root to where the file
  ///   sat in the project instead of where the tree put it, so a bundled
  ///   deployment answers 404 for its own assets.
  ///
  /// `<release_env>`, `<app_env>`, `<region>` and `<app_env>-<region>` are
  /// overlays: absent is normal, so omitting one costs only what it held.
  ///
  /// Ask for the list rather than restating it, since the stems grow:
  ///
  /// ```ignore
  /// let files = config::config_paths(&config_dir, &Deployment::from_env());
  /// ```
  ///
  /// Every path it returns exists, so it can be handed to a loader whole.
  pub fn from_store_at<S: C5Store>(store: &S, root: impl AsRef<Path>) -> Result<Self, HostError> {
    let root = root.as_ref();
    Self::sections(store, root, root.to_path_buf(), false, &[])
  }

  /// `from_store_at` over a store loaded from the files `located` names,
  /// which become the configuration's provenance and the path its errors
  /// report.
  pub fn from_store<S: C5Store>(store: &S, located: Located) -> Result<Self, HostError> {
    Self::from_store_keeping(store, located, false)
  }

  pub(super) fn from_store_keeping<S: C5Store>(store: &S, located: Located, keep_undecrypted: bool) -> Result<Self, HostError> {
    let at = located.sources.first().cloned().unwrap_or_else(|| located.root.clone());
    let mut config = Self::sections(store, &located.root, at, keep_undecrypted, &located.sources)?;
    if config.cache.as_ref().is_none_or(|cache| cache.data.is_none()) {
      if let Some(file) = located.sources.iter().find(|file| declares_empty_table(file, &["cache", "data"])) {
        return Err(HostError::Config(file.clone(), "[cache.data] has no keys, which turns nothing on; write `enabled = true` under it to answer cached methods from memory".to_owned()));
      }
    }
    config.sources = located.sources;
    Ok(config)
  }

  pub(super) fn sections<S: C5Store>(store: &S, root: &Path, at: PathBuf, keep_undecrypted: bool, sources: &[PathBuf]) -> Result<Self, HostError> {
    let fail = |e: ConfigError| HostError::Config(at.clone(), e.to_string());

    let mut ignored: Vec<String> = Vec::new();
    for key in store.key_paths_with_prefix(None) {
      let head = key.split('.').next().unwrap_or(&key);
      if !SECTIONS.contains(&head) && !ignored.iter().any(|h| h == head) {
        ignored.push(head.to_owned());
      }
    }
    ignored.sort();

    let mut public: BTreeMap<String, PublicValue> = BTreeMap::new();
    if store.path_exists("public") || !store.key_paths_with_prefix(Some("public")).is_empty() {
      let mut keys: Vec<String> = store
        .key_paths_with_prefix(Some("public"))
        .into_iter()
        .filter_map(|k| k.strip_prefix("public.").map(str::to_owned))
        .collect();
      if let Some(c5store::value::C5DataValue::Map(map)) = store.get("public") {
        keys.extend(map.keys().cloned());
      }
      keys.sort();
      keys.dedup();
      for key in keys {
        if let Some((head, _)) = key.split_once('.') {
          return Err(HostError::Config(
            at.clone(),
            format!("public.{head}: a table; a value is a string, integer, float or boolean"),
          ));
        }
        let valid = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
          && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
          return Err(HostError::Config(
            at.clone(),
            format!("public.{key}: a key is a TypeScript identifier, letters, digits and `_`"),
          ));
        }
        use c5store::value::C5DataValue as V;
        let value = match store.get(&format!("public.{key}")) {
          Some(V::String(s)) => PublicValue::Str(s),
          Some(V::Integer(i)) => PublicValue::Int(i),
          Some(V::UInteger(u)) => match i64::try_from(u) {
            Ok(i) => PublicValue::Int(i),
            Err(_) => return Err(HostError::Config(at.clone(), format!("public.{key}: {u} does not fit an i64"))),
          },
          Some(V::Float(f)) => PublicValue::Float(f),
          Some(V::Boolean(b)) => PublicValue::Bool(b),
          _ => {
            return Err(HostError::Config(
              at.clone(),
              format!("public.{key}: a value is a string, integer, float or boolean"),
            ));
          }
        };
        public.insert(key, value);
      }
    }

    /// A section, absent or not. An absent one is deserialized as an empty
    /// table rather than taken from `Default`, so every `serde(default)` the
    /// section declares applies: a derived `Default` ignores serde attributes,
    /// which silently drops the defaults that name a function.
    fn section<S: C5Store, T: for<'de> Deserialize<'de>>(store: &S, key: &str, at: &Path) -> Result<T, HostError> {
      let fail = |e: String| HostError::Config(at.to_path_buf(), format!("{key}: {e}"));
      if !store.path_exists(key) {
        return serde_json::from_str::<T>("{}").map_err(|e| fail(e.to_string()));
      }
      store.get_into_struct::<T>(key).map_err(|e| fail(e.to_string()))
    }

    let app_section: AppSection = section(store, "app", &at)?;
    let server: ServerConfig = section(store, "server", &at)?;
    let files: FilesSection = section(store, "files", &at)?;
    let document: DocumentConfig = section(store, "document", &at)?;
    let session: SessionSection = match store.path_exists("session") {
      true => store.get_into_struct("session").map_err(fail)?,
      false => SessionSection::default(),
    };
    // c5store stores a secret it could not decrypt as null, so the key path exists with no value.
    let mut undecrypted = Vec::new();
    if session.key.is_none() && store.path_exists("session.key") && keep_undecrypted {
      undecrypted.push("session.key".to_owned());
    } else if session.key.is_none() && store.path_exists("session.key") {
      return Err(HostError::Config(
        at.clone(),
        "`session.key` is written but has no value: a secret that does not decrypt loads empty".to_owned(),
      ));
    }
    let mut secrets: BTreeMap<String, SecretEntry> = section(store, "secrets", &at)?;
    // A table holding nothing yet, `[secrets.cms]` alone, is not in the store.
    for source in sources {
      let Some(text) = source.extension().filter(|e| *e == "toml").and_then(|_| std::fs::read_to_string(source).ok()) else { continue };
      let Ok(table) = text.parse::<toml::Table>() else { continue };
      if let Some(toml::Value::Table(declared)) = table.get("secrets") {
        for name in declared.keys() {
          secrets.entry(name.clone()).or_default();
        }
      }
    }
    for (name, entry) in secrets.iter_mut() {
      if entry.value.is_some() || !store.path_exists(&format!("secrets.{name}.value")) {
        continue;
      }
      if keep_undecrypted {
        undecrypted.push(format!("secrets.{name}.value"));
      } else {
        return Err(HostError::Config(at.clone(), format!("`secrets.{name}.value` is written but has no value: a secret that does not decrypt loads empty")));
      }
    }
    if !matches!(session.csrf.as_str(), "origin" | "identified" | "always") {
      return Err(HostError::Config(
        at.clone(),
        format!("session.csrf `{}` is not a choice; origin, identified or always", session.csrf),
      ));
    }
    for trusted in &server.trusted_origins {
      if crate::live::origin_of(trusted).as_deref() != Some(trusted.trim_end_matches('/')) {
        return Err(HostError::Config(at.clone(), format!("server.trusted_origins `{trusted}` is not an origin; write the scheme and host, `https://pay.example`")));
      }
    }
    if !matches!(session.csrf_scheme.as_str(), "single_use" | "session" | "derived") {
      return Err(HostError::Config(
        at.clone(),
        format!("session.csrf_scheme `{}` is not a choice; single_use, session or derived", session.csrf_scheme),
      ));
    }
    if session.csrf_outstanding == 0 {
      return Err(HostError::Config(at.clone(), "session.csrf_outstanding is at least 1".to_owned()));
    }
    let cache: Option<CacheSection> = if store.path_exists("cache") {
      Some(store.get_into_struct("cache").map_err(fail)?)
    } else {
      None
    };
    let store_section: StoreSection = if store.path_exists("store") { store.get_into_struct("store").map_err(fail)? } else { StoreSection::default() };
    for (i, name) in store_section.slot_order.iter().enumerate() {
      if name.is_empty() {
        return Err(HostError::Config(at.clone(), "store.slot_order names an empty slot".to_owned()));
      }
      if store_section.slot_order[..i].contains(name) {
        return Err(HostError::Config(at.clone(), format!("store.slot_order names `{name}` twice")));
      }
    }

    let mut clients = BTreeMap::new();
    let mut names: Vec<String> = store
      .key_paths_with_prefix(Some("clients"))
      .into_iter()
      .filter_map(|k| {
        k.strip_prefix("clients.")
          .map(|rest| rest.split('.').next().unwrap_or(rest).to_owned())
      })
      .collect();
    if let Some(c5store::value::C5DataValue::Map(map)) = store.get("clients") {
      names.extend(map.keys().cloned());
    }
    names.sort();
    names.dedup();
    for name in names {
      let client: ClientConfig = store.get_into_struct(&format!("clients.{name}")).map_err(fail)?;
      match client.transport.as_deref() {
        None | Some("mock") => {}
        Some(other) => return Err(HostError::Value(format!("clients.{name}.transport"), other.to_owned())),
      }
      if client.base_url.is_none() && !client.is_mock() {
        return Err(HostError::Config(
          at.clone(),
          format!("clients.{name}.base_url is required unless transport = \"mock\""),
        ));
      }
      clients.insert(name, client);
    }

    let mut statics: Vec<StaticRoot> = match store.get("static") {
      Some(value) => {
        let json = to_json(&value);
        serde_json::from_value(json).map_err(|e| HostError::Config(at.clone(), format!("static: {e}")))?
      }
      None => Vec::new(),
    };
    let routes: Vec<RouteDecl> = match store.get("route") {
      Some(value) => {
        let json = to_json(&value);
        serde_json::from_value(json).map_err(|e| HostError::Config(at.clone(), format!("route: {e}")))?
      }
      None => Vec::new(),
    };

    let locales: Option<LocalesSection> =
      if store.path_exists("locales") || !store.key_paths_with_prefix(Some("locales")).is_empty() {
        let mut json = serde_json::Map::new();
        for key in ["supported", "default", "order", "remember", "cookie"] {
          if let Some(value) = store.get(&format!("locales.{key}")) {
            json.insert(key.to_owned(), to_json(&value));
          }
        }
        Some(
          serde_json::from_value(serde_json::Value::Object(json))
            .map_err(|e| HostError::Config(at.clone(), format!("locales: {e}")))?,
        )
      } else {
        None
      };

    let auth: Option<AuthSection> =
      if store.path_exists("auth") || !store.key_paths_with_prefix(Some("auth")).is_empty() {
        let mut json = serde_json::Map::new();
        for key in ["provider", "login", "users", "client", "methods", "second_factor", "issuer", "preset", "tenant", "client_id", "client_secret", "scopes", "claims", "team_id", "key_id", "signing_key", "store", "rp_name", "verification"] {
          if let Some(value) = store.get(&format!("auth.{key}")) {
            json.insert(key.to_owned(), to_json(&value));
          }
        }
        let section: AuthSection = serde_json::from_value(serde_json::Value::Object(json))
          .map_err(|e| HostError::Config(at.clone(), format!("auth: {e}")))?;
        match (&section.provider, section.methods.is_empty()) {
          (None, true) => return Err(HostError::Config(at.clone(), "auth names no `provider` and no `[[auth.methods]]`".to_owned())),
          (Some(_), false) => return Err(HostError::Config(at.clone(), "auth names both a `provider` and `[[auth.methods]]`; one method is `provider`, several are `[[auth.methods]]`".to_owned())),
          _ => {}
        }
        let methods = section.all_methods();
        for (i, method) in methods.iter().enumerate() {
          let at_key = if section.methods.is_empty() { "auth".to_owned() } else { format!("auth.methods `{}`", method.name) };
          if methods[..i].iter().any(|earlier| earlier.name == method.name) {
            return Err(HostError::Config(at.clone(), format!("{at_key} is named twice")));
          }
          if !PROVIDERS.contains(&method.provider.as_str()) {
            let named = if section.methods.is_empty() { format!("auth.provider `{}`", method.provider) } else { format!("{at_key}: provider `{}`", method.provider) };
            return Err(HostError::Config(at.clone(), format!("{named} is not a provider; the providers are {}", PROVIDERS.join(", "))));
          }
          if method.provider == "oidc" {
            let refuse = |why: &str| Err(HostError::Config(at.clone(), format!("{at_key}: {why}")));
            if method.client_id.is_none() {
              return refuse("an oidc method needs `client_id`");
            }
            match (&method.preset, &method.issuer) {
              (None, None) => return refuse("an oidc method needs `issuer` or a `preset`"),
              (Some(preset), _) if !PRESETS.contains(&preset.as_str()) => return Err(HostError::Config(at.clone(), format!("{at_key}: preset `{preset}` is not one of {}", PRESETS.join(", ")))),
              (Some(preset), _) if preset == "apple" && (method.team_id.is_none() || method.key_id.is_none() || method.signing_key.is_none()) => return refuse("preset = \"apple\" needs `team_id`, `key_id` and `signing_key`"),
              _ => {}
            }
            for named in [&method.client_secret, &method.signing_key].into_iter().flatten() {
              if !secrets.contains_key(named) {
                return Err(HostError::Config(at.clone(), format!("{at_key}: names `{named}`, which no [secrets.{named}] declares")));
              }
            }
          }
          if method.provider == "passkey" && method.verification.as_deref().is_some_and(|v| v != "required" && v != "preferred") {
            return Err(HostError::Config(at.clone(), format!("{at_key}: verification is `required` or `preferred`")));
          }
          if method.provider == "passkey" {
            match &method.store {
              None => return Err(HostError::Config(at.clone(), format!("{at_key}: a passkey method needs `store`, the directory its passkeys are kept in, relative to the project: `store = \"data/passkeys\"`"))),
              Some(store) if Path::new(store).components().next().is_some_and(|first| first.as_os_str() == "config") => {
                return Err(HostError::Config(at.clone(), format!("{at_key}: store `{store}` is inside the configuration directory, which holds what the host reads and restarts `fsr dev` on every change; keep the passkeys beside it, `data/passkeys`")))
              }
              Some(_) => {}
            }
          }
          if method.provider == "service" {
            match &method.client {
              Some(client) if clients.contains_key(client) => {}
              Some(client) => return Err(HostError::Config(at.clone(), format!("{at_key}: client `{client}` is not a [clients] entry"))),
              None => return Err(HostError::Config(at.clone(), format!("{at_key}: provider = \"service\" needs a client"))),
            }
          }
        }
        if methods.iter().all(|method| method.provider == "passkey") {
          return Err(HostError::Config(
            at.clone(),
            "a passkey needs another method beside it in [[auth.methods]]: one that proves who a new visitor is, so an account can be made and a lost device recovered".to_owned(),
          ));
        }
        if let Some(second) = &section.second_factor {
          if !methods.iter().any(|method| &method.name == second) || methods.len() < 2 {
            return Err(HostError::Config(at.clone(), format!("auth.second_factor `{second}` must name one of two or more [[auth.methods]]")));
          }
        }
        if !section.login.starts_with('/') {
          return Err(HostError::Config(
            at.clone(),
            format!("auth.login `{}` must be a path", section.login),
          ));
        }
        Some(section)
      } else {
        None
      };
    let typecheck: Option<TypecheckSection> =
      if store.path_exists("typecheck") || !store.key_paths_with_prefix(Some("typecheck")).is_empty() {
        let mut json = serde_json::Map::new();
        for key in ["version", "sha512", "tsc", "enabled"] {
          if let Some(value) = store.get(&format!("typecheck.{key}")) {
            json.insert(key.to_owned(), to_json(&value));
          }
        }
        Some(
          serde_json::from_value(serde_json::Value::Object(json))
            .map_err(|e| HostError::Config(at.clone(), format!("typecheck: {e}")))?,
        )
      } else {
        None
      };
    let vendor: Option<VendorSection> = if store.path_exists("vendor") || !store.key_paths_with_prefix(Some("vendor")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in store.key_paths_with_prefix(Some("vendor")) {
        let name = key.strip_prefix("vendor.").and_then(|rest| rest.split('.').next()).unwrap_or_default();
        if !name.is_empty() && name != "hosts" {
          return Err(HostError::Config(at.clone(), format!("vendor.{name} is not a key; [vendor] takes hosts")));
        }
      }
      if let Some(value) = store.get("vendor.hosts") {
        json.insert("hosts".to_owned(), to_json(&value));
      }
      Some(serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("vendor: {e}")))?)
    } else {
      None
    };

    let build: Option<BuildSection> = if store.path_exists("build") || !store.key_paths_with_prefix(Some("build")).is_empty() {
      const KEYS: [&str; 5] = ["strict", "target", "lib", "before", "after"];
      for key in store.key_paths_with_prefix(Some("build")) {
        let name = key.strip_prefix("build.").and_then(|rest| rest.split('.').next()).unwrap_or_default();
        if !name.is_empty() && !KEYS.contains(&name) {
          return Err(HostError::Config(at.clone(), format!("build.{name} is not a key; [build] takes strict, target, lib, before and after")));
        }
      }
      let mut json = serde_json::Map::new();
      for key in KEYS {
        if let Some(value) = store.get(&format!("build.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: BuildSection = serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("build: {e}")))?;
      section.check().map_err(|why| HostError::Config(at.clone(), why))?;
      Some(section)
    } else {
      None
    };

    let time_zones = match store.get("time.zones").map(|v| to_json(&v)) {
      None => None,
      Some(value) => {
        let zones: TimeZones = serde_json::from_value(value.clone()).map_err(|_| HostError::Config(at.clone(), format!("time.zones is `compiled` or `system`, not {value}")))?;
        let source = match zones {
          TimeZones::Compiled => snapfire_time::Source::Compiled,
          TimeZones::System => snapfire_time::Source::System,
        };
        snapfire_time::choose(source).map_err(|e| HostError::Config(at.clone(), format!("time.zones: {e}")))?;
        Some(zones)
      }
    };

    let trace: Option<TraceSection> = if store.path_exists("trace") || !store.key_paths_with_prefix(Some("trace")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["expose", "inline", "report"] {
        if let Some(value) = store.get(&format!("trace.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: TraceSection = serde_json::from_value(serde_json::Value::Object(json))
        .map_err(|e| HostError::Config(at.clone(), format!("trace: {e}")))?;
      for (key, patterns) in [("expose", &section.expose), ("inline", &section.inline), ("report", &section.report)] {
        let mut router = matchit::Router::new();
        for pattern in patterns {
          if !pattern.starts_with('/') {
            return Err(HostError::Config(at.clone(), format!("trace.{key} `{pattern}` must be a path")));
          }
          if let Err(e) = router.insert(pattern.as_str(), ()) {
            return Err(HostError::Config(at.clone(), format!("trace.{key} `{pattern}` is not a route pattern: {e}")));
          }
        }
      }
      Some(section)
    } else {
      None
    };

    let desktop: Option<DesktopSection> = if store.path_exists("desktop") || !store.key_paths_with_prefix(Some("desktop")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["name", "serve", "cookies"] {
        if let Some(value) = store.get(&format!("desktop.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: DesktopSection = serde_json::from_value(serde_json::Value::Object(json))
        .map_err(|e| HostError::Config(at.clone(), format!("desktop: {e}")))?;
      Some(section)
    } else {
      None
    };

    let pwa: Option<PwaSection> = if store.path_exists("pwa") || !store.key_paths_with_prefix(Some("pwa")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["name", "short_name", "description", "theme_color", "background_color", "display", "start_url", "file", "worker", "offline"] {
        if let Some(value) = store.get(&format!("pwa.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: PwaSection = serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("pwa: {e}")))?;
      section.check().map_err(|why| HostError::Config(at.clone(), format!("pwa {why}")))?;
      Some(section)
    } else {
      None
    };

    let mut mdx = MdxSection::default();
    let mdx_keys: Vec<String> = store.key_paths_with_prefix(Some("mdx")).into_iter().filter_map(|k| k.strip_prefix("mdx.").map(str::to_owned)).collect();
    for key in &mdx_keys {
      let value = store.get(&format!("mdx.{key}"));
      let flag = |value: Option<c5store::value::C5DataValue>| match value.as_ref().map(to_json) {
        Some(serde_json::Value::Bool(b)) => Ok(b),
        _ => Err(HostError::Config(at.clone(), format!("mdx.{key} must be true or false"))),
      };
      match key.split('.').next().unwrap_or(key) {
        "gfm" => mdx.gfm = flag(value)?,
        "heading_ids" => mdx.heading_ids = flag(value)?,
        "frontmatter" => mdx.frontmatter = flag(value)?,
        "highlight" => mdx.highlight = Some(flag(value)?),
        "collections" => {
          let Some(name) = key.strip_prefix("collections.") else { continue };
          match value.as_ref().map(to_json) {
            Some(serde_json::Value::String(dir)) if !dir.starts_with('/') && !dir.split('/').any(|part| part == "..") => {
              mdx.collections.insert(name.to_owned(), dir.trim_end_matches('/').to_owned());
            }
            Some(serde_json::Value::Object(_)) => {}
            _ => return Err(HostError::Config(at.clone(), format!("mdx.collections.{name} must be a directory under the app, as \"content/posts\""))),
          }
        }
        other => return Err(HostError::Config(at.clone(), format!("mdx.{other} is not a key; [mdx] takes gfm, heading_ids, frontmatter, highlight and collections"))),
      }
    }

    let crawl: Option<CrawlSection> = if store.path_exists("crawl") || !store.key_paths_with_prefix(Some("crawl")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["robots", "sitemap", "disallow"] {
        if let Some(value) = store.get(&format!("crawl.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: CrawlSection = serde_json::from_value(serde_json::Value::Object(json))
        .map_err(|e| HostError::Config(at.clone(), format!("crawl: {e}")))?;
      for (key, path) in [("robots", &section.robots), ("sitemap", &section.sitemap)] {
        if let Some(path) = path.as_deref().filter(|p| !p.starts_with('/')) {
          return Err(HostError::Config(at.clone(), format!("crawl.{key} `{path}` must be a path")));
        }
      }
      if let Some(bad) = section.disallow.iter().find(|p| !p.starts_with('/')) {
        return Err(HostError::Config(at.clone(), format!("crawl.disallow `{bad}` must be a path")));
      }
      if section.robots.is_none() && section.sitemap.is_none() {
        return Err(HostError::Config(at.clone(), "crawl names neither `robots` nor `sitemap`, so it serves nothing".to_owned()));
      }
      if document.origin.is_none() {
        return Err(HostError::Config(at.clone(), "crawl needs `document.origin`: a sitemap names absolute URLs and `robots.txt` names the sitemap's".to_owned()));
      }
      Some(section)
    } else {
      None
    };

    if session.store == "service" {
      match &session.client {
        Some(client) if clients.contains_key(client) => {}
        Some(client) => {
          return Err(HostError::Config(
            at.clone(),
            format!("session.client names `{client}`, which is not a [clients] entry"),
          ));
        }
        None => {
          return Err(HostError::Config(
            at.clone(),
            "session.store = \"service\" needs session.client".to_owned(),
          ));
        }
      }
    }

    let site: Option<SiteSection> = if store.path_exists("site")
      || !store.key_paths_with_prefix(Some("site")).is_empty()
    {
      let mut json = serde_json::Map::new();
      for key in ["name", "at", "shell", "probe"] {
        if let Some(value) = store.get(&format!("site.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: SiteSection = serde_json::from_value(serde_json::Value::Object(json))
        .map_err(|e| HostError::Config(at.clone(), format!("site: {e}")))?;
      if section.name.is_empty()
        || !section
          .name
          .chars()
          .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
      {
        return Err(HostError::Config(
          at.clone(),
          format!(
            "site.name `{}` must be lowercase letters, digits, `_` or `-`",
            section.name
          ),
        ));
      }
      if !section.at.starts_with('/') || section.at.len() < 2 || section.at.ends_with('/') || section.at.contains('{') {
        return Err(HostError::Config(
          at.clone(),
          format!(
            "site.at `{}` must be a path such as `/billing`, with no trailing slash",
            section.at
          ),
        ));
      }
      Some(section)
    } else {
      None
    };

    let sites: Option<SitesSection> =
      if store.path_exists("sites") || !store.key_paths_with_prefix(Some("sites")).is_empty() {
        let scalar = |key: &str| -> Result<Option<String>, HostError> {
          match store.get(&format!("sites.{key}")) {
            Some(value) => match to_json(&value) {
              serde_json::Value::String(s) => Ok(Some(s)),
              other => Err(HostError::Config(
                at.clone(),
                format!("sites.{key} must be a string, found {other}"),
              )),
            },
            None => Ok(None),
          }
        };
        let root = scalar("root")?;
        let poll = scalar("poll")?;
        let fetch_from = scalar("store")?;
        if fetch_from.is_some() && root.is_none() {
          return Err(HostError::Config(
            at.clone(),
            "sites.store fetches into sites.root, so it needs one".to_owned(),
          ));
        }
        if let Some(poll) = &poll {
          if parse_duration(poll).is_none() {
            return Err(HostError::Config(
              at.clone(),
              format!("sites.poll `{poll}` is not a duration"),
            ));
          }
        }
        let mut names: Vec<String> = store
          .key_paths_with_prefix(Some("sites"))
          .into_iter()
          .filter_map(|k| {
            k.strip_prefix("sites.")
              .map(|rest| rest.split('.').next().unwrap_or(rest).to_owned())
          })
          .filter(|name| !["root", "poll", "store"].contains(&name.as_str()))
          .collect();
        if let Some(c5store::value::C5DataValue::Map(map)) = store.get("sites") {
          names.extend(map.keys().filter(|k| !["root", "poll", "store"].contains(&k.as_str())).cloned());
        }
        names.sort();
        names.dedup();
        let mut mounts = BTreeMap::new();
        for name in names {
          let mount: MountConfig = store.get_into_struct(&format!("sites.{name}")).map_err(fail)?;
          mount.check(&name).map_err(|message| HostError::Config(at.clone(), message))?;
          if let Some(artifact) = &mount.artifact {
            if artifact.contains('@') && !artifact.contains('/') && root.is_none() {
              return Err(HostError::Config(
                at.clone(),
                format!("sites.{name}.artifact `{artifact}` names a version, which needs sites.root"),
              ));
            }
          }
          mounts.insert(name, mount);
        }
        Some(SitesSection { root, poll, store: fetch_from, mounts })
      } else {
        None
      };
    if site.is_some() && sites.is_some() {
      return Err(HostError::Config(
        at.clone(),
        "a site cannot mount sites; drop [sites] or [site]".to_owned(),
      ));
    }

    let root = root.to_path_buf();
    let app = root.join(&app_section.dir);
    let mut inferred = Vec::new();
    let css_route = match &site {
      Some(site) => site.under("/static/css"),
      None => "/static/css".to_owned(),
    };

    let mut document = document;
    // Linked after the document's own sheets, whether written or inferred from
    // `styles/`, so a component's rules come later in the cascade.
    let mut component_sheets: Vec<String> = Vec::new();
    let mut facts_dir = "dist".to_owned();
    let mut bundle: Option<Bundle> = None;
    if let Some((dir, facts)) = build_facts(&app, &statics) {
      facts_dir = dir.clone();
      if let Some(public_path) = facts.public_path {
        let route = public_path.trim_end_matches('/').to_owned();
        // A deploy tree keeps the facts in `dist/` and the outputs under the static root
        // that serves the route, which is where their content is hashed from.
        let outputs_dir = statics.iter().find(|s| s.route == route).map_or_else(|| app.join(&dir), |s| app.join(&s.dir));
        bundle = Some(Bundle {
          route: route.clone(),
          graph: facts.graph.clone(),
          externals: facts.externals.clone(),
          minified: facts.minified.clone(),
          outputs: facts.outputs.iter().cloned().collect(),
          styles: facts.styles.clone(),
          dir: outputs_dir,
          hashed: Default::default(),
        });
        if !statics.iter().any(|s| s.route == route) {
          statics.push(StaticRoot {
            route: route.clone(),
            dir: dir.clone(),
          });
          inferred.push(format!("static {route} from {dir}/.snapfire-build.json"));
        }
        if document.entry.is_none() && facts.entries.iter().any(|e| e == "src/main.js") {
          // From the route rather than from the path it was trimmed out of:
          // the entry is a URL the static root above answers, so a public
          // path written without a trailing slash cannot run the two together.
          document.entry = Some(format!("{route}/src/main.js"));
          inferred.push(format!("document.entry from {dir}/.snapfire-build.json"));
        }
        component_sheets = facts.styles.iter().map(|style| format!("{route}/{style}")).collect();
      }
    }
    if document.import_map.is_none() && app.join("importmap.json").is_file() {
      document.import_map = Some("importmap.json".to_owned());
      inferred.push("document.import_map from importmap.json".to_owned());
    }
    let vendor_route = match &site {
      Some(site) => site.under("/static/js/vendor"),
      None => "/static/js/vendor".to_owned(),
    };
    if app.join("vendor").is_dir() && !statics.iter().any(|s| s.route == vendor_route) {
      statics.push(StaticRoot {
        route: vendor_route.clone(),
        dir: "vendor".to_owned(),
      });
      inferred.push(format!("static {vendor_route} from vendor/"));
    }
    let dirs: DirsSection = section(store, "dirs", &at)?;
    if site.is_none() && app.join(&dirs.public).is_dir() && !statics.iter().any(|s| s.route.trim_end_matches('/').is_empty() || s.dir.trim_end_matches('/') == dirs.public) {
      statics.push(StaticRoot {
        route: "/".to_owned(),
        dir: dirs.public.clone(),
      });
      inferred.push(format!("static / from {}/", dirs.public));
    }
    let images: ImagesSection = section(store, "images", &at)?;
    let media: MediaSection = section(store, "media", &at)?;
    for width in &images.widths {
      if *width == 0 {
        return Err(HostError::Config(at.clone(), "images.widths: a width is at least 1".to_owned()));
      }
    }
    for format in &images.formats {
      if format != "avif" && format != "webp" {
        return Err(HostError::Config(at.clone(), format!("images.formats: `{format}` is not `avif` or `webp`")));
      }
    }
    for (format, quality) in &images.quality {
      if format != "avif" && format != "webp" {
        return Err(HostError::Config(at.clone(), format!("images.quality: `{format}` is not `avif` or `webp`")));
      }
      if *quality > 100 {
        return Err(HostError::Config(at.clone(), format!("images.quality.{format}: {quality} is above 100")));
      }
    }
    if let Some(base) = &images.base {
      if !base.starts_with("https://") && !base.starts_with("http://") || base.ends_with('/') {
        return Err(HostError::Config(at.clone(), format!("images.base `{base}` must be `scheme://host/path` with no trailing slash")));
      }
    }
    for (name, source) in &images.sources {
      if !source.template.contains("{src}") {
        return Err(HostError::Config(at.clone(), format!("images.sources.{name}.template must carry `{{src}}`")));
      }
    }
    let fonts: FontsSection = if store.path_exists("fonts") || !store.key_paths_with_prefix(Some("fonts")).is_empty() {
      let base = match store.get("fonts.base") {
        Some(value) => match to_json(&value) {
          serde_json::Value::String(s) => Some(s),
          other => return Err(HostError::Config(at.clone(), format!("fonts.base must be a string, found {other}"))),
        },
        None => None,
      };
      if let Some(base) = &base {
        if !base.starts_with("https://") && !base.starts_with("http://") || base.ends_with('/') {
          return Err(HostError::Config(at.clone(), format!("fonts.base `{base}` must be `scheme://host/path` with no trailing slash")));
        }
      }
      let mut keys: Vec<String> = store
        .key_paths_with_prefix(Some("fonts"))
        .into_iter()
        .filter_map(|k| k.strip_prefix("fonts.").map(|rest| rest.split('.').next().unwrap_or(rest).to_owned()))
        .filter(|k| k != "base")
        .collect();
      if let Some(c5store::value::C5DataValue::Map(map)) = store.get("fonts") {
        keys.extend(map.keys().filter(|k| *k != "base").cloned());
      }
      keys.sort();
      keys.dedup();
      let mut faces = BTreeMap::new();
      for key in keys {
        let valid = key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
          && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
          return Err(HostError::Config(at.clone(), format!("fonts.{key}: a key is lowercase letters, digits and `-`, since it names `--font-{key}`")));
        }
        let entry: FontEntry = store
          .get_into_struct(&format!("fonts.{key}"))
          .map_err(|e| HostError::Config(at.clone(), format!("fonts.{key}: {e}")))?;
        if let Some(remote) = &entry.remote {
          if !remote.starts_with("https://") && !remote.starts_with("http://") {
            return Err(HostError::Config(at.clone(), format!("fonts.{key}.remote `{remote}` must be a URL")));
          }
          if !entry.files.is_empty() {
            return Err(HostError::Config(at.clone(), format!("fonts.{key}: `remote` and `files` name two sources for one key")));
          }
        }
        if let Some(variable) = &entry.variable {
          if !variable.starts_with("--") || variable.len() < 3 {
            return Err(HostError::Config(at.clone(), format!("fonts.{key}.variable `{variable}` must start with `--`")));
          }
        }
        faces.insert(key, entry);
      }
      FontsSection { base, faces }
    } else {
      FontsSection::default()
    };

    let shell_vendor = site
      .as_ref()
      .and_then(|s| s.shell.as_deref())
      .map(|shell| root.join(shell))
      .filter(|shell| shell.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "generated"))
      .and_then(|shell| shell.parent().and_then(|p| p.parent()).map(|app| app.join("vendor")))
      .filter(|dir| dir.is_dir());
    if shell_vendor.is_some() {
      inferred.push("static /static/js/vendor from the shell's vendor/, served when running alone".to_owned());
    }
    let icons_route = match &site {
      Some(site) => site.under("/static/icons"),
      None => "/static/icons".to_owned(),
    };
    // `fsr bundle` moves `icons/` under `serve/`, so the directory answering
    // this route is the tree's rather than the project's. Whichever root
    // serves it is where the files are; the href is the route either way.
    let icons_dir = statics
      .iter()
      .find(|s| s.route == icons_route)
      .map(|s| app.join(&s.dir))
      .filter(|dir| dir.is_dir())
      .or_else(|| app.join(&dirs.icons).is_dir().then(|| app.join(&dirs.icons)));
    let mut favicon = None;
    if let Some(icons_dir) = icons_dir {
      if !statics.iter().any(|s| s.route == icons_route) {
        statics.push(StaticRoot {
          route: icons_route.clone(),
          dir: dirs.icons.clone(),
        });
        inferred.push(format!("static {icons_route} from {}/", dirs.icons));
      }
      let held = |name: &str| icons_dir.join(name).is_file();
      let mut linked = Vec::new();
      for (file, mut attrs) in [
        ("favicon.svg", vec![("rel", "icon"), ("type", "image/svg+xml")]),
        ("favicon.ico", vec![("rel", "icon"), ("type", "image/x-icon")]),
        (
          "favicon-32x32.png",
          vec![("rel", "icon"), ("type", "image/png"), ("sizes", "32x32")],
        ),
        (
          "favicon-16x16.png",
          vec![("rel", "icon"), ("type", "image/png"), ("sizes", "16x16")],
        ),
        (
          "apple-touch-icon.png",
          vec![("rel", "apple-touch-icon"), ("sizes", "180x180")],
        ),
        ("site.webmanifest", vec![("rel", "manifest")]),
      ] {
        if !held(file) {
          continue;
        }
        attrs.push(("href", ""));
        let mut table: BTreeMap<String, String> =
          attrs.into_iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect();
        table.insert("tag".to_owned(), "link".to_owned());
        table.insert("href".to_owned(), format!("{icons_route}/{file}"));
        document.head.push(table);
        linked.push(file);
      }
      if !linked.is_empty() {
        inferred.push(format!("document.head links [{}] from {icons_route}", linked.join(", ")));
      }
      if held("favicon.ico") {
        let at = format!("{icons_route}/favicon.ico");
        inferred.push(format!("/favicon.ico from {at}"));
        favicon = Some(at);
      }
    }
    let has_icon = document.head.iter().any(|t| t.get("rel").is_some_and(|rel| rel.split_whitespace().any(|r| r == "icon")));
    if document.empty_icon && !has_icon {
      let table: BTreeMap<String, String> = [("tag", "link"), ("rel", "icon"), ("href", "data:,")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
      document.head.push(table);
      inferred.push("document.head links an empty icon, since no icons/ serves one".to_owned());
    }
    if app.join(&dirs.styles).is_dir() {
      if !statics.iter().any(|s| s.route == css_route) {
        statics.push(StaticRoot {
          route: css_route.clone(),
          dir: dirs.styles.clone(),
        });
        inferred.push(format!("static {css_route} from {}/", dirs.styles));
      }
      if document.styles.is_none() {
        let compiled = |name: &str| bundle.as_ref().filter(|b| b.outputs.contains(&format!("{}/{name}", dirs.styles))).map(|b| format!("{}/{}/{name}", b.route, dirs.styles));
        let mut from_bundle = false;
        let mut sheets: Vec<String> = std::fs::read_dir(app.join(&dirs.styles))
          .map(|entries| {
            entries
              .filter_map(|e| e.ok())
              .map(|e| e.path())
              .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "css"))
              .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
              .map(|name| match compiled(&name) {
                Some(url) => {
                  from_bundle = true;
                  url
                }
                None => format!("{css_route}/{name}"),
              })
              .collect()
          })
          .unwrap_or_default();
        sheets.sort();
        let themes_prefix = format!("{}/themes/", dirs.styles);
        let mut themes: Vec<String> = bundle
          .iter()
          .flat_map(|b| {
            let twin = b.minified.clone().map(|suffix| format!("{suffix}.css"));
            let prefix = themes_prefix.as_str();
            b.outputs.iter().filter(move |o| o.starts_with(prefix) && o.ends_with(".css") && twin.as_ref().is_none_or(|twin| !o.ends_with(twin.as_str()))).map(|o| format!("{}/{o}", b.route))
          })
          .collect();
        themes.sort();
        let themed = !themes.is_empty();
        sheets.extend(themes);
        if !sheets.is_empty() {
          document.styles = Some(sheets);
          inferred.push(match from_bundle {
            true => format!("document.styles from styles/*.css as {facts_dir}/.snapfire-build.json compiled them"),
            false => "document.styles from styles/*.css".to_owned(),
          });
          if themed {
            inferred.push(format!("document.styles gains the named theme sheets from {facts_dir}/.snapfire-build.json"));
          }
        }
      }
    }
    if !component_sheets.is_empty() {
      let sheets = document.styles.get_or_insert_with(Vec::new);
      for href in component_sheets {
        if !sheets.contains(&href) {
          sheets.push(href);
        }
      }
      inferred.push(format!("document.styles gains the component stylesheets from {facts_dir}/.snapfire-build.json"));
    }
    for (name, client) in clients.iter_mut() {
      if client.document.is_none() {
        let openapi = format!("clients/{name}.openapi.json");
        let others = [format!("clients/{name}.proto"), format!("clients/{name}.graphql")];
        client.document = Some(match others.into_iter().find(|d| app.join(d).is_file()) {
          Some(other) if !app.join(&openapi).is_file() => other,
          _ => openapi,
        });
        inferred.push(format!("clients.{name}.document from clients/"));
      }
    }
    for (name, client) in &clients {
      if let Some(timeout) = &client.timeout {
        if parse_duration(timeout).is_none() {
          return Err(HostError::Config(at.clone(), format!("clients.{name}.timeout `{timeout}` is not a duration")));
        }
      }
      let over_http = client.document.as_deref().is_none_or(|d| !d.ends_with(".proto") && !d.ends_with(".graphql"));
      if !over_http && (client.user_agent.is_some() || client.timeout.is_some() || client.max_connections.is_some()) {
        return Err(HostError::Config(at.clone(), format!("clients.{name}: `user_agent`, `timeout` and `max_connections` apply only to an OpenAPI client")));
      }
    }
    for (name, client) in &clients {
      let Some(credential) = &client.credential else { continue };
      let refuse = |why: String| Err(HostError::Config(at.clone(), format!("clients.{name}.credential: {why}")));
      if !secrets.contains_key(&credential.secret) {
        return refuse(format!("names `{}`, which no [secrets.{}] declares", credential.secret, credential.secret));
      }
      if credential.header.is_some() && credential.query.is_some() {
        return refuse("a credential goes out as a header or a query parameter, not both".to_owned());
      }
      if credential.query.is_some() && client.document.as_deref().is_some_and(|d| d.ends_with(".proto")) {
        return refuse("a gRPC call has no query string; send it as a header".to_owned());
      }
      let authorization = credential.header.as_deref().is_none_or(|h| h.eq_ignore_ascii_case("authorization")) && credential.query.is_none();
      if authorization && client.bearer.as_ref().and_then(BearerKey::key).is_some() {
        return refuse("`bearer` already sends the session's token as `Authorization`; name another header".to_owned());
      }
    }

    Ok(Self {
      root,
      app,
      runs: app_section.runs,
      sources: Vec::new(),
      server,
      document,
      session,
      cache,
      store: store_section,
      clients,
      statics,
      routes,
      locales,
      auth,
      typecheck,
      vendor,
      build,
      trace,
      crawl,
      time_zones,
      desktop,
      pwa,
      files,
      mdx,
      site,
      sites,
      dirs,
      images,
      media,
      fonts,
      shell_vendor,
      favicon,
      public,
      bundle,
      inferred,
      ignored,
      deployment: None,
      undecrypted,
      secrets,
    })
  }
}
