//! `config/`, loaded through c5store in the order `config_paths` gives so
//! deployment overlays layer over `app.toml` and `C5_` environment variables
//! win, plus what the host infers from the app directory so the file only
//! carries deployment facts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use c5store::error::ConfigError;
use c5store::{create_c5store, C5Store, C5StoreOptions};
use serde::Deserialize;

use crate::locale::LocalesSection;
use crate::HostError;

/// The host's configuration after loading and inference. `root` is the
/// project directory, `app` the application directory under it.
#[derive(Debug, Clone)]
pub struct Config {
  pub root: PathBuf,
  pub app: PathBuf,
  pub sources: Vec<PathBuf>,
  pub server: ServerConfig,
  pub document: DocumentConfig,
  pub session: SessionSection,
  /// The render memo; absent means nothing is cached.
  pub cache: Option<CacheSection>,
  pub clients: BTreeMap<String, ClientConfig>,
  pub statics: Vec<StaticRoot>,
  /// The locales the application serves; absent means one, `en`, with no
  /// prefix, cookie or header consulted.
  pub locales: Option<LocalesSection>,
  /// The identity provider; absent means no login and no `/auth/` routes.
  pub auth: Option<AuthSection>,
  /// Which settings were inferred rather than written, for the report.
  pub inferred: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSection {
  #[serde(default = "default_app_dir")]
  pub dir: String,
}

impl Default for AppSection {
  fn default() -> Self {
    Self { dir: default_app_dir() }
  }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
  #[serde(default = "default_listen")]
  pub listen: String,
  #[serde(default = "default_plan")]
  pub plan: String,
  /// The directory of contract files `fsr build` writes, merged at boot in name order.
  #[serde(default = "default_contracts")]
  pub contracts: String,
  /// Where `prerender` writes and the host reads a route rendered once at
  /// build time, relative to the app directory. Absent, nothing is prerendered.
  #[serde(default)]
  pub prerender: Option<String>,
  /// Whether the document carries the live-refresh script and the host
  /// answers `/__fsr/events` and `/__fsr/changed`. Absent, it follows
  /// `RELEASE_ENV`: on when that is `development`, its default.
  #[serde(default)]
  pub dev: Option<bool>,
}

impl Default for ServerConfig {
  fn default() -> Self {
    Self { listen: default_listen(), plan: default_plan(), contracts: default_contracts(), prerender: None, dev: None }
  }
}

/// The document shell. `entry`, `import_map` and `styles` are inferred when absent.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DocumentConfig {
  #[serde(default)]
  pub title: String,
  #[serde(default)]
  pub entry: Option<String>,
  #[serde(default)]
  pub import_map: Option<String>,
  /// Stylesheet URLs linked from the head, in order.
  #[serde(default)]
  pub styles: Option<Vec<String>>,
  #[serde(default = "default_shell")]
  pub shell: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSection {
  /// The cookie signing key. Required, so a deployment never runs on a default.
  pub key: String,
  #[serde(default = "default_store")]
  pub store: String,
  #[serde(default = "default_ttl")]
  pub ttl: String,
  #[serde(default = "default_capacity")]
  pub capacity: u64,
  #[serde(default)]
  pub secure: bool,
}

/// The render memo: evaluated subtrees keyed by plan node, params, identity
/// and a fingerprint of their data, so a hit skips evaluation and a data
/// change is a miss. `ttl` bounds how long an entry nobody invalidates lives.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheSection {
  #[serde(default = "default_cache_capacity")]
  pub capacity: u64,
  #[serde(default = "default_cache_ttl")]
  pub ttl: String,
}

fn default_cache_capacity() -> u64 {
  1000
}

fn default_cache_ttl() -> String {
  "1m".to_owned()
}

/// A service the application calls. `document` defaults to
/// `clients/<name>.openapi.json` under the app directory, or
/// `clients/<name>.proto` when that is the file present; a `.proto` document
/// is reached over gRPC, anything else over HTTP.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientConfig {
  #[serde(default)]
  pub document: Option<String>,
  pub base_url: String,
  /// Which custody entry goes out as a bearer token on this client's calls:
  /// `true` for `access_token`, a string for another key. Absent, the
  /// client carries no token.
  #[serde(default)]
  pub bearer: Option<BearerKey>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum BearerKey {
  Toggle(bool),
  Named(String),
}

impl BearerKey {
  /// The custody key, `None` when written as `false`.
  pub fn key(&self) -> Option<&str> {
    match self {
      Self::Toggle(true) => Some("access_token"),
      Self::Toggle(false) => None,
      Self::Named(key) => Some(key.as_str()),
    }
  }
}

/// The `[auth]` section: which provider signs users in and where its login
/// page is. `users` is the `file` provider's table, relative to the
/// configuration directory and read on its own, never through the ladder.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthSection {
  pub provider: String,
  #[serde(default = "default_login")]
  pub login: String,
  #[serde(default)]
  pub users: Option<String>,
}

