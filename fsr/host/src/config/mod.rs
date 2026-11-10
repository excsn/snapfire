//! `config/`, loaded through c5store in the order `config_paths` gives so
//! deployment overlays layer over `app.toml` and `C5_` environment variables
//! win, `.c5encval` values decrypted with the keys under `private_keys`, plus
//! what the host infers from the app directory so the file only carries
//! deployment facts. `Loader` is how one is read and what a reload rereads.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use c5store::error::ConfigError;
use c5store::secrets::{Base64SecretDecryptor, EciesX25519SecretDecryptor, SecretKeyStore};
use c5store::{C5Store, C5StoreOptions, create_c5store};
pub use c5store::SecretOptions;
use ecies_25519::EciesX25519;
use serde::Deserialize;

use snapfire_fsr_core::ext::Catalogs;
use snapfire_fsr_runtime::{HeadEl, Meta};

use crate::HostError;
use crate::locale::LocalesSection;

mod bundle;
mod load;
mod values;

use self::bundle::*;

/// The host's configuration after loading and inference. `root` is the
/// project directory, `app` the application directory under it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
  pub root: PathBuf,
  pub app: PathBuf,
  /// `[app] runs`.
  pub runs: Runs,
  pub sources: Vec<PathBuf>,
  pub server: ServerConfig,
  pub document: DocumentConfig,
  pub session: SessionSection,
  /// The render memo; absent means nothing is cached.
  pub cache: Option<CacheSection>,
  /// `[store]`: the application's say over the browser store's merge, defaults filled.
  pub store: StoreSection,
  pub clients: BTreeMap<String, ClientConfig>,
  pub statics: Vec<StaticRoot>,
  /// `[[route]]`: the patterns the application registers in Rust, so a reader
  /// of the configuration knows the deployment answers them before it boots.
  pub routes: Vec<RouteDecl>,
  /// The locales the application serves; absent means one, `en`, with no
  /// prefix, cookie or header consulted.
  pub locales: Option<LocalesSection>,
  /// The identity provider; absent means no login and no `/auth/` routes.
  pub auth: Option<AuthSection>,
  /// Which TypeScript `fsr` checks the application with; absent means the
  /// checker's own default version.
  pub typecheck: Option<TypecheckSection>,
  /// `[vendor]`: where `fsr add` fetches packages from; absent means esm.sh.
  pub vendor: Option<VendorSection>,
  /// `[build]`: how `fsr` builds the application; absent means the defaults.
  pub build: Option<BuildSection>,
  /// `[trace]`: which requests a client may read its own trace of; absent
  /// means none outside development.
  pub trace: Option<TraceSection>,
  /// `[crawl]`: the `robots.txt` and `sitemap.xml` the host answers; absent
  /// means neither.
  pub crawl: Option<CrawlSection>,
  /// `[time] zones`: where `Date`'s local time reads its zone rules when the build holds both sources; absent means the compiled database.
  pub time_zones: Option<TimeZones>,
  /// `[desktop]`: the native window `fsr dev --desktop` and `fsr serve
  /// --desktop` open; the host reads none of it.
  pub desktop: Option<DesktopSection>,
  /// `[pwa]`: the web app manifest and the service worker the host serves; absent means neither.
  pub pwa: Option<PwaSection>,
  /// `[files]`: the directories a body's `file` calls reach, relative to the application; none means `file` reaches nothing.
  pub files: FilesSection,
  /// `[mdx]`: how `fsr` reads `.mdx` and `.md` files, defaults filled.
  pub mdx: MdxSection,
  /// The application as a site: its name and the prefix its routes sit
  /// under, both fixed at build. Absent, the application is whole.
  pub site: Option<SiteSection>,
  /// The sites this application mounts as their shell; absent means none.
  pub sites: Option<SitesSection>,
  /// `[dirs]`: where each kind of asset lives, defaults filled.
  pub dirs: DirsSection,
  /// `[images]`: the variant policy and the sources, defaults filled.
  pub images: ImagesSection,
  /// `[media]`: video and audio, defaults filled.
  pub media: MediaSection,
  /// `[fonts]`: the families declared and where their files are served from.
  pub fonts: FontsSection,
  /// The shell's `vendor/`, beside the `generated/shell.json` that `[site]
  /// shell` names. A site's import map points its frameworks at the shell's
  /// URLs, so a site running alone serves this at `/static/js/vendor` to
  /// answer them. A mount, a deploy tree and a packed artifact never carry it.
  pub shell_vendor: Option<PathBuf>,
  /// The static path `/favicon.ico` answers from, `<icons route>/favicon.ico`,
  /// when the icons directory holds that file. A browser asks for the root
  /// path on any answer that is not a document, a plain 404 or a payload
  /// opened in a tab, and this stops that request logging a second 404.
  pub favicon: Option<String>,
  /// `[public]`: the deployment's own values, which a body reads as
  /// `ctx.config.<key>`. Scalars only, one level deep.
  pub public: BTreeMap<String, PublicValue>,
  /// The bundle's module graph, when its facts file was found; the input to
  /// `server.preload`.
  pub bundle: Option<Bundle>,
  /// Which settings were inferred rather than written, for the report.
  pub inferred: Vec<String>,
  /// Top-level keys the host does not own, left where they were for the
  /// application's own store to read. A project that shares `config/`
  /// between its store and the host's puts its keys on the same rungs.
  pub ignored: Vec<String>,
  /// The deployment whose ladder was loaded; `None` when the caller owned
  /// the store, when `dev` reads the environment.
  pub deployment: Option<Deployment>,
  /// Keys written as secrets this process could not decrypt, recorded rather
  /// than refused when the loader was asked to keep them (`Loader::keep_undecrypted`).
  pub undecrypted: Vec<String>,
  /// `[secrets.<name>]`: what the application's clients send as credentials.
  pub secrets: BTreeMap<String, SecretEntry>,
}

/// One `[public]` value: a TOML scalar, kept as written so the build can type
/// it and the host can hand it to a body.
#[derive(Debug, Clone, PartialEq)]
pub enum PublicValue {
  Str(String),
  Int(i64),
  Float(f64),
  Bool(bool),
}

impl PublicValue {
  /// The TypeScript type a body sees the value as.
  pub fn ts(&self) -> &'static str {
    match self {
      PublicValue::Str(_) => "string",
      PublicValue::Int(_) => "bigint",
      PublicValue::Float(_) => "number",
      PublicValue::Bool(_) => "boolean",
    }
  }

  pub fn to_value(&self) -> snapfire_fsr_core::Value {
    use snapfire_fsr_core::Value;
    match self {
      PublicValue::Str(s) => Value::str(s.clone()),
      PublicValue::Int(i) => Value::int(*i),
      PublicValue::Float(f) => Value::F64(*f),
      PublicValue::Bool(b) => Value::Bool(*b),
    }
  }
}

impl std::fmt::Display for PublicValue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      PublicValue::Str(s) => write!(f, "{s:?}"),
      PublicValue::Int(i) => write!(f, "{i}"),
      PublicValue::Float(x) => write!(f, "{x}"),
      PublicValue::Bool(b) => write!(f, "{b}"),
    }
  }
}

/// `[dirs]`: where each kind of asset lives under the app directory. Every
/// key defaults to the conventional name, so an application that writes
/// nothing changes nothing. The routes stay fixed: `/static/css`,
/// `/static/icons` and the bundle's public path answer whatever directory
/// a key names.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DirsSection {
  #[serde(default = "default_styles_dir")]
  pub styles: String,
  #[serde(default = "default_fonts_dir")]
  pub fonts: String,
  #[serde(default = "default_images_dir")]
  pub images: String,
  #[serde(default = "default_icons_dir")]
  pub icons: String,
  /// Files served at the site root under the names they have, as a static
  /// root at `/` that a request no file answers falls through.
  #[serde(default = "default_public_dir")]
  pub public: String,
}

impl Default for DirsSection {
  fn default() -> Self {
    serde_json::from_str("{}").expect("every dirs key has a default")
  }
}

fn default_styles_dir() -> String {
  "styles".to_owned()
}
fn default_fonts_dir() -> String {
  "fonts".to_owned()
}
fn default_images_dir() -> String {
  "images".to_owned()
}
fn default_icons_dir() -> String {
  "icons".to_owned()
}
fn default_public_dir() -> String {
  "public".to_owned()
}

