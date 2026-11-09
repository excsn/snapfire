use super::*;

impl Config {
  /// A path from the file, against the app directory.
  pub fn resolve(&self, relative: &str) -> PathBuf {
    self.app.join(relative)
  }

  /// The directory the first configuration file came from, which is where
  /// `auth.users` resolves; the project root when nothing was loaded.
  /// The origins beside this site's own a write may come from: `server.trusted_origins`, then each sign-in issuer that posts its callback back (`form_post`), Apple's.
  pub fn trusted_origins(&self) -> Vec<String> {
    let mut out: Vec<String> = self.server.trusted_origins.iter().map(|o| o.trim_end_matches('/').to_ascii_lowercase()).collect();
    let posting = self.auth.as_ref().map(|auth| auth.all_methods()).unwrap_or_default().into_iter().filter(|m| m.provider == "oidc" && m.preset.as_deref() == Some("apple"));
    for _ in posting {
      out.push("https://appleid.apple.com".to_owned());
    }
    out.dedup();
    out
  }

  pub fn config_dir(&self) -> PathBuf {
    self
      .sources
      .first()
      .and_then(|p| p.parent())
      .map(Path::to_path_buf)
      .unwrap_or_else(|| {
        let config = self.root.join("config");
        if config.is_dir() { config } else { self.root.clone() }
      })
  }

  /// `document.origin` checked: a scheme this host can be reached over, a
  /// host after it and nothing else, since anything further would land in the
  /// middle of a URL it is prefixed to.
  pub fn origin(&self) -> Result<Option<String>, HostError> {
    let Some(origin) = self.document.origin.as_deref() else {
      return Ok(None);
    };
    let why = |reason: &str| HostError::Value("document.origin".to_owned(), format!("`{origin}` {reason}"));
    let rest = origin
      .strip_prefix("https://")
      .or_else(|| origin.strip_prefix("http://"))
      .ok_or_else(|| why("needs an https:// or http:// scheme"))?;
    if rest.is_empty() {
      return Err(why("names no host"));
    }
    if rest.contains('/') {
      return Err(why("carries a path; an origin is the scheme and host alone"));
    }
    Ok(Some(origin.to_owned()))
  }

  pub fn session_ttl(&self) -> Result<Duration, HostError> {
    parse_duration(&self.session.ttl)
      .ok_or_else(|| HostError::Value("session.ttl".to_owned(), self.session.ttl.clone()))
  }

  /// Whether development conveniences are on: `server.dev` when written,
  /// else whether the deployment loaded is `development`, the environment's
  /// `RELEASE_ENV` when nothing says which was.
  pub fn dev(&self) -> bool {
    self.server.dev.unwrap_or_else(|| self.deployment.clone().unwrap_or_else(Deployment::from_env).release_env == "development")
  }

  /// The values this deployment must hold that its files may leave to the
  /// environment, each with the `C5_` variable that sets it. An absent
  /// session key is set in development, which mints one; a placeholder is
  /// one everywhere.
  /// The keys `runtime_values` reports on, whatever their state.
  pub fn default_runtime_keys() -> &'static [&'static str] {
    &["session.key"]
  }

  pub fn runtime_values(&self) -> Vec<RuntimeValue> {
    let state = match self.session.key.as_deref() {
      None if self.undecrypted.iter().any(|key| key == "session.key") => ValueState::Encrypted,
      Some(SESSION_KEY_PLACEHOLDER) => ValueState::Placeholder,
      None if !self.dev() => ValueState::Missing,
      _ => ValueState::Set,
    };
    let mut values = vec![RuntimeValue::new("session.key", state)];
    for name in self.sent_secrets() {
      let key = format!("secrets.{name}.value");
      let state = match self.secrets.get(name).and_then(|entry| entry.value.as_ref()) {
        Some(_) => ValueState::Set,
        None if self.undecrypted.contains(&key) => ValueState::Encrypted,
        None => ValueState::Missing,
      };
      values.push(RuntimeValue::new(key, state));
    }
    values
  }

  /// The secrets a client that reaches a backend sends, by name; a mock
  /// client sends nothing.
  pub fn sent_secrets(&self) -> Vec<&str> {
    let mut names: Vec<&str> = self.clients.values().filter(|client| !client.is_mock()).filter_map(|client| client.credential.as_ref()).map(|c| c.secret.as_str()).collect();
    names.sort();
    names.dedup();
    names
  }

  /// Each secret's value written as plain text in one of `sources`, which
  /// only a development deployment may hold.
  pub(super) fn plain_secrets(&self) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for source in &self.sources {
      let Ok(text) = std::fs::read_to_string(source) else { continue };
      let Ok(table) = text.parse::<toml::Table>() else { continue };
      let Some(toml::Value::Table(secrets)) = table.get("secrets") else { continue };
      for (name, entry) in secrets {
        if let Some(toml::Value::String(_)) = entry.get("value") {
          found.push((source.clone(), name.clone()));
        }
      }
    }
    found
  }

  /// The cache lifetime, `None` when no `[cache]` section is written.
  pub fn cache_ttl(&self) -> Result<Option<Duration>, HostError> {
    let Some(cache) = &self.cache else { return Ok(None) };
    parse_duration(&cache.ttl)
      .map(Some)
      .ok_or_else(|| HostError::Value("cache.ttl".to_owned(), cache.ttl.clone()))
  }
}