pub const PROVIDERS: &[&str] = &["file"];

fn default_login() -> String {
  "/login".to_owned()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StaticRoot {
  pub route: String,
  pub dir: String,
}

const SECTIONS: &[&str] = &["app", "server", "document", "session", "cache", "clients", "static", "locales", "auth"];

fn default_app_dir() -> String {
  "app".to_owned()
}
fn default_listen() -> String {
  "127.0.0.1:8080".to_owned()
}
fn default_plan() -> String {
  "generated/plan.json".to_owned()
}
fn default_contracts() -> String {
  "generated/contracts".to_owned()
}
fn default_shell() -> String {
  "shell#document".to_owned()
}
fn default_store() -> String {
  "memory".to_owned()
}
fn default_ttl() -> String {
  "8h".to_owned()
}
fn default_capacity() -> u64 {
  4096
}

/// The deployment axes, read from the environment: `RELEASE_ENV` (default `development`), `APP_ENV` (default `local`) and `APP_REGION` (no default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
  pub release_env: String,
  pub app_env: String,
  pub region: Option<String>,
}

impl Deployment {
  pub fn from_env() -> Self {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    Self {
      release_env: var("RELEASE_ENV").unwrap_or_else(|| "development".to_owned()),
      app_env: var("APP_ENV").unwrap_or_else(|| "local".to_owned()),
      region: var("APP_REGION"),
    }
  }
}

impl Default for Deployment {
  fn default() -> Self {
    Self { release_env: "development".to_owned(), app_env: "local".to_owned(), region: None }
  }
}

/// The files a configuration directory contributes, in loading order: `app`, `<release_env>`, `<app_env>`, `<region>` and `<app_env>-<region>`, each as `.toml` then `.yaml`, keeping only those that exist. Any other file in the directory is ignored.
pub fn config_paths(dir: &Path, deployment: &Deployment) -> Vec<PathBuf> {
  let mut stems = vec!["app".to_owned(), deployment.release_env.clone(), deployment.app_env.clone()];
  if let Some(region) = &deployment.region {
    stems.push(region.clone());
    stems.push(format!("{}-{}", deployment.app_env, region));
  }
  let mut seen = Vec::new();
  let mut paths = Vec::new();
  for stem in stems {
    if seen.contains(&stem) {
      continue;
    }
    for ext in ["toml", "yaml"] {
      let file = dir.join(format!("{stem}.{ext}"));
      if file.is_file() {
        paths.push(file);
      }
    }
    seen.push(stem);
  }
  paths
}

/// Where the configuration was found: the files c5store loads, in order, the directory they came from and the project root every path resolves against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
  pub sources: Vec<PathBuf>,
  pub dir: PathBuf,
  pub root: PathBuf,
}

impl Located {
  /// One more file, loaded after everything found so far. A relative path resolves against the configuration directory.
  pub fn extra(mut self, path: impl AsRef<Path>) -> Self {
    let path = path.as_ref();
    self.sources.push(if path.is_absolute() { path.to_path_buf() } else { self.dir.join(path) });
    self
  }
}

/// `locate_with` under the deployment read from the environment.
pub fn locate(path: &Path) -> Result<Located, HostError> {
  locate_with(path, &Deployment::from_env())
}

/// `path` is a project root holding `config/`, a `config/` directory, a directory holding `app.toml` or `app.yaml`, or one configuration file. A directory contributes the files `config_paths` names; a file is loaded alone. A file's project root is its directory, or the parent when that directory is named `config`.
pub fn locate_with(path: &Path, deployment: &Deployment) -> Result<Located, HostError> {
  if path.is_file() {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let root = if dir.file_name().is_some_and(|n| n == "config") { dir.parent().map(Path::to_path_buf).unwrap_or(dir.clone()) } else { dir.clone() };
    return Ok(Located { sources: vec![path.to_path_buf()], dir, root });
  }
  if !path.is_dir() {
    return Err(HostError::NoConfig(path.to_path_buf()));
  }
  let config_dir = path.join("config");
  if config_dir.is_dir() {
    return Ok(Located { sources: config_paths(&config_dir, deployment), dir: config_dir, root: path.to_path_buf() });
  }
  if path.file_name().is_some_and(|n| n == "config") {
    return Ok(Located { sources: config_paths(path, deployment), dir: path.to_path_buf(), root: path.parent().map(Path::to_path_buf).unwrap_or_default() });
  }
  if ["app.toml", "app.yaml"].iter().any(|name| path.join(name).is_file()) {
    return Ok(Located { sources: config_paths(path, deployment), dir: path.to_path_buf(), root: path.to_path_buf() });
  }
  Err(HostError::NoConfig(path.to_path_buf()))
}