/// `[images]`: the variant policy every image is built under, whether a
/// plain `<img>` of an imported asset is rewritten, the base its URLs are
/// served from when a CDN answers them and the named sources a remote
/// image is addressed through.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ImagesSection {
  #[serde(default = "default_image_widths")]
  pub widths: Vec<u32>,
  #[serde(default = "default_image_formats")]
  pub formats: Vec<String>,
  /// Quality per format, `avif` and `webp`, 0 to 100. A format left out
  /// keeps its default.
  #[serde(default)]
  pub quality: BTreeMap<String, u8>,
  /// Whether an `<img>` whose `src` is an imported asset's is lowered as a
  /// `Picture`. Off, only `Picture` does anything.
  #[serde(default = "default_true")]
  pub rewrite: bool,
  /// Whether the served original of a JPEG, PNG, WebP or GIF loses its
  /// metadata: everything but the EXIF orientation, creator and copyright and
  /// the ICC profile. Off, the original is served as the author saved it.
  #[serde(default = "default_true")]
  pub strip: bool,
  /// `scheme://host/path` every emitted image URL is prefixed with, for a
  /// deployment whose static tree a CDN serves. No trailing slash.
  #[serde(default)]
  pub base: Option<String>,
  /// `[images.sources.<name>]`: a URL template a remote image is addressed
  /// through, `{src}` the value given and `{width}` each width of the
  /// policy. Format negotiation is the service's.
  #[serde(default)]
  pub sources: BTreeMap<String, ImageSource>,
}

/// `[media]`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MediaSection {
  /// Where an `<Embed>`'s still comes from: `provider`, the video host's own
  /// image, or `local`, fetched once at build and served from this
  /// application, so a page asks the provider for nothing until it plays and
  /// still shows it offline. A placeholder `local` holds no still for shows
  /// the title alone and the build report names it.
  #[serde(default)]
  pub embed_stills: EmbedStills,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbedStills {
  #[default]
  Provider,
  Local,
}

impl Default for MediaSection {
  fn default() -> Self {
    serde_json::from_str("{}").expect("every media key has a default")
  }
}

impl Default for ImagesSection {
  fn default() -> Self {
    serde_json::from_str("{}").expect("every images key has a default")
  }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ImageSource {
  pub template: String,
}

fn default_true() -> bool {
  true
}
fn default_image_widths() -> Vec<u32> {
  vec![640, 960, 1280, 1920, 2560]
}
fn default_image_formats() -> Vec<String> {
  vec!["avif".to_owned(), "webp".to_owned()]
}

/// `[fonts]`: `base` is where the font files are served from when a CDN
/// answers them, and one `[fonts.<key>]` per family the application declares.
/// A family the directory holds and no key names is inferred from the
/// files' own name tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FontsSection {
  pub base: Option<String>,
  pub faces: BTreeMap<String, FontEntry>,
}

/// One `[fonts.<key>]`. Every field is optional, though a key carries at
/// least one, since an empty table is not read: `family` alone names a
/// family the directory holds and gives it the variable `--font-<key>`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct FontEntry {
  /// The family name, when the files' own tables are not to be trusted or
  /// `remote` carries no file to read.
  #[serde(default)]
  pub family: Option<String>,
  /// The files under `[dirs] fonts` this key covers. Absent, every file
  /// whose family matches.
  #[serde(default)]
  pub files: Vec<String>,
  /// The system face the fallback is sized against: one `font::fallbacks`
  /// names. Absent, `Arial`.
  #[serde(default)]
  pub fallback: Option<String>,
  /// `font-display`; absent, `swap`.
  #[serde(default)]
  pub display: Option<String>,
  /// Whether the faces are preloaded. Absent, the regular weight only.
  #[serde(default)]
  pub preload: Option<bool>,
  /// A provider's own stylesheet URL. The build links it, preconnects to
  /// its file origin, widens the policy and skips the metrics and the
  /// preload, since there is no file to read.
  #[serde(default)]
  pub remote: Option<String>,
  /// The CSS variable the family is defined as; absent, `--font-<key>`.
  #[serde(default)]
  pub variable: Option<String>,
  /// The Unicode scripts a generated card image draws with these faces,
  /// named as Unicode names them: `["Han", "Hiragana", "Katakana"]`. A face
  /// with none is one of the others card text falls back through in order.
  #[serde(default)]
  pub scripts: Vec<String>,
}

/// `[sites]`: `root` is where `name@version` artifacts resolve, `poll` how
/// often the table is reread, `store` where a version the root lacks is
/// fetched from and one `[sites.<name>]` per mounted site.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SitesSection {
  pub root: Option<String>,
  pub poll: Option<String>,
  /// A directory of `<name>-<version>.tar.gz` archives, against the project
  /// root, or an `http://` or `https://` URL serving the same names.
  pub store: Option<String>,
  pub mounts: BTreeMap<String, MountConfig>,
}

/// `[sites.<name>]`: `artifact`, `name@version` under the root or a path,
/// with `hash` the content hash the mount is pinned to and `allow_engine`
/// whether an artifact carrying engine-owned rows may be mounted.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MountConfig {
  #[serde(default)]
  pub artifact: Option<String>,
  #[serde(default)]
  pub hash: Option<String>,
  #[serde(default)]
  pub allow_engine: bool,
}

impl MountConfig {
  pub fn new(artifact: Option<String>, hash: Option<String>, allow_engine: bool) -> Self {
    Self { artifact, hash, allow_engine }
  }
}

impl MountConfig {
  /// What the table refuses: a row naming no artifact.
  fn check(&self, name: &str) -> Result<(), String> {
    if self.artifact.is_none() {
      return Err(format!("sites.{name} needs artifact"));
    }
    Ok(())
  }
}

/// `[site]`: `name` prefixes every id the build emits, `<name>:` and `at` is
/// the path every route and link is written under.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SiteSection {
  pub name: String,
  pub at: String,
  /// What the site was built against: a path to the shell's `generated/shell.json` or a hand-written one.
  #[serde(default)]
  pub shell: Option<String>,
  /// Paths a shell renders against a candidate holding this site before the
  /// candidate swaps in: a path alone, anonymously, or `{ path, as }` signed
  /// in as that subject. A page that fails refuses the reload.
  #[serde(default)]
  pub probe: Vec<crate::Probe>,
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

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AppSection {
  #[serde(default = "default_app_dir")]
  pub dir: String,
  #[serde(default)]
  pub runs: Runs,
}

impl Default for AppSection {
  fn default() -> Self {
    Self { dir: default_app_dir(), runs: Runs::default() }
  }
}

