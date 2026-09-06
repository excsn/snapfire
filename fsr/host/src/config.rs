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

use snapfire_fsr_runtime::{HeadEl, Meta};

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
  /// The application as a site: its name and the prefix its routes sit
  /// under, both fixed at build. Absent, the application is whole.
  pub site: Option<SiteSection>,
  /// The sites this application mounts as their shell; absent means none.
  pub sites: Option<SitesSection>,
  /// Which settings were inferred rather than written, for the report.
  pub inferred: Vec<String>,
}

/// `[sites]`: `root` is where `name@version` artifacts resolve, `poll` how
/// often the table is reread, and one `[sites.<name>]` per mounted site.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SitesSection {
  pub root: Option<String>,
  pub poll: Option<String>,
  pub mounts: BTreeMap<String, MountConfig>,
}

/// `[sites.<name>]`: `artifact` is `name@version` under the root or a path,
/// `hash` the content hash the mount is pinned to and `allow_engine` whether
/// an artifact carrying engine-owned rows may be mounted.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MountConfig {
  pub artifact: String,
  #[serde(default)]
  pub hash: Option<String>,
  #[serde(default)]
  pub allow_engine: bool,
}

/// `[site]`: `name` prefixes every id the build emits, `<name>:`, and `at` is
/// the path every route and link is written under.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SiteSection {
  pub name: String,
  pub at: String,
  /// What the site was built against: a path to the shell's `generated/shell.json` or a hand-written one.
  #[serde(default)]
  pub shell: Option<String>,
}

impl SiteSection {
  /// `<name>:`, the prefix on every id.
  pub fn prefix(&self) -> String {
    format!("{}:", self.name)
  }

  /// `at` joined with a path: `/` is `at` itself.
  pub fn under(&self, path: &str) -> String {
    let at = self.at.trim_end_matches('/');
    if path == "/" {
      at.to_owned()
    } else {
      format!("{at}{path}")
    }
  }
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
  /// Head elements every document carries, which a segment's `meta`
  /// overrides one identity at a time. Each table names a `tag` and its
  /// attributes; `children` is the element's text when it takes any.
  #[serde(default)]
  pub head: Vec<BTreeMap<String, String>>,
  #[serde(default = "default_shell")]
  pub shell: String,
}