impl Config {
  pub fn load(path: impl AsRef<Path>) -> Result<Self, HostError> {
    Self::load_located(locate(path.as_ref())?)
  }

  /// Loads the files `located` names, in that order, then `C5_` environment variables over them.
  pub fn load_located(located: Located) -> Result<Self, HostError> {
    if located.sources.is_empty() {
      return Err(HostError::NoConfig(located.root.clone()));
    }
    let (store, _mgr) = create_c5store(located.sources.clone(), Some(C5StoreOptions::default()))
      .map_err(|e| HostError::Config(located.sources[0].clone(), e.to_string()))?;
    Self::from_store(&store, located)
  }

  /// Reads the sections out of a store, which may have been loaded by the
  /// binary for its own reasons, and infers what the file left out.
  pub fn from_store<S: C5Store>(store: &S, located: Located) -> Result<Self, HostError> {
    let at = located.sources.first().cloned().unwrap_or_else(|| located.root.clone());
    let fail = |e: ConfigError| HostError::Config(at.clone(), e.to_string());

    for key in store.key_paths_with_prefix(None) {
      let head = key.split('.').next().unwrap_or(&key);
      if !SECTIONS.contains(&head) {
        return Err(HostError::Config(at.clone(), format!("unknown key `{key}`; sections are {}", SECTIONS.join(", "))));
      }
    }

    fn section<S: C5Store, T: for<'de> Deserialize<'de> + Default>(store: &S, key: &str) -> Result<T, ConfigError> {
      if !store.path_exists(key) {
        return Ok(T::default());
      }
      store.get_into_struct::<T>(key)
    }

    let app_section: AppSection = section(store, "app").map_err(fail)?;
    let server: ServerConfig = section(store, "server").map_err(fail)?;
    let document: DocumentConfig = section(store, "document").map_err(fail)?;
    if !store.path_exists("session") {
      return Err(HostError::Config(at.clone(), "missing section `session`; `session.key` is required".to_owned()));
    }
    let session: SessionSection = store.get_into_struct("session").map_err(fail)?;
    let cache: Option<CacheSection> = if store.path_exists("cache") { Some(store.get_into_struct("cache").map_err(fail)?) } else { None };

    let mut clients = BTreeMap::new();
    let mut names: Vec<String> = store
      .key_paths_with_prefix(Some("clients"))
      .into_iter()
      .filter_map(|k| k.strip_prefix("clients.").map(|rest| rest.split('.').next().unwrap_or(rest).to_owned()))
      .collect();
    if let Some(c5store::value::C5DataValue::Map(map)) = store.get("clients") {
      names.extend(map.keys().cloned());
    }
    names.sort();
    names.dedup();
    for name in names {
      let client: ClientConfig = store.get_into_struct(&format!("clients.{name}")).map_err(fail)?;
      clients.insert(name, client);
    }

    let mut statics: Vec<StaticRoot> = match store.get("static") {
      Some(value) => {
        let json = to_json(&value);
        serde_json::from_value(json).map_err(|e| HostError::Config(at.clone(), format!("static: {e}")))?
      }
      None => Vec::new(),
    };

    let locales: Option<LocalesSection> = if store.path_exists("locales") || !store.key_paths_with_prefix(Some("locales")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["supported", "default", "order", "remember", "cookie"] {
        if let Some(value) = store.get(&format!("locales.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      Some(serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("locales: {e}")))?)
    } else {
      None
    };

    let auth: Option<AuthSection> = if store.path_exists("auth") || !store.key_paths_with_prefix(Some("auth")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["provider", "login", "users"] {
        if let Some(value) = store.get(&format!("auth.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: AuthSection = serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("auth: {e}")))?;
      if !PROVIDERS.contains(&section.provider.as_str()) {
        return Err(HostError::Config(at.clone(), format!("auth.provider `{}` is not a provider; the providers are {}", section.provider, PROVIDERS.join(", "))));
      }
      if !section.login.starts_with('/') {
        return Err(HostError::Config(at.clone(), format!("auth.login `{}` must be a path", section.login)));
      }
      Some(section)
    } else {
      None
    };

    let root = located.root.clone();
    let app = root.join(&app_section.dir);
    let mut inferred = Vec::new();

    let mut document = document;
    if let Some(facts) = build_facts(&app) {
      if let Some(public_path) = facts.public_path {
        let route = public_path.trim_end_matches('/').to_owned();
        if !statics.iter().any(|s| s.route == route) {
          statics.push(StaticRoot { route: route.clone(), dir: "dist".to_owned() });
          inferred.push(format!("static {route} from dist/.snapfire-build.json"));
        }
        if document.entry.is_none() && facts.entries.iter().any(|e| e == "src/main.js") {
          document.entry = Some(format!("{}src/main.js", public_path));
          inferred.push("document.entry from dist/.snapfire-build.json".to_owned());
        }
      }
    }
    if document.import_map.is_none() && app.join("importmap.json").is_file() {
      document.import_map = Some("importmap.json".to_owned());
      inferred.push("document.import_map from importmap.json".to_owned());
    }
    if app.join("vendor").is_dir() && !statics.iter().any(|s| s.route == "/static/js/vendor") {
      statics.push(StaticRoot { route: "/static/js/vendor".to_owned(), dir: "vendor".to_owned() });
      inferred.push("static /static/js/vendor from vendor/".to_owned());
    }
    if app.join("styles").is_dir() {
      if !statics.iter().any(|s| s.route == "/static/css") {
        statics.push(StaticRoot { route: "/static/css".to_owned(), dir: "styles".to_owned() });
        inferred.push("static /static/css from styles/".to_owned());
      }
      if document.styles.is_none() {
        let mut sheets: Vec<String> = std::fs::read_dir(app.join("styles"))
          .map(|entries| {
            entries
              .filter_map(|e| e.ok())
              .map(|e| e.path())
              .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "css"))
              .filter_map(|p| p.file_name().map(|n| format!("/static/css/{}", n.to_string_lossy())))
              .collect()
          })
          .unwrap_or_default();
        sheets.sort();
        if !sheets.is_empty() {
          document.styles = Some(sheets);
          inferred.push("document.styles from styles/*.css".to_owned());
        }
      }
    }
    for (name, client) in clients.iter_mut() {
      if client.document.is_none() {
        let openapi = format!("clients/{name}.openapi.json");
        let proto = format!("clients/{name}.proto");
        client.document = Some(if !app.join(&openapi).is_file() && app.join(&proto).is_file() { proto } else { openapi });
        inferred.push(format!("clients.{name}.document from clients/"));
      }
    }

    Ok(Self { root, app, sources: located.sources, server, document, session, cache, clients, statics, locales, auth, inferred })
  }

  /// A path from the file, against the app directory.
  pub fn resolve(&self, relative: &str) -> PathBuf {
    self.app.join(relative)
  }

  /// The directory the first configuration file came from, which is where
  /// `auth.users` resolves; the project root when nothing was loaded.
  pub fn config_dir(&self) -> PathBuf {
    self.sources.first().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_else(|| self.root.clone())
  }

  pub fn session_ttl(&self) -> Result<Duration, HostError> {
    parse_duration(&self.session.ttl).ok_or_else(|| HostError::Value("session.ttl".to_owned(), self.session.ttl.clone()))
  }

  /// Whether development conveniences are on: `server.dev` when written,
  /// else whether `RELEASE_ENV` is `development`, which it is when unset.
  pub fn dev(&self) -> bool {
    self.server.dev.unwrap_or_else(|| Deployment::from_env().release_env == "development")
  }

  /// The cache lifetime, `None` when no `[cache]` section is written.
  pub fn cache_ttl(&self) -> Result<Option<Duration>, HostError> {
    let Some(cache) = &self.cache else { return Ok(None) };
    parse_duration(&cache.ttl).map(Some).ok_or_else(|| HostError::Value("cache.ttl".to_owned(), cache.ttl.clone()))
  }
}

#[derive(Deserialize, Default)]
struct BuildFacts {
  #[serde(default, rename = "publicPath")]
  public_path: Option<String>,
  #[serde(default)]
  entries: Vec<String>,
}

fn build_facts(app: &Path) -> Option<BuildFacts> {
  let text = std::fs::read_to_string(app.join("dist/.snapfire-build.json")).ok()?;
  serde_json::from_str(&text).ok()
}

fn to_json(value: &c5store::value::C5DataValue) -> serde_json::Value {
  use c5store::value::C5DataValue as V;
  match value {
    V::Null => serde_json::Value::Null,
    V::Boolean(b) => serde_json::Value::Bool(*b),
    V::Integer(i) => serde_json::Value::from(*i),
    V::UInteger(u) => serde_json::Value::from(*u),
    V::Float(f) => serde_json::Value::from(*f),
    V::String(s) => serde_json::Value::String(s.clone()),
    V::Bytes(b) => serde_json::Value::String(String::from_utf8_lossy(b).into_owned()),
    V::Array(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
    V::Map(map) => serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
  }
}

/// `30s`, `15m`, `8h`, `2d`, or a bare number of seconds.
pub fn parse_duration(raw: &str) -> Option<Duration> {
  let raw = raw.trim();
  let (digits, unit) = raw.split_at(raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len()));
  let n: u64 = digits.parse().ok()?;
  let seconds = match unit.trim() {
    "" | "s" => n,
    "m" => n * 60,
    "h" => n * 3600,
    "d" => n * 86_400,
    _ => return None,
  };
  Some(Duration::from_secs(seconds))
}