/// `[app] runs`: `web` serves pages to a browser, a desktop window among them; `terminal` draws the root page in a terminal with FSR's Ink and serves nothing.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Runs {
  #[default]
  Web,
  Terminal,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ServerConfig {
  #[serde(default = "default_listen")]
  pub listen: String,
  #[serde(default = "default_plan")]
  pub plan: String,
  /// The directory of contract files `fsr build` writes, merged at boot in name order.
  #[serde(default = "default_contracts")]
  pub contracts: String,
  /// The most bytes a request body may carry; a larger one is answered 413
  /// before anything reads it. Default 1 MiB.
  #[serde(default = "default_max_body")]
  pub max_body: usize,
  /// The most bytes one uploaded part may carry, `0` for no cap beyond
  /// `max_body`. A part over it is refused before the body runs.
  #[serde(default)]
  pub max_upload: usize,
  /// The hosts this deployment answers on, which a body reads as `ctx.host`.
  /// Empty, the request's `Host` is never read and `ctx.host` is null. A
  /// header naming anything absent from this list does not match, so a client
  /// cannot put a host of its own into a body. Compared lowercased and whole,
  /// port included, so a deployment on a port lists the port.
  #[serde(default)]
  pub hosts: Vec<String>,
  /// The request headers a middleware reads as `request.headers`, by lowercase
  /// name. A header not listed never reaches a body, so a client cannot put a
  /// value of its own there unless the deployment asked for it.
  #[serde(default)]
  pub request_headers: Vec<String>,
  /// Where `prerender` writes and the host reads a route rendered once at
  /// build time, relative to the app directory. Absent, nothing is prerendered.
  #[serde(default)]
  pub prerender: Option<String>,
  /// Whether the document carries the live-refresh script and the host
  /// answers `/__fsr/events` and `/__fsr/changed`. Absent, it follows
  /// `RELEASE_ENV`: on when that is `development`, its default.
  #[serde(default)]
  pub dev: Option<bool>,
  /// No longer read: every page renders on the server and every island in
  /// the browser. Kept so that a configuration still naming it is refused
  /// with that rather than as an unknown key.
  #[serde(default)]
  pub render: Option<String>,
  /// How long a browser may reuse a file from a `[[static]]` root whose name
  /// carries no content hash without asking again, in seconds. Left out, it
  /// asks every time (`no-cache`), so a deploy never pairs fresh HTML with a
  /// module the browser kept from the one before; the answer is a 304 while
  /// the file is unchanged. A hashed name is `immutable` either way. `0` sends
  /// no `Cache-Control` at all. Ignored under `dev`, where every static
  /// answer is `no-cache`.
  #[serde(default)]
  pub static_max_age: Option<u64>,
  /// Whether `serve` negotiates HTTP/2 on the connection as well as HTTP/1.1.
  /// Without `[server.tls]` this is h2c: a client that opens with the HTTP/2
  /// preface is served and a browser, which wants ALPN over TLS, is not.
  #[serde(default)]
  pub http2: bool,
  /// How long, in seconds, `serve_listener` waits on SIGTERM or SIGINT for the connections it holds to finish before it returns. It stops accepting at once.
  #[serde(default = "default_shutdown_grace")]
  pub shutdown_grace: u64,
  /// Other origins a write may come from, `https://pay.example`, beside this site's own: a payment provider posting back. An OIDC method posting its callback (`form_post`) adds its issuer's origin itself.
  #[serde(default)]
  pub trusted_origins: Vec<String>,
  /// The path a liveness probe asks, answered 200 while the process serves. Empty turns it off.
  #[serde(default = "default_health")]
  pub health: String,
  /// The path a readiness probe asks, answered 200 while serving and 503 once a shutdown has begun. Empty turns it off.
  #[serde(default = "default_ready")]
  pub ready: String,
  /// `[server.tls]`: absent, the listener is plain TCP. Present, the host
  /// needs the `tls` feature, which is off by default.
  #[serde(default)]
  pub tls: Option<TlsSection>,
}

/// `[server.tls]`: where the certificate and its key are read from, what the
/// handshake offers and which signal re-reads them.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TlsSection {
  /// The directory `cert` and `key` are read under; absent, they resolve
  /// against the project root like every other configured path.
  #[serde(default)]
  pub dir: Option<String>,
  /// The certificate chain in PEM, leaf first.
  #[serde(default = "default_cert")]
  pub cert: String,
  /// The private key in PEM: PKCS#8, PKCS#1 or SEC1.
  #[serde(default = "default_key")]
  pub key: String,
  /// What the handshake offers, in order of preference. Absent, `h2` and
  /// `http/1.1` when `server.http2` is on and `http/1.1` alone when it is not.
  #[serde(default)]
  pub alpn: Option<Vec<String>>,
  /// The signal that re-reads the files and swaps the certificate:
  /// `hup`, `usr1`, `usr2` or `none`. Unix only; elsewhere a restart.
  #[serde(default = "default_reload")]
  pub reload: String,
}

impl TlsSection {
  /// The certificate and key paths, resolved under `dir` when it is set and
  /// against `root` otherwise. An absolute path is taken as written.
  pub fn files(&self, root: &Path) -> (PathBuf, PathBuf) {
    let under = |name: &str| -> PathBuf {
      let path = Path::new(name);
      if path.is_absolute() {
        return path.to_path_buf();
      }
      match &self.dir {
        Some(dir) if Path::new(dir).is_absolute() => Path::new(dir).join(path),
        Some(dir) => root.join(dir).join(path),
        None => root.join(path),
      }
    };
    (under(&self.cert), under(&self.key))
  }
}

impl Default for ServerConfig {
  fn default() -> Self {
    Self {
      listen: default_listen(),
      plan: default_plan(),
      contracts: default_contracts(),
      max_body: default_max_body(),
      max_upload: 0,
      hosts: Vec::new(),
      request_headers: Vec::new(),
      prerender: None,
      dev: None,
      render: None,
      static_max_age: None,
      http2: false,
      shutdown_grace: default_shutdown_grace(),
      trusted_origins: Vec::new(),
      health: default_health(),
      ready: default_ready(),
      tls: None,
    }
  }
}

/// A Content-Security-Policy as directives and their sources, which is what
/// lets the host add a source of its own without the policy having left a hole
/// for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Csp(pub BTreeMap<String, Vec<String>>);

impl Csp {
  /// Adds `source` to `directive`, once. The host calls this for what only it
  /// knows: the inline import map's hash, plus in a development host the nonce
  /// on its own refresh script.
  pub fn add(&mut self, directive: &str, source: impl Into<String>) {
    let sources = self.0.entry(directive.to_owned()).or_default();
    let source = source.into();
    if !sources.contains(&source) {
      sources.push(source);
    }
  }

  /// Adds `source` to `directive` without narrowing it: a directive the policy
  /// leaves to `default-src` starts from `default-src`'s sources, and a policy
  /// naming neither restricts nothing, so it is left alone.
  pub fn widen(&mut self, directive: &str, source: impl Into<String>) {
    if !self.0.contains_key(directive) {
      let Some(fallback) = self.0.get("default-src").cloned() else {
        return;
      };
      self.0.insert(directive.to_owned(), fallback);
    }
    self.add(directive, source);
  }

  /// `widen` with the hash or nonce of inline content, unless `directive` already allows `'unsafe-inline'`: a browser ignores `'unsafe-inline'` beside a hash or a nonce, so adding one would block every other inline script or style the policy let through.
  pub fn admit_inline(&mut self, directive: &str, hash: impl Into<String>) {
    let effective = self.0.get(directive).or_else(|| self.0.get("default-src"));
    if effective.is_some_and(|sources| sources.iter().any(|s| s == "'unsafe-inline'")) {
      return;
    }
    self.widen(directive, hash);
  }

  /// Whether the policy has `directive` at all.
  pub fn declares(&self, directive: &str) -> bool {
    self.0.contains_key(directive)
  }

  /// Whether `directive` names `source` already.
  pub fn names(&self, directive: &str, source: &str) -> bool {
    self.0.get(directive).is_some_and(|s| s.iter().any(|v| v == source))
  }

  /// The header value: `directive source source; directive source`. A
  /// directive with no sources is written bare, which is what `upgrade-insecure-requests`
  /// wants. Empty overall is `None`, since a policy naming nothing forbids everything.
  pub fn header(&self) -> Option<String> {
    if self.0.is_empty() {
      return None;
    }
    let mut parts = Vec::with_capacity(self.0.len());
    for (directive, sources) in &self.0 {
      match sources.is_empty() {
        true => parts.push(directive.clone()),
        false => parts.push(format!("{directive} {}", sources.join(" "))),
      }
    }
    Some(parts.join("; "))
  }
}

/// Which build of the embedded client `/static/js/fsr` answers with. The
/// minified modules are about a third smaller and import their siblings by
/// `.min.js`, which the host answers either way, so this picks what the entry
/// point gets and the graph follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ClientBuild {
  /// Minified unless `server.dev` is on. The default.
  #[default]
  Auto,
  /// The readable build, whatever `server.dev` says.
  Readable,
  /// The minified build, whatever `server.dev` says.
  Minified,
}

impl ClientBuild {
  /// Whether to serve the minified build, given whether this host is a
  /// development one. Always minified in a binary built without the readable
  /// build, since there is nothing else to answer with.
  pub fn minified(self, dev: bool) -> bool {
    if !crate::client::HAS_READABLE {
      return true;
    }
    match self {
      Self::Auto => !dev,
      Self::Readable => false,
      Self::Minified => true,
    }
  }
}