impl DocumentConfig {
  /// The configured head as the outermost `Meta`, the layer every segment
  /// folds over.
  pub fn head_meta(&self) -> Result<Meta, HostError> {
    let mut head = Vec::new();
    for table in &self.head {
      let Some(tag) = table.get("tag") else {
        return Err(HostError::Value("document.head".to_owned(), "each entry needs a `tag`".to_owned()));
      };
      let mut attrs = Vec::new();
      let mut children = None;
      for (key, value) in table {
        match key.as_str() {
          "tag" => {}
          "children" => children = Some(value.clone()),
          _ => attrs.push((key.clone(), value.clone())),
        }
      }
      head.push(HeadEl { tag: tag.clone(), attrs, children });
    }
    Ok(Meta { title: None, description: None, head })
  }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSection {
  /// The cookie signing key. Required, so a deployment never runs on a default.
  pub key: String,
  /// `memory`, or `service` with `client` naming the `[clients.<name>]`
  /// entry whose contract declares `getSession`, `putSession` and
  /// `deleteSession`.
  #[serde(default = "default_store")]
  pub store: String,
  #[serde(default)]
  pub client: Option<String>,
  #[serde(default = "default_ttl")]
  pub ttl: String,
  #[serde(default = "default_capacity")]
  pub capacity: u64,
  #[serde(default)]
  pub secure: bool,
  /// When the host mints the session's CSRF token into requests: `identified`,
  /// once the session has an identity, or `always`. A token joins the render
  /// memo key, so `always` memoises every page per session.
  #[serde(default = "default_csrf")]
  pub csrf: String,
}

fn default_csrf() -> String {
  "identified".to_owned()
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
  /// The data cache over every method whose contract declares `cache`;
  /// absent, no method is cached whatever the contract says.
  #[serde(default)]
  pub data: Option<DataCacheSection>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataCacheSection {
  /// Entries per policy; `cache.capacity` when absent.
  #[serde(default)]
  pub capacity: Option<u64>,
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
  /// Required unless `transport` is `mock`.
  #[serde(default)]
  pub base_url: Option<String>,
  /// `mock` answers from `responses` and reaches nothing; absent, the
  /// document decides between HTTP and gRPC.
  #[serde(default)]
  pub transport: Option<String>,
  /// The recorded responses a mock client answers with, relative to the app
  /// directory; `clients/<name>.mock.json` when absent.
  #[serde(default)]
  pub responses: Option<String>,
  /// Which custody entry goes out as a bearer token on this client's calls:
  /// `true` for `access_token`, a string for another key. Absent, the
  /// client carries no token.
  #[serde(default)]
  pub bearer: Option<BearerKey>,
}

impl ClientConfig {
  pub fn is_mock(&self) -> bool {
    self.transport.as_deref() == Some("mock")
  }

  /// Where the mock's responses are read from, relative to the app directory.
  pub fn responses_file(&self, name: &str) -> String {
    self.responses.clone().unwrap_or_else(|| format!("clients/{name}.mock.json"))
  }
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
  /// The `[clients.<name>]` entry a `service` provider sends `authenticate` to.
  #[serde(default)]
  pub client: Option<String>,
}

pub const PROVIDERS: &[&str] = &["file", "service"];

fn default_login() -> String {
  "/login".to_owned()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StaticRoot {
  pub route: String,
  pub dir: String,
}

const SECTIONS: &[&str] = &["app", "server", "document", "session", "cache", "clients", "static", "locales", "auth", "site", "sites"];

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
    if !matches!(session.csrf.as_str(), "identified" | "always") {
      return Err(HostError::Config(at.clone(), format!("session.csrf `{}` is not a choice; identified or always", session.csrf)));
    }
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
      match client.transport.as_deref() {
        None | Some("mock") => {}
        Some(other) => return Err(HostError::Value(format!("clients.{name}.transport"), other.to_owned())),
      }
      if client.base_url.is_none() && !client.is_mock() {
        return Err(HostError::Config(at.clone(), format!("clients.{name}.base_url is required unless transport = \"mock\"")));
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
      for key in ["provider", "login", "users", "client"] {
        if let Some(value) = store.get(&format!("auth.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: AuthSection = serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("auth: {e}")))?;
      if !PROVIDERS.contains(&section.provider.as_str()) {
        return Err(HostError::Config(at.clone(), format!("auth.provider `{}` is not a provider; the providers are {}", section.provider, PROVIDERS.join(", "))));
      }
      if section.provider == "service" {
        match &section.client {
          Some(client) if clients.contains_key(client) => {}
          Some(client) => return Err(HostError::Config(at.clone(), format!("auth.client names `{client}`, which is not a [clients] entry"))),
          None => return Err(HostError::Config(at.clone(), "auth.provider = \"service\" needs auth.client".to_owned())),
        }
      }
      if !section.login.starts_with('/') {
        return Err(HostError::Config(at.clone(), format!("auth.login `{}` must be a path", section.login)));
      }
      Some(section)
    } else {
      None
    };
    if session.store == "service" {
      match &session.client {
        Some(client) if clients.contains_key(client) => {}
        Some(client) => return Err(HostError::Config(at.clone(), format!("session.client names `{client}`, which is not a [clients] entry"))),
        None => return Err(HostError::Config(at.clone(), "session.store = \"service\" needs session.client".to_owned())),
      }
    }

    let site: Option<SiteSection> = if store.path_exists("site") || !store.key_paths_with_prefix(Some("site")).is_empty() {
      let mut json = serde_json::Map::new();
      for key in ["name", "at", "shell"] {
        if let Some(value) = store.get(&format!("site.{key}")) {
          json.insert(key.to_owned(), to_json(&value));
        }
      }
      let section: SiteSection = serde_json::from_value(serde_json::Value::Object(json)).map_err(|e| HostError::Config(at.clone(), format!("site: {e}")))?;
      if section.name.is_empty() || !section.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err(HostError::Config(at.clone(), format!("site.name `{}` must be lowercase letters, digits, `_` or `-`", section.name)));
      }
      if !section.at.starts_with('/') || section.at.len() < 2 || section.at.ends_with('/') || section.at.contains('{') {
        return Err(HostError::Config(at.clone(), format!("site.at `{}` must be a path such as `/billing`, with no trailing slash", section.at)));
      }
      Some(section)
    } else {
      None
    };

    let sites: Option<SitesSection> = if store.path_exists("sites") || !store.key_paths_with_prefix(Some("sites")).is_empty() {
      let scalar = |key: &str| -> Result<Option<String>, HostError> {
        match store.get(&format!("sites.{key}")) {
          Some(value) => match to_json(&value) {
            serde_json::Value::String(s) => Ok(Some(s)),
            other => Err(HostError::Config(at.clone(), format!("sites.{key} must be a string, found {other}"))),
          },
          None => Ok(None),
        }
      };
      let root = scalar("root")?;
      let poll = scalar("poll")?;
      if let Some(poll) = &poll {
        if parse_duration(poll).is_none() {
          return Err(HostError::Config(at.clone(), format!("sites.poll `{poll}` is not a duration")));
        }
      }
      let mut names: Vec<String> = store
        .key_paths_with_prefix(Some("sites"))
        .into_iter()
        .filter_map(|k| k.strip_prefix("sites.").map(|rest| rest.split('.').next().unwrap_or(rest).to_owned()))
        .filter(|name| name != "root" && name != "poll")
        .collect();
      if let Some(c5store::value::C5DataValue::Map(map)) = store.get("sites") {
        names.extend(map.keys().filter(|k| *k != "root" && *k != "poll").cloned());
      }
      names.sort();
      names.dedup();
      let mut mounts = BTreeMap::new();
      for name in names {
        let mount: MountConfig = store.get_into_struct(&format!("sites.{name}")).map_err(fail)?;
        if mount.artifact.contains('@') && !mount.artifact.contains('/') && root.is_none() {
          return Err(HostError::Config(at.clone(), format!("sites.{name}.artifact `{}` names a version, which needs sites.root", mount.artifact)));
        }
        mounts.insert(name, mount);
      }
      Some(SitesSection { root, poll, mounts })
    } else {
      None
    };
    if site.is_some() && sites.is_some() {
      return Err(HostError::Config(at.clone(), "a site cannot mount sites; drop [sites] or [site]".to_owned()));
    }

    let root = located.root.clone();
    let app = root.join(&app_section.dir);
    let mut inferred = Vec::new();
    let css_route = match &site {
      Some(site) => site.under("/static/css"),
      None => "/static/css".to_owned(),
    };

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
    let icons_route = match &site {
      Some(site) => site.under("/static/icons"),
      None => "/static/icons".to_owned(),
    };
    if app.join("icons").is_dir() {
      if !statics.iter().any(|s| s.route == icons_route) {
        statics.push(StaticRoot { route: icons_route.clone(), dir: "icons".to_owned() });
        inferred.push(format!("static {icons_route} from icons/"));
      }
      let held = |name: &str| app.join("icons").join(name).is_file();
      // Only what the file did not already state: an entry the application
      // wrote for the same rel and size wins over what the directory implies.
      let written: Vec<(Option<String>, Option<String>)> =
        document.head.iter().map(|t| (t.get("rel").cloned(), t.get("sizes").cloned())).collect();
      let mut linked = Vec::new();
      for (file, mut attrs) in [
        ("favicon.svg", vec![("rel", "icon"), ("type", "image/svg+xml")]),
        ("favicon-32x32.png", vec![("rel", "icon"), ("type", "image/png"), ("sizes", "32x32")]),
        ("favicon-16x16.png", vec![("rel", "icon"), ("type", "image/png"), ("sizes", "16x16")]),
        ("apple-touch-icon.png", vec![("rel", "apple-touch-icon"), ("sizes", "180x180")]),
        ("site.webmanifest", vec![("rel", "manifest")]),
      ] {
        let sizes = attrs.iter().find(|(k, _)| *k == "sizes").map(|(_, v)| (*v).to_owned());
        if !held(file) || written.iter().any(|(rel, held_sizes)| rel.as_deref() == Some(attrs[0].1) && *held_sizes == sizes) {
          continue;
        }
        attrs.push(("href", ""));
        let mut table: BTreeMap<String, String> = attrs.into_iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect();
        table.insert("tag".to_owned(), "link".to_owned());
        table.insert("href".to_owned(), format!("{icons_route}/{file}"));
        document.head.push(table);
        linked.push(file);
      }
      if !linked.is_empty() {
        inferred.push(format!("document.head links [{}] from icons/", linked.join(", ")));
      }
    }
    if app.join("styles").is_dir() {
      if !statics.iter().any(|s| s.route == css_route) {
        statics.push(StaticRoot { route: css_route.clone(), dir: "styles".to_owned() });
        inferred.push(format!("static {css_route} from styles/"));
      }
      if document.styles.is_none() {
        let mut sheets: Vec<String> = std::fs::read_dir(app.join("styles"))
          .map(|entries| {
            entries
              .filter_map(|e| e.ok())
              .map(|e| e.path())
              .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "css"))
              .filter_map(|p| p.file_name().map(|n| format!("{css_route}/{}", n.to_string_lossy())))
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

    Ok(Self { root, app, sources: located.sources, server, document, session, cache, clients, statics, locales, auth, site, sites, inferred })
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
  snapfire_fsr_core::parse_duration(raw)
}