/// The document shell. `entry`, `import_map` and `styles` are inferred when absent.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DocumentConfig {
  /// The document's title, which a route's `meta` overrides.
  #[serde(default)]
  pub title: Text,
  /// How a route's title is written into the document, `"{page} · {title}"`:
  /// `{page}` is the route's title and `{title}` this document's. Absent, a
  /// route's title is written as given. A `meta` returning `absolute: true`
  /// is written as given either way.
  #[serde(default)]
  pub title_template: Option<Text>,
  #[serde(default)]
  pub entry: Option<String>,
  #[serde(default)]
  pub import_map: Option<String>,
  /// Whether the head carries a `modulepreload` link for every module the page
  /// loads before the first island can mount: `entry`'s own static imports,
  /// the import map's targets and whatever those reach. A module an island
  /// loads through `import()` is left out, since which islands a document
  /// holds is not known until it renders.
  #[serde(default)]
  pub module_preload: bool,
  /// Which build of the embedded client the page loads, `auto`, `readable` or
  /// `minified`. `auto` follows `server.dev`.
  #[serde(default)]
  pub client: ClientBuild,
  /// `[document.csp]`, one directive per key and its sources as a list. The
  /// host merges its own sources into the directives it owns, so a policy never
  /// has to name what the host emitted. Absent, no policy is sent and whatever
  /// sits in front of the host owns the header.
  #[serde(default)]
  pub csp: Option<Csp>,
  /// `[document.csp_report_only]`, the same shape, sent as
  /// `Content-Security-Policy-Report-Only`, which a browser reports against and
  /// never enforces. A policy for a page carrying third-party scripts belongs
  /// here first: an ad tag injects scripts from origins no allowlist can name
  /// ahead of time. A hash in `script-src` also makes the browser ignore
  /// `'unsafe-inline'` in that same directive.
  #[serde(default)]
  pub csp_report_only: Option<Csp>,
  /// Stylesheet URLs linked from the head, in order.
  #[serde(default)]
  pub styles: Option<Vec<String>>,
  /// Head elements every document carries, which a segment's `meta`
  /// overrides one identity at a time: what the host inferred from the icons
  /// it serves. Not a configuration key; the root layout's `meta` is where an
  /// application writes its own.
  #[serde(skip)]
  pub head: Vec<BTreeMap<String, String>>,
  #[serde(default = "default_shell")]
  pub shell: String,
  /// Whether the head carries `<link rel="icon" href="data:,">` when the
  /// application serves no icon of its own, so a browser does not ask for
  /// `/favicon.ico` and log the 404. On unless set to `false`. An icon from
  /// `icons/` or a layout's plain `rel="icon"` replaces it.
  #[serde(default = "default_empty_icon")]
  pub empty_icon: bool,
  /// `scheme://host` this deployment is reached at, with no trailing slash.
  /// A crawler reads `rel=canonical` and `rel=alternate` as absolute URLs
  /// only, so the host prefixes a path on either with this. Absent, both are
  /// written as the application wrote them.
  #[serde(default)]
  pub origin: Option<String>,
}

impl Default for DocumentConfig {
  fn default() -> Self {
    serde_json::from_str("{}").expect("every document key has a default")
  }
}

impl DocumentConfig {
  /// The configured head as the outermost `Meta`, the layer every segment
  /// folds over.
  pub fn head_meta(&self) -> Result<Meta, HostError> {
    let mut head = Vec::new();
    for table in &self.head {
      let Some(tag) = table.get("tag") else {
        return Err(HostError::Value(
          "document.head".to_owned(),
          "each entry needs a `tag`".to_owned(),
        ));
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
      head.push(HeadEl::new(tag.clone(), attrs, children));
    }
    Ok(Meta::default().head(head))
  }
}

/// Text the document carries: a literal, or a message key read from the
/// catalog of the request's locale with the literal it falls back to.
/// `title = "Shop"` or `title = { key = "document.title", default = "Shop" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
  Literal(String),
  Keyed { key: String, default: String },
}

impl Default for Text {
  fn default() -> Self {
    Text::Literal(String::new())
  }
}

impl Text {
  /// The literal: the text itself, or what a key falls back to.
  pub fn literal(&self) -> &str {
    match self {
      Text::Literal(text) => text,
      Text::Keyed { default, .. } => default,
    }
  }

  pub fn is_keyed(&self) -> bool {
    matches!(self, Text::Keyed { .. })
  }

  /// The text under `locale`: a key read from `catalogs`, which answers from
  /// the default locale's table when `locale` has none, else the literal.
  pub fn resolve(&self, catalogs: &Catalogs, locale: &str) -> String {
    match self {
      Text::Literal(text) => text.clone(),
      Text::Keyed { key, default } => catalogs.lookup(locale, key).map(str::to_owned).unwrap_or_else(|| default.clone()),
    }
  }
}

impl schemars::JsonSchema for Text {
  fn schema_name() -> std::borrow::Cow<'static, str> {
    "Text".into()
  }

  fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
      "description": "A literal; a table names a message key read from the catalog of the request's locale and the literal it falls back to.",
      "anyOf": [
        { "type": "string" },
        {
          "type": "object",
          "properties": { "key": { "type": "string" }, "default": { "type": "string" } },
          "required": ["key"],
          "additionalProperties": false
        }
      ]
    })
  }
}

impl<'de> Deserialize<'de> for Text {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
      type Value = Text;

      fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a string or a table with `key` and `default`")
      }

      fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Text, E> {
        Ok(Text::Literal(text.to_owned()))
      }

      fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Text, A::Error> {
        let mut key = None;
        let mut default = None;
        while let Some(field) = map.next_key::<String>()? {
          match field.as_str() {
            "key" => key = Some(map.next_value::<String>()?),
            "default" => default = Some(map.next_value::<String>()?),
            other => return Err(serde::de::Error::unknown_field(other, &["key", "default"])),
          }
        }
        let key = key.ok_or_else(|| serde::de::Error::missing_field("key"))?;
        Ok(Text::Keyed { key, default: default.unwrap_or_default() })
      }
    }

    deserializer.deserialize_any(Visitor)
  }
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SessionSection {
  /// The cookie signing key, from the environment or a secret in a deployment. Absent, a
  /// development host signs with a key it generates at boot and any other host that builds its own
  /// key ring refuses to start.
  #[serde(default)]
  pub key: Option<String>,
  /// Keys that still verify a cookie signed before `key` replaced them, in
  /// order; a rotation moves the old key here and removes it once the ttl
  /// has passed. A reload may change both.
  #[serde(default)]
  pub previous_keys: Vec<String>,
  /// `memory`, `cookie` (the record sealed into the cookie under the
  /// session keys) or `service` with `client` naming the `[clients.<name>]`
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
  /// Whether a CSRF token stands beside the origin check every write passes: `origin`,
  /// the default, mints none; `identified` mints and verifies one once the session has an
  /// identity; `always` for any session whose page renders it. Under `always` a fresh
  /// session is established by the response that rendered a token and by no other. A
  /// token joins the memo key of a subtree that renders it.
  #[serde(default = "default_csrf")]
  pub csrf: String,
  /// How a token is made and checked: `single_use`, a fresh token per form
  /// good for one post; `session`, one random token per session, replaced at
  /// sign-in; `derived`, an hmac of the session id, stable for its life.
  #[serde(default = "default_csrf_scheme")]
  pub csrf_scheme: String,
  /// Under `single_use`, how many minted tokens stay valid at once, one per
  /// open form; the oldest is dropped as newer ones are minted.
  #[serde(default = "default_csrf_outstanding")]
  pub csrf_outstanding: u32,
  /// The most one session may hold, about, in bytes. A write that would take it over is refused.
  #[serde(default = "default_session_max_bytes")]
  pub max_bytes: u64,
}

fn default_session_max_bytes() -> u64 {
  snapfire_fsr_session::DEFAULT_MAX_BYTES as u64
}

/// The `session.key` the `fsr new` scaffold writes into `config/production.toml`. A host refuses to
/// start while the key holds it.
pub const SESSION_KEY_PLACEHOLDER: &str = "replace-this-with-a-generated-key";

impl Default for SessionSection {
  fn default() -> Self {
    Self {
      key: None,
      previous_keys: Vec::new(),
      store: default_store(),
      client: None,
      ttl: default_ttl(),
      capacity: default_capacity(),
      secure: false,
      csrf: default_csrf(),
      csrf_scheme: default_csrf_scheme(),
      csrf_outstanding: default_csrf_outstanding(),
      max_bytes: default_session_max_bytes(),
    }
  }
}

fn default_csrf() -> String {
  "origin".to_owned()
}

fn default_csrf_scheme() -> String {
  "single_use".to_owned()
}

fn default_csrf_outstanding() -> u32 {
  8
}

/// The render memo: evaluated subtrees keyed by plan node, params, identity
/// and a fingerprint of their data, so a hit skips evaluation and a data
/// change is a miss. `ttl` bounds how long an entry nobody invalidates lives.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CacheSection {
  #[serde(default = "default_cache_capacity")]
  pub capacity: u64,
  #[serde(default = "default_cache_ttl")]
  pub ttl: String,
  /// The data cache over every method whose contract declares `cache`;
  /// absent, no method is cached whatever the contract says. Read it through
  /// [`CacheSection::data_cache`], which honours `enabled = false`.
  #[serde(default)]
  pub data: Option<DataCacheSection>,
}

impl CacheSection {
  /// `[cache.data]` when it turns the data cache on: present and not `enabled = false`.
  pub fn data_cache(&self) -> Option<&DataCacheSection> {
    self.data.as_ref().filter(|data| data.enabled != Some(false))
  }
}

/// `[store]`: how the browser store settles a key two parallel slots both
/// seed. `slot_order` lists slot names, the later winning, `content` for the
/// page; a slot it leaves out loses to every slot it names and stands in name
/// order among the others. Empty, name order decides throughout.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct StoreSection {
  #[serde(default)]
  pub slot_order: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DataCacheSection {
  /// `true` turns the data cache on; any other key does too. `false` keeps it off with the section written.
  #[serde(default)]
  pub enabled: Option<bool>,
  /// Entries per policy; `cache.capacity` when absent.
  #[serde(default)]
  pub capacity: Option<u64>,
}

/// Whether the TOML file at `file` writes the table at `path` with no keys, which c5store drops from the store, so a store alone cannot tell it from an absent one.
fn declares_empty_table(file: &Path, path: &[&str]) -> bool {
  let Some(text) = file.extension().filter(|e| *e == "toml").and_then(|_| std::fs::read_to_string(file).ok()) else { return false };
  let Ok(value) = text.parse::<toml::Table>() else { return false };
  let mut at = toml::Value::Table(value);
  for key in path {
    match at.get(*key) {
      Some(next) => at = next.clone(),
      None => return false,
    }
  }
  at.is_table()
}

fn default_cache_capacity() -> u64 {
  1000
}

fn default_cache_ttl() -> String {
  "1m".to_owned()
}

/// A service the application calls. `document` defaults to
/// `clients/<name>.openapi.json` under the app directory, or
/// `clients/<name>.proto` or `clients/<name>.graphql` when that is the file
/// present. A `.proto` document is reached over gRPC, a `.graphql` one by
/// posting each operation to `base_url` and anything else over HTTP.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
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
  /// An application credential sent on every call: a secret's value as a
  /// bearer token, a named header or a query parameter.
  #[serde(default)]
  pub credential: Option<CredentialConfig>,
  /// The `User-Agent` an HTTP client's calls send; `snapfire_fsr_service/<version>` when absent.
  #[serde(default)]
  pub user_agent: Option<String>,
  /// How long an HTTP client's call waits for the whole answer, a duration such as `5s`; 30 seconds when absent.
  #[serde(default)]
  pub timeout: Option<String>,
  /// The most connections an HTTP client opens to its backend at once; 32 when absent.
  #[serde(default)]
  pub max_connections: Option<usize>,
}

/// `credential = { secret = "cms" }`: the secret `[secrets.cms]` holds, sent
/// as `Authorization: Bearer <value>`; with `header`, as that header's value;
/// with `query`, as that query parameter.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CredentialConfig {
  pub secret: String,
  #[serde(default)]
  pub header: Option<String>,
  #[serde(default)]
  pub query: Option<String>,
}

/// `[secrets.<name>]`: a value the application holds for a backend. Its
/// `value` comes from `C5_SECRETS__<NAME>__VALUE` or is written encrypted;
/// written plain in a file only in development.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SecretEntry {
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub value: Option<Secret>,
}

/// A secret's value, which never prints.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
  pub fn new(value: impl Into<String>) -> Self {
    Self(value.into())
  }

  /// The value, for the transport that sends it.
  pub fn expose(&self) -> &str {
    &self.0
  }
}

impl std::fmt::Debug for Secret {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("Secret(***)")
  }
}

impl schemars::JsonSchema for Secret {
  fn schema_name() -> std::borrow::Cow<'static, str> {
    "Secret".into()
  }

  fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": ["string", "object"], "description": "Set by the C5_SECRETS__<NAME>__VALUE variable or written as a `.c5encval` value; plain text only in development." })
  }
}

impl ClientConfig {
  pub fn is_mock(&self) -> bool {
    self.transport.as_deref() == Some("mock")
  }

  /// Where the mock's responses are read from, relative to the app directory.
  pub fn responses_file(&self, name: &str) -> String {
    self
      .responses
      .clone()
      .unwrap_or_else(|| format!("clients/{name}.mock.json"))
  }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(untagged)]
#[non_exhaustive]
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

/// The `[auth]` section: which method signs users in (or which several) and
/// where the login page is. `users` is the `file` provider's table, relative
/// to the configuration directory and read on its own, never through the ladder.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AuthSection {
  /// The one method, when `methods` is empty.
  #[serde(default)]
  pub provider: Option<String>,
  #[serde(default = "default_login")]
  pub login: String,
  #[serde(default)]
  pub users: Option<String>,
  /// The `[clients.<name>]` entry a `service` provider sends its steps to.
  #[serde(default)]
  pub client: Option<String>,
  /// Several methods on one login page, in the order the page lists them; the first is used when
  /// a sign-in names none and the page offers no choice.
  #[serde(default)]
  pub methods: Vec<MethodSection>,
  /// A method of `methods` that every sign-in through the others must also pass, for the same
  /// subject.
  #[serde(default)]
  pub second_factor: Option<String>,
  /// `oidc`: the issuer, when no `preset` names it.
  #[serde(default)]
  pub issuer: Option<String>,
  /// `oidc`: `google`, `microsoft` or `apple`, which name the issuer and its quirks.
  #[serde(default)]
  pub preset: Option<String>,
  /// `oidc` with `preset = "microsoft"`: the tenant, `common` when absent.
  #[serde(default)]
  pub tenant: Option<String>,
  #[serde(default)]
  pub client_id: Option<String>,
  /// `oidc`: the `[secrets.<name>]` holding the client secret. In development a secret with no
  /// value makes the method a stand-in: a "sign in as" form over the `users` table.
  #[serde(default)]
  pub client_secret: Option<String>,
  /// `oidc`: the scopes asked for, `openid email profile` (Apple: `openid name email`) when absent.
  #[serde(default)]
  pub scopes: Option<Vec<String>>,
  /// `oidc`: claim names the application reads, each taken from the issuer's claim of the value's name.
  #[serde(default)]
  pub claims: BTreeMap<String, String>,
  /// `oidc` with `preset = "apple"`: the team id, the key id and the `[secrets.<name>]` holding
  /// the PEM of the key Apple issued, which signs the client secret.
  #[serde(default)]
  pub team_id: Option<String>,
  #[serde(default)]
  pub key_id: Option<String>,
  #[serde(default)]
  pub signing_key: Option<String>,
  /// `passkey`: the directory the passkeys are kept in, relative to the project, outside `config/`;
  /// they are written to `passkeys.json` inside it.
  #[serde(default)]
  pub store: Option<String>,
  /// `passkey`: what the browser's prompt calls the site, `document.title` when absent.
  #[serde(default)]
  pub rp_name: Option<String>,
  /// `passkey`: `required` to insist the authenticator verifies the user; `preferred` when absent.
  #[serde(default)]
  pub verification: Option<String>,
}

/// One `[[auth.methods]]` entry.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct MethodSection {
  /// What `/auth/login?method=` names it by.
  pub name: String,
  pub provider: String,
  #[serde(default)]
  pub users: Option<String>,
  #[serde(default)]
  pub client: Option<String>,
  /// `oidc`: the issuer, when no `preset` names it.
  #[serde(default)]
  pub issuer: Option<String>,
  /// `oidc`: `google`, `microsoft` or `apple`, which name the issuer and its quirks.
  #[serde(default)]
  pub preset: Option<String>,
  /// `oidc` with `preset = "microsoft"`: the tenant, `common` when absent.
  #[serde(default)]
  pub tenant: Option<String>,
  #[serde(default)]
  pub client_id: Option<String>,
  /// `oidc`: the `[secrets.<name>]` holding the client secret. In development a secret with no
  /// value makes the method a stand-in: a "sign in as" form over the `users` table.
  #[serde(default)]
  pub client_secret: Option<String>,
  /// `oidc`: the scopes asked for, `openid email profile` (Apple: `openid name email`) when absent.
  #[serde(default)]
  pub scopes: Option<Vec<String>>,
  /// `oidc`: claim names the application reads, each taken from the issuer's claim of the value's name.
  #[serde(default)]
  pub claims: BTreeMap<String, String>,
  /// `oidc` with `preset = "apple"`: the team id, the key id and the `[secrets.<name>]` holding
  /// the PEM of the key Apple issued, which signs the client secret.
  #[serde(default)]
  pub team_id: Option<String>,
  #[serde(default)]
  pub key_id: Option<String>,
  #[serde(default)]
  pub signing_key: Option<String>,
  /// `passkey`: the directory the passkeys are kept in, relative to the project, outside `config/`;
  /// they are written to `passkeys.json` inside it.
  #[serde(default)]
  pub store: Option<String>,
  /// `passkey`: what the browser's prompt calls the site, `document.title` when absent.
  #[serde(default)]
  pub rp_name: Option<String>,
  /// `passkey`: `required` to insist the authenticator verifies the user; `preferred` when absent.
  #[serde(default)]
  pub verification: Option<String>,
}

impl AuthSection {
  /// Every method the section configures, the single `provider` as one named after itself.
  pub fn all_methods(&self) -> Vec<MethodSection> {
    match &self.provider {
      Some(provider) if self.methods.is_empty() => vec![MethodSection {
        name: self.preset.clone().unwrap_or_else(|| provider.clone()),
        provider: provider.clone(),
        users: self.users.clone(),
        client: self.client.clone(),
        issuer: self.issuer.clone(),
        preset: self.preset.clone(),
        tenant: self.tenant.clone(),
        client_id: self.client_id.clone(),
        client_secret: self.client_secret.clone(),
        scopes: self.scopes.clone(),
        claims: self.claims.clone(),
        team_id: self.team_id.clone(),
        key_id: self.key_id.clone(),
        signing_key: self.signing_key.clone(),
        store: self.store.clone(),
        rp_name: self.rp_name.clone(),
        verification: self.verification.clone(),
      }],
      _ => self.methods.clone(),
    }
  }
}

pub const PROVIDERS: &[&str] = &["file", "service", "oidc", "passkey"];

/// The issuers `[auth] preset` names.
pub const PRESETS: &[&str] = &["google", "microsoft", "apple"];

/// `[build]`: build-time choices that are not the type checker's.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct BuildSection {
  /// Refuses a component that renders in the browser alone; off when absent.
  #[serde(default)]
  pub strict: Option<bool>,
  /// The `target` every generated tsconfig carries, `es2017` or later or `esnext`; `es2022` when absent.
  #[serde(default)]
  pub target: Option<String>,
  /// The `lib` every generated tsconfig carries; left to the target when absent.
  #[serde(default)]
  pub lib: Option<Vec<String>>,
  /// Cargo projects, relative to the project root, that `fsr build` and `fsr dev` build and run
  /// to completion in order before the application builds.
  #[serde(default)]
  pub before: Vec<String>,
  /// Cargo projects, relative to the project root, that `fsr build` and `fsr dev` build and run
  /// to completion in order after the application builds.
  #[serde(default)]
  pub after: Vec<String>,
}

/// The target a build writes when `[build] target` is absent.
pub const DEFAULT_TARGET: &str = "es2022";

impl BuildSection {
  /// Refuses a target snapfirec cannot deliver, a name tsc does not read as a target and an empty `lib` entry.
  fn check(&self) -> Result<(), String> {
    if let Some(target) = &self.target {
      let lower = target.to_ascii_lowercase();
      let year = lower.strip_prefix("es").filter(|y| y.len() == 4).and_then(|y| y.parse::<u32>().ok());
      match (lower.as_str(), year) {
        ("esnext", _) => {}
        (_, Some(year)) if (2017..=2100).contains(&year) => {}
        (_, Some(_)) => return Err(format!("build.target `{target}` is older than es2017; the modules the build emits need an ES2017 engine")),
        _ => return Err(format!("build.target `{target}` is not a target; write `esnext` or an edition such as `es2023`")),
      }
    }
    if self.lib.as_ref().is_some_and(|lib| lib.iter().any(|entry| entry.trim().is_empty())) {
      return Err("build.lib names an empty library".to_owned());
    }
    for (key, projects) in [("before", &self.before), ("after", &self.after)] {
      if projects.iter().any(|project| project.trim().is_empty()) {
        return Err(format!("build.{key} names an empty project"));
      }
    }
    Ok(())
  }
}

/// The `[typecheck]` section, read by `fsr` rather than by the host: the
/// TypeScript a build checks with, the integrity of a version the checker
/// pins no hash for and a compiler to use as given on a machine that cannot
/// fetch one.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TypecheckSection {
  #[serde(default)]
  pub version: Option<String>,
  #[serde(default)]
  pub sha512: Option<String>,
  #[serde(default)]
  pub tsc: Option<String>,
  /// Whether a build checks types; on when absent.
  #[serde(default)]
  pub enabled: Option<bool>,
}

/// The `[vendor]` section, read by `fsr add` rather than by the host: the
/// hosts it fetches a package from, tried in order. Each one answers the
/// esm.sh API, as esm.sh itself or a copy of it run elsewhere does. A host
/// that fails hands the package to the next; one that says the package or
/// version does not exist ends the search.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct VendorSection {
  #[serde(default)]
  pub hosts: Vec<String>,
}

/// The `[trace]` section. Both lists hold route patterns as `matchit` reads
/// them: `/` is the home page alone and `/docs/{*rest}` the paths under
/// `/docs`. `expose` lists the paths whose requests carry an `x-sf-request`
/// token and whose trace that token reads back from `/__fsr/trace/{token}`.
/// Empty exposes nothing; `fsr dev` exposes every path.
/// `[time] zones`: the time zone source the host reads, one the build holds through the `tz_compiled` or the `tz_system` feature.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TimeZones {
  /// The IANA database compiled into the binary.
  Compiled,
  /// The host's zoneinfo under `/usr/share/zoneinfo`.
  System,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TraceSection {
  #[serde(default)]
  pub expose: Vec<String>,
  /// The paths whose HTML pages end with their own finished trace in a
  /// `<script type="application/json" id="sf-trace">`. Such a page is answered
  /// `private`, since what it carries belongs to one request.
  #[serde(default)]
  pub inline: Vec<String>,
  /// The paths whose HTML pages name their trace id in `<meta name="sf-trace">`,
  /// so a client report can point at the server's side of a slow page. The id
  /// reads nothing back: `expose` is what lets a client read a trace.
  #[serde(default)]
  pub report: Vec<String>,
}

/// `[crawl]`. Each file is served at the path its key names and not at all
/// without one. A site run alone serves its sitemap under its prefix and no
/// `robots.txt`, which only the origin's root may answer.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CrawlSection {
  #[serde(default)]
  pub robots: Option<String>,
  #[serde(default)]
  pub sitemap: Option<String>,
  /// Paths `robots.txt` disallows beside the framework's own `/__fsr/` and `/_sf/`.
  #[serde(default)]
  pub disallow: Vec<String>,
}

/// `[files]`: the directories `file` reads and writes in a body, each relative to the application's directory or absolute. A path a body names is resolved against the application's directory and refused outside every root.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct FilesSection {
  #[serde(default)]
  pub roots: Vec<String>,
}

/// `[pwa]`. The manifest is written from `name` and the fields beside it, or
/// read from `file`, never both; its icons are the PNGs and SVGs the icons
/// directory holds with their size in the name. Outside development the host
/// serves a worker at `/sw.js`, its own unless `worker` names the
/// application's. The worker falls back to `offline` for a page it never cached.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PwaSection {
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default)]
  pub short_name: Option<String>,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub theme_color: Option<String>,
  #[serde(default)]
  pub background_color: Option<String>,
  /// `standalone` when left out.
  #[serde(default)]
  pub display: Option<String>,
  /// `/` when left out.
  #[serde(default)]
  pub start_url: Option<String>,
  /// The application's own manifest, relative to the app directory.
  #[serde(default)]
  pub file: Option<String>,
  /// The application's own worker, relative to the app directory.
  #[serde(default)]
  pub worker: Option<String>,
  /// The route a page the worker never cached shows offline, precached at install.
  #[serde(default)]
  pub offline: Option<String>,
}

impl PwaSection {
  fn check(&self) -> Result<(), String> {
    let fields = [&self.name, &self.short_name, &self.description, &self.theme_color, &self.background_color, &self.display, &self.start_url];
    match (&self.file, &self.name) {
      (Some(_), _) if fields.iter().any(|f| f.is_some()) => Err("names either `file` or the manifest's fields, not both".to_owned()),
      (None, None) => Err("needs `name` for the manifest it writes or `file` naming the application's own".to_owned()),
      _ => Ok(()),
    }
  }
}

/// `[desktop]`. `name` leads the window's title, before the page's own.
/// `serve` is how the window reaches the host and `cookies` where a scheme
/// window keeps the cookies the host sets.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DesktopSection {
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default)]
  pub serve: DesktopServe,
  #[serde(default)]
  pub cookies: DesktopCookies,
}

/// `loopback` listens on a local port the webview loads. `scheme` answers
/// `app://localhost` in process with no port; a response arrives whole, so
/// nothing streams.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DesktopServe {
  #[default]
  Loopback,
  Scheme,
}

/// `memory` keeps a scheme window's cookies for one run; `disk` keeps them
/// in the application's data directory across runs.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DesktopCookies {
  #[default]
  Memory,
  Disk,
}

/// `[mdx]`. The host reads none of it: `fsr` built with its `mdx` feature
/// does, and refuses a key whose feature it was built without.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MdxSection {
  /// GFM tables, strikethrough, task lists, footnotes and literal autolinks; on unless set.
  pub gfm: bool,
  /// An `id` on every heading and a `toc` export; on unless set.
  pub heading_ids: bool,
  /// YAML or TOML frontmatter as `export const meta`; off unless set.
  pub frontmatter: bool,
  /// Fenced code highlighted at build; absent means on when `fsr` carries `mdx-highlight`.
  pub highlight: Option<bool>,
  /// `[mdx.collections]`: a name for each directory of content files, listed in `generated/content.ts`.
  pub collections: BTreeMap<String, String>,
}

impl Default for MdxSection {
  fn default() -> Self {
    Self { gfm: true, heading_ids: true, frontmatter: false, highlight: None, collections: BTreeMap::new() }
  }
}

fn default_login() -> String {
  "/login".to_owned()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct StaticRoot {
  pub route: String,
  pub dir: String,
}

/// A route the application registers with `.route` rather than through the
/// file tree. Boot refuses a declared pattern nothing registers.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct RouteDecl {
  pub pattern: String,
}

const SECTIONS: &[&str] = &[
  "app",
  "server",
  "document",
  "session",
  "cache",
  "store",
  "clients",
  "static",
  "route",
  "locales",
  "auth",
  "typecheck",
  "vendor",
  "build",
  "trace",
  "crawl",
  "desktop",
  "pwa",
  "files",
  "mdx",
  "site",
  "sites",
  "public",
  "dirs",
  "images",
  "media",
  "fonts",
  "secrets",
];

fn default_app_dir() -> String {
  "app".to_owned()
}
fn default_listen() -> String {
  "127.0.0.1:8080".to_owned()
}
fn default_plan() -> String {
  "generated/plan.sexp".to_owned()
}
fn default_contracts() -> String {
  "generated/contracts".to_owned()
}
fn default_cert() -> String {
  "cert.pem".to_owned()
}
fn default_key() -> String {
  "key.pem".to_owned()
}
fn default_reload() -> String {
  "hup".to_owned()
}

fn default_shutdown_grace() -> u64 {
  20
}

fn default_health() -> String {
  "/health".to_owned()
}

fn default_ready() -> String {
  "/ready".to_owned()
}

fn default_max_body() -> usize {
  1 << 20
}
fn default_empty_icon() -> bool {
  true
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

/// A value a deployment outside development must hold, which its files may
/// leave to the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RuntimeValue {
  pub key: String,
  /// The variable c5store reads it from, `env_name(key)`.
  pub env: String,
  pub state: ValueState,
}

impl RuntimeValue {
  pub fn new(key: impl Into<String>, state: ValueState) -> Self {
    let key = key.into();
    Self { env: env_name(&key), key, state }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueState {
  Set,
  /// The value a scaffold writes for a person to replace.
  Placeholder,
  /// Written as a secret this process could not decrypt; the deployment's
  /// keys decrypt it when it starts.
  Encrypted,
  Missing,
}

/// The `C5_` variable c5store layers over the files for `key`: `session.key`
/// is `C5_SESSION__KEY`.
pub fn env_name(key: &str) -> String {
  format!("C5_{}", key.split('.').map(|part| part.to_uppercase()).collect::<Vec<_>>().join("__"))
}

/// The top-level sections the host reads; any other is the application's.
pub fn host_sections() -> &'static [&'static str] {
  SECTIONS
}

/// The configuration a deployment merges, as a JSON Schema (draft 2020-12)
/// read off the types the host deserializes it into: one property per section
/// the host reads, any other top-level table the application's own. A value
/// the deployment's environment may set instead of a file carries
/// `x-runtime`, the `C5_` variable that sets it. An editor validates
/// `app.toml` and its overlays against it; nothing in the host reads it.
pub fn config_schema() -> serde_json::Value {
  use schemars::{JsonSchema, SchemaGenerator};
  let mut generator = SchemaGenerator::default();
  fn of<T: JsonSchema>(generator: &mut SchemaGenerator) -> serde_json::Value {
    generator.subschema_for::<T>().to_value()
  }
  let public = serde_json::json!({
    "type": "object",
    "description": "Values a body reads as `ctx.config`, the same on every request and public: a page may render them.",
    "additionalProperties": { "type": ["string", "integer", "number", "boolean"] }
  });
  let sections: Vec<(&str, serde_json::Value)> = vec![
    ("app", of::<AppSection>(&mut generator)),
    ("server", of::<ServerConfig>(&mut generator)),
    ("document", of::<DocumentConfig>(&mut generator)),
    ("session", of::<SessionSection>(&mut generator)),
    ("cache", of::<CacheSection>(&mut generator)),
    ("store", of::<StoreSection>(&mut generator)),
    ("clients", serde_json::json!({ "type": "object", "additionalProperties": of::<ClientConfig>(&mut generator) })),
    ("static", serde_json::json!({ "type": "array", "items": of::<StaticRoot>(&mut generator) })),
    ("route", serde_json::json!({ "type": "array", "items": of::<RouteDecl>(&mut generator) })),
    ("locales", of::<LocalesSection>(&mut generator)),
    ("auth", of::<AuthSection>(&mut generator)),
    ("typecheck", of::<TypecheckSection>(&mut generator)),
    ("vendor", of::<VendorSection>(&mut generator)),
    ("build", of::<BuildSection>(&mut generator)),
    ("trace", of::<TraceSection>(&mut generator)),
    ("crawl", of::<CrawlSection>(&mut generator)),
    ("desktop", of::<DesktopSection>(&mut generator)),
    ("pwa", of::<PwaSection>(&mut generator)),
    ("files", of::<FilesSection>(&mut generator)),
    ("mdx", serde_json::json!({
      "type": "object",
      "properties": {
        "gfm": { "type": "boolean", "default": true },
        "heading_ids": { "type": "boolean", "default": true },
        "frontmatter": { "type": "boolean", "default": false },
        "highlight": { "type": "boolean" },
        "collections": { "type": "object", "additionalProperties": { "type": "string" } }
      },
      "additionalProperties": false
    })),
    ("site", of::<SiteSection>(&mut generator)),
    ("sites", serde_json::json!({
      "type": "object",
      "properties": { "root": { "type": "string" }, "poll": { "type": "string" }, "store": { "type": "string" } },
      "additionalProperties": of::<MountConfig>(&mut generator)
    })),
    ("public", public),
    ("dirs", of::<DirsSection>(&mut generator)),
    ("images", of::<ImagesSection>(&mut generator)),
    ("media", of::<MediaSection>(&mut generator)),
    ("fonts", serde_json::json!({
      "type": "object",
      "properties": { "base": { "type": "string" } },
      "additionalProperties": of::<FontEntry>(&mut generator)
    })),
    ("secrets", serde_json::json!({
      "type": "object",
      "description": "A value the application's clients send as a credential, `credential = { secret = \"<name>\" }` on a client.",
      "additionalProperties": of::<SecretEntry>(&mut generator),
      "x-runtime": "C5_SECRETS__<NAME>__VALUE"
    })),
  ];
  debug_assert_eq!(sections.iter().map(|(name, _)| *name).collect::<Vec<_>>(), SECTIONS);
  let mut defs = serde_json::Map::new();
  for (name, schema) in generator.take_definitions(true) {
    defs.insert(name, schema);
  }
  for value in Config::default_runtime_keys() {
    let (section, field) = value.split_once('.').expect("a runtime key names its section");
    let def = sections.iter().find(|(name, _)| *name == section).and_then(|(_, schema)| schema.get("$ref")).and_then(|r| r.as_str()).and_then(|r| r.rsplit('/').next());
    if let Some(property) = def.and_then(|def| defs.get_mut(def)).and_then(|def| def.pointer_mut(&format!("/properties/{field}"))).and_then(|p| p.as_object_mut()) {
      property.insert("x-runtime".to_owned(), serde_json::Value::String(env_name(value)));
    }
  }
  serde_json::json!({
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "title": "SnapFire FSR configuration",
    "description": "app.toml and its overlays merged; a top-level table the host does not read is the application's own.",
    "type": "object",
    "properties": serde_json::Map::from_iter(sections.into_iter().map(|(name, schema)| (name.to_owned(), schema))),
    "additionalProperties": true,
    "$defs": defs,
  })
}

/// The deployment axes, read from the environment: `RELEASE_ENV` (default `development`), `APP_ENV` (default `local`) and `APP_REGION` (no default).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Deployment {
  pub release_env: String,
  pub app_env: String,
  pub region: Option<String>,
}

impl Deployment {
  pub fn release_env(mut self, release_env: String) -> Self {
    self.release_env = release_env;
    self
  }

  pub fn app_env(mut self, app_env: String) -> Self {
    self.app_env = app_env;
    self
  }

  pub fn region(mut self, region: Option<String>) -> Self {
    self.region = region;
    self
  }

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
    Self {
      release_env: "development".to_owned(),
      app_env: "local".to_owned(),
      region: None,
    }
  }
}

/// The files a configuration directory contributes, in loading order: `app`, `<release_env>`, `<app_env>`, `<region>`, `<app_env>-<region>` and `bundle`, each as `.toml` then `.yaml`, keeping only those that exist. Any other file in the directory is ignored. `bundle.toml` is what `fsr bundle` writes to name the paths a deploy tree moved, so it loads last and no deployment overlay displaces it.
pub fn config_paths(dir: &Path, deployment: &Deployment) -> Vec<PathBuf> {
  let mut stems = vec![
    "app".to_owned(),
    deployment.release_env.clone(),
    deployment.app_env.clone(),
  ];
  if let Some(region) = &deployment.region {
    stems.push(region.clone());
    stems.push(format!("{}-{}", deployment.app_env, region));
  }
  // Last, so a deploy tree's own layout wins over every overlay: the paths in
  // it are the tree's, not a deployment's opinion about them.
  stems.push("bundle".to_owned());
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
#[non_exhaustive]
pub struct Located {
  pub sources: Vec<PathBuf>,
  pub dir: PathBuf,
  pub root: PathBuf,
}

impl Located {
  /// One more file, loaded after everything found so far. A relative path resolves against the configuration directory.
  pub fn extra(mut self, path: impl AsRef<Path>) -> Self {
    let path = path.as_ref();
    self.sources.push(if path.is_absolute() {
      path.to_path_buf()
    } else {
      self.dir.join(path)
    });
    self
  }
}

/// `locate_with` under the deployment read from the environment.
pub fn locate(path: &Path) -> Result<Located, HostError> {
  locate_with(path, &Deployment::from_env())
}

/// `path` is one of four things: a project root holding `config/`; a `config/` directory; a directory holding `app.toml` or `app.yaml`; a single configuration file. A directory contributes the files `config_paths` names; a file is loaded alone. A file's project root is its directory or the parent when that directory is named `config`.
pub fn locate_with(path: &Path, deployment: &Deployment) -> Result<Located, HostError> {
  if path.is_file() {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let root = if dir.file_name().is_some_and(|n| n == "config") {
      dir.parent().map(Path::to_path_buf).unwrap_or(dir.clone())
    } else {
      dir.clone()
    };
    return Ok(Located {
      sources: vec![path.to_path_buf()],
      dir,
      root,
    });
  }
  if !path.is_dir() {
    return Err(HostError::NoConfig(path.to_path_buf()));
  }
  let config_dir = path.join("config");
  if config_dir.is_dir() {
    return Ok(Located {
      sources: config_paths(&config_dir, deployment),
      dir: config_dir,
      root: path.to_path_buf(),
    });
  }
  if path.file_name().is_some_and(|n| n == "config") {
    return Ok(Located {
      sources: config_paths(path, deployment),
      dir: path.to_path_buf(),
      root: path.parent().map(Path::to_path_buf).unwrap_or_default(),
    });
  }
  if ["app.toml", "app.yaml"].iter().any(|name| path.join(name).is_file()) {
    return Ok(Located {
      sources: config_paths(path, deployment),
      dir: path.to_path_buf(),
      root: path.to_path_buf(),
    });
  }
  Err(HostError::NoConfig(path.to_path_buf()))
}

/// The directory of secret keys, beside the configuration files: `config/private_keys` in the stock layout.
pub const PRIVATE_KEYS: &str = "private_keys";

type SecretsHook = Arc<dyn Fn(&mut SecretOptions) + Send + Sync>;

/// How an artifact is read: the path to locate, the deployment whose ladder
/// applies, any extra files and how the configuration's secrets decrypt. The
/// host keeps the one it was built from, so `Host::reload` reads the same way.
#[derive(Clone)]
pub struct Loader {
  path: PathBuf,
  deployment: Deployment,
  extras: Vec<PathBuf>,
  secrets: Option<SecretsHook>,
  keep_undecrypted: bool,
}

impl std::fmt::Debug for Loader {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Loader")
      .field("path", &self.path)
      .field("deployment", &self.deployment)
      .field("extras", &self.extras)
      .field("secrets", &self.secrets.is_some())
      .finish()
  }
}

impl<P: AsRef<Path>> From<P> for Loader {
  fn from(path: P) -> Self {
    Loader::at(path)
  }
}

#[derive(Deserialize, Default, schemars::JsonSchema)]
struct BuildFacts {
  #[serde(default, rename = "publicPath")]
  public_path: Option<String>,
  #[serde(default)]
  entries: Vec<String>,
  /// Stylesheets the compiler's plugins produced beside their components,
  /// which nothing in the bundle imports.
  #[serde(default)]
  styles: Vec<String>,
  /// Each entry's transitive static imports, in the output directory's own
  /// terms. A module an island loads through `import()` is not an edge here.
  #[serde(default)]
  graph: BTreeMap<String, Vec<String>>,
  /// The bare specifiers the bundle carries, which the page's import map
  /// resolves.
  #[serde(default)]
  externals: Vec<String>,
  /// The suffix of the minified twin the compiler wrote beside each output, when it wrote one.
  #[serde(default)]
  minified: Option<String>,
  #[serde(default)]
  outputs: Vec<String>,
}

/// What the bundle's facts say about its module graph, kept for `server.preload`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Bundle {
  /// The URL prefix the bundle is served under, with no trailing slash.
  pub route: String,
  pub graph: BTreeMap<String, Vec<String>>,
  pub externals: Vec<String>,
  /// The suffix of each output's minified twin, `.min`, when the bundle was built minified.
  pub minified: Option<String>,
  /// Every file the compiler wrote, relative to the bundle's directory.
  pub outputs: BTreeSet<String>,
  /// The stylesheets of the Vue and Svelte components it compiled, relative to the bundle's directory.
  pub styles: Vec<String>,
  /// The directory the outputs are in.
  pub dir: PathBuf,
  hashed: std::sync::OnceLock<BTreeMap<String, String>>,
}

/// Whether the compiler writes a module under this extension: `.js` or a framework component's own
/// extension, which its compiled module keeps.
pub fn is_module_extension(ext: &str) -> bool {
  ext == "js" || snapfire_compiler_wire::claimed(&ext.to_ascii_lowercase()).is_some()
}

/// `30s`, `15m`, `8h`, `2d` or a bare number of seconds.
pub fn parse_duration(raw: &str) -> Option<Duration> {
  snapfire_fsr_core::parse_duration(raw)
}

#[cfg(test)]
mod config_dir_tests;
