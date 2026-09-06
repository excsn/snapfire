# API Reference: snapfire_fsr_host

The stock host: `config/` plus the build's artifacts as a `tower::Service` over `http` types.

## Contents

* [1. Configuration](#1-configuration)
  * [Deployment](#deployment)
  * [config_paths](#config_paths)
  * [locate and Located](#locate-and-located)
  * [Config](#config)
  * [AppSection](#appsection)
  * [ServerConfig](#serverconfig)
  * [TlsSection](#tlssection)
  * [DocumentConfig](#documentconfig)
  * [SessionSection](#sessionsection)
  * [CacheSection](#cachesection)
  * [DataCacheSection](#datacachesection)
  * [ClientConfig](#clientconfig)
  * [StaticRoot](#staticroot)
  * [LocalesSection](#localessection)
  * [AuthSection](#authsection)
  * [BearerKey](#bearerkey)
  * [TypecheckSection](#typechecksection)
  * [MountConfig](#mountconfig)
  * [SiteSection](#sitesection)
  * [SitesSection](#sitessection)
  * [parse_duration](#parse_duration)
* [2. Building](#2-building)
  * [Host::from](#hostfrom)
  * [HostBuilder](#hostbuilder)
* [3. The Host](#3-the-host)
  * [Host](#host)
  * [Mount](#mount)
  * [Server-mode islands](#server-mode-islands)
  * [Locales](#locales)
  * [Resolution](#resolution)
  * [Preflight](#preflight)
  * [RenderMode](#rendermode)
  * [HostReport](#hostreport)
  * [ServiceProvider](#serviceprovider)
  * [ServiceSessionStore](#servicesessionstore)
  * [SiteReport](#sitereport)
  * [Body](#body)
  * [PAYLOAD_ENCODINGS](#payload_encodings)
* [4. Serving](#4-serving)
  * [HostService](#hostservice)
  * [hyper](#hyper)
  * [actix](#actix)
* [5. The Shell](#5-the-shell)
  * [DocumentShell](#documentshell)
  * [head](#head)
  * [canonical](#canonical)
* [6. Error Handling](#6-error-handling)
  * [HostError](#hosterror)

## 1. Configuration

### Deployment

* `pub struct config::Deployment { pub release_env: String, pub app_env: String, pub region: Option<String> }`
* `Deployment::from_env()`: `RELEASE_ENV` (default `development`), `APP_ENV` (default `local`), `APP_REGION` (none), an empty value counting as unset. `Default` is the same without reading the environment.

### config_paths

* `pub fn config::config_paths(dir: &Path, deployment: &Deployment) -> Vec<PathBuf>`: the stems `app`, `<release_env>`, `<app_env>`, `<region>` and `<app_env>-<region>` in that order, repeated stems once, each as `.toml` then `.yaml`, keeping only files that exist.

### locate and Located

* `pub fn config::locate(path: &Path) -> Result<Located, HostError>`: `locate_with` under `Deployment::from_env()`.
* `pub fn config::locate_with(path: &Path, deployment: &Deployment) -> Result<Located, HostError>`: a file is loaded alone, with its directory as the root or the parent when that directory is named `config`; a directory holding `config/` is a root whose sources are `config_paths` of that directory; a directory named `config` is the same with its parent as root; a directory holding `app.toml` or `app.yaml` is its own root and config directory. Anything else is `NoConfig`.
* `pub struct Located { pub sources: Vec<PathBuf>, pub dir: PathBuf, pub root: PathBuf }`
* `Located::extra(self, path: impl AsRef<Path>) -> Self` appends one file, a relative path joined onto `dir`.

### Config

* `pub struct Config { pub root: PathBuf, pub app: PathBuf, pub sources: Vec<PathBuf>, pub server: ServerConfig, pub document: DocumentConfig, pub session: SessionSection, pub cache: Option<CacheSection>, pub clients: BTreeMap<String, ClientConfig>, pub statics: Vec<StaticRoot>, pub locales: Option<LocalesSection>, pub auth: Option<AuthSection>, pub typecheck: Option<TypecheckSection>, pub inferred: Vec<String> }`
* `Config::load(path) -> Result<Config, HostError>`: `locate`, then `load_located`.
* `Config::load_located(located: Located) -> Result<Config, HostError>`: `NoConfig` when `sources` is empty; otherwise c5store over the sources in that order with default options, later files overriding, then `C5_*` environment variables with `__` as the level separator, then `from_store`.
* `Config::from_store<S: C5Store>(store: &S, located: Located) -> Result<Config, HostError>`: reads the sections, refuses a top-level key outside `app`, `server`, `document`, `session`, `cache`, `clients`, `static`, `locales`, `auth`, `typecheck`, `site` and `sites`, requires `session`, refuses an `auth.provider` outside `PROVIDERS` and an `auth.login` that is not a path, then infers: a static root for `dist` at the build facts' `publicPath`, `document.entry` as `<publicPath>src/main.js` when the facts list that entry, `document.import_map` from `importmap.json`, `/static/js/vendor` from `vendor/`, `/static/css` from `styles/` with `document.styles` as every `.css` file in it sorted by name, plus each client's `document` as `clients/<name>.openapi.json`. Written values win; every inference is listed in `inferred`.
* `Config::resolve(&self, relative: &str) -> PathBuf` joins onto `app`.
* `Config::config_dir(&self) -> PathBuf`: the directory of the first file loaded, the project root when none; where `auth.users` resolves.
* `Config::session_ttl(&self) -> Result<Duration, HostError>`.
* `Config::dev(&self) -> bool`: `server.dev` when written, else whether `Deployment::from_env().release_env` is `development`, which it is when the variable is unset.
* `Config::cache_ttl(&self) -> Result<Option<Duration>, HostError>`: `None` without a `[cache]` section; a lifetime nobody can parse is `HostError::Value("cache.ttl", ..)`.
* Every table refuses unknown keys.

### AppSection

* `dir` (default `app`), relative to the project root.

### ServerConfig

* `listen` (default `127.0.0.1:8080`), `plan` (default `generated/plan.json`), `contracts` (default `generated/contracts`), a directory whose `*.json` files are merged in name order at boot.
* `prerender: Option<String>`: the directory, relative to the app, that `prerender` writes and the host reads; absent by default.
* `dev: Option<bool>`: whether the document carries the live-refresh script and the host answers `/__fsr/events` and `/__fsr/changed`; absent, it follows `RELEASE_ENV`.
* `max_body: usize` (default 1048576): the most bytes a request body may carry. `serve` stops reading a larger body and answers 413 with a text naming the key; `handle` answers the same for a body handed to it whole, before a session is opened.
* `http2: bool` (default false): whether a served connection negotiates HTTP/2 as well as HTTP/1.1. The listener carries no TLS and so no ALPN, which makes this h2c: a client opening with the HTTP/2 preface is served and a browser, which speaks HTTP/2 only over TLS, is not. `HostBuilder::http2` overrides it for a host built in Rust. Nothing else changes: the same `Host::handle` answers both versions.
* `tls: Option<TlsSection>`, the `[server.tls]` table: absent, the listener is plain TCP. Present, the host must be built with the `tls` feature or `build` is a `HostError::Config` saying so, since a configured certificate must never be answered with plaintext.

### TlsSection

* `dir: Option<String>`: the directory `cert` and `key` are read under. Absent, each resolves against the project root; an absolute path is taken as written.
* `cert` (default `cert.pem`): the certificate chain in PEM, leaf first.
* `key` (default `key.pem`): the private key in PEM, PKCS#8, PKCS#1 or SEC1.
* `alpn: Option<Vec<String>>`: what the handshake offers, in order. Absent, `["h2", "http/1.1"]` when `server.http2` is on and `["http/1.1"]` when it is not.
* `reload` (default `hup`): the signal that re-reads both files and swaps the certificate for the handshakes that follow. `hup`, `usr1`, `usr2` or `none`; anything else is a `HostError::Config` naming the four. Unix only: elsewhere a new certificate needs a restart.
* `files(&self, root: &Path) -> (PathBuf, PathBuf)`: the two paths as resolved.

### DocumentConfig

* `title` (default empty), `entry: Option<String>`, `import_map: Option<String>` and `styles: Option<Vec<String>>`, stylesheet URLs linked in order, all three inferred when absent, `shell` (default `shell#document`).

### SessionSection

* `key: String`, required. `store` (default `memory`; `service` keeps every record behind the client `client` names, over `ServiceSessionStore`; any other value is `HostError::Value`), `client: Option<String>` (required with `service` and must be a `[clients]` entry, else `HostError::Config`), `ttl` (default `8h`), `capacity` (default 4096), `secure` (default false), `csrf` (default `identified`; `always` mints the token for every session and establishes a fresh session on its first response, and any other value is `HostError::Config`).

### CacheSection

* `capacity: u64` (default 1000), `ttl: String` (default `1m`). Present at all means `build` installs `FibreCache::bounded(capacity, ttl)` on the app; absent means nothing is cached.
* `data: Option<DataCacheSection>`: present means `build` installs the service layer's `DataCache` over every method whose contract declares `cache`; absent means no method is cached.

### DataCacheSection

* `capacity: Option<u64>`: entries per policy, `cache.capacity` when absent. Unknown keys are refused.

### ClientConfig

* `document: Option<String>`, inferred as `clients/<name>.openapi.json` when absent, falling back to `clients/<name>.proto` when only that file exists; `base_url: Option<String>`, required unless the transport is `mock`. A `.proto` document is reached with `GrpcTransport`, anything else with `HttpTransport`. The table key is the service name. `bearer: Option<BearerKey>`: which custody entry the client's calls carry as a bearer token; absent, none.
* `transport: Option<String>`: `mock` answers from `responses` over a `MockTransport` and reaches nothing; any other value is `HostError::Value`. `responses: Option<String>`: the mock's file, relative to the app directory, `clients/<name>.mock.json` when absent; an object of method name to a response in the payload's JSON encoding, or to `{"$fail": {"kind": "<FailureKind>", "message": "..."}}`.
* `is_mock(&self) -> bool`; `responses_file(&self, name: &str) -> String`.

### StaticRoot

* `route: String`, `dir: String` relative to the app directory. A trailing slash on `route` is ignored. A written root with the same `route` as an inferred one replaces it.

### LocalesSection

* `pub struct locale::LocalesSection { pub supported: Vec<String>, pub default: Option<String>, pub order: Vec<String>, pub remember: bool, pub cookie: String }`, the `[locales]` section. `supported` is at least one tag of letters, digits, `_` and `-`, spelled as the application wants to see it; `default` is among them, the first when absent; `order` names the sources consulted, any subset of `prefix`, `cookie` and `header`, all three in that order when absent; `remember` is false by default; `cookie` is `sf_locale` by default. Checked at `build`, which fails with `HostError::Config` naming the offence.

### AuthSection

* `pub struct config::AuthSection { pub provider: String, pub login: String, pub users: Option<String>, pub client: Option<String> }`, the `[auth]` section. `provider` is one of `config::PROVIDERS`, `["file", "service"]`; `login` is the application's login page, `/login` by default, and must start with `/`; `users` is the `file` provider's table, `auth.toml` by default, relative to `config_dir` and read on its own rather than through the ladder; `client` is the `[clients]` entry a `service` provider sends `authenticate` to, required with it and checked against the table.

### BearerKey

* `pub enum config::BearerKey { Toggle(bool), Named(String) }`, untagged: `true`, `false` or a string in the file.
* `key(&self) -> Option<&str>`: `access_token` for `true`, the string for `Named`, `None` for `false`.

### TypecheckSection

* `pub struct config::TypecheckSection { pub version: Option<String>, pub sha512: Option<String>, pub tsc: Option<String>, pub enabled: Option<bool> }`, the `[typecheck]` section. The host reads none of it; `fsr` does, when it spawns `snapfiretc` over the application. `version` is the TypeScript a build checks with, the checker's own default when absent, and `fsr` writes the one it resolved here when the file names none; `sha512` is the integrity a fetch of a version the checker pins no hash for must match; `tsc` is a compiler to use as given; `enabled` is whether a build checks types at all, on when absent.

### SiteSection

* `pub struct SiteSection { pub name: String, pub at: String, pub shell: Option<String> }`: `[site]`. `name` is lowercase letters, digits, `_` and `-`; `at` a path with no trailing slash and no parameter; `shell` the path, against the project root, of the shell contract the site's build reads. `Deserialize`, `Clone`, `PartialEq`, `Eq`.
* `prefix(&self) -> String`: `<name>:`. `under(&self, path: &str) -> String`: `at` joined with a path, `/` being `at` itself.
* An application with `[site]` and `[sites]` is refused. Its `styles/` root is inferred under `<at>/static/css`.

### SitesSection

* `pub struct SitesSection { pub root: Option<String>, pub poll: Option<String>, pub mounts: BTreeMap<String, MountConfig> }`: `[sites]`. `root` is where `name@version` artifacts resolve, required by any mount spelled that way; `poll` a duration or absent.

### MountConfig

* `pub struct MountConfig { pub artifact: String, pub hash: Option<String>, pub allow_engine: bool }`: `[sites.<name>]`. `artifact` is `name@version` under the root or a path against the project root; `hash` pins the content hash; `allow_engine` admits an artifact with engine-owned rows.

### parse_duration

* `pub fn config::parse_duration(raw: &str) -> Option<Duration>`: `<n>`, `<n>s`, `<n>m`, `<n>h`, `<n>d`.

## 2. Building

### Host::from

* `Host::from(path: impl AsRef<Path>) -> Result<HostBuilder, HostError>` locates and loads the configuration per `locate`, then the plan file and the contracts directory when it exists, merged with `Contract::merge` file by file.
* `Host::from_cwd() -> Result<HostBuilder, HostError>` is `Host::from(".")`.
* `Host::from_located(located: config::Located) -> Result<HostBuilder, HostError>` is `Host::from_config(Config::load_located(located)?)`.
* `Host::from_config(config: Config) -> Result<HostBuilder, HostError>`: reads the plan file and the contracts directory the configuration names, then `from_config_with`.
* `Host::from_config_with(config: Config, plan: String, contract: Option<Contract>) -> Result<HostBuilder, HostError>`: over a plan file and a contract already in memory, which is how `fsr test` renders a route a spec loads.

### HostBuilder

* `services_over(self, transport: Arc<dyn Transport>) -> Self`: every client's calls go to this transport; the contract still comes from the documents.
* `services(self, services: Arc<Services>) -> Self`: a registry built elsewhere, in place of the clients.
* `session_store(self, store: Arc<dyn SessionStore>) -> Self`.
* `http2(self, on: bool) -> Self`: negotiates HTTP/2 as well as HTTP/1.1 on a served connection, over `server.http2`.
* `shell(self, evaluator: Arc<dyn Evaluator>) -> Self`.
* `prerendered(self, dir: impl Into<PathBuf>) -> Self`: where prerendered documents are read from, over `server.prerender`.
* `meta(self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self`: describes the segment whose data source is `name` once its data has loaded, the `AppBuilder` method of the same name.
* `identity(self, provider: Arc<dyn IdentityProvider>) -> Self`: the provider behind the `/auth/` routes, in place of the one `[auth]` names; the login page is `auth.login` when the section is written, `/login` otherwise.
* `extension<F>(self, name: impl Into<String>, reach: snapfire_fsr_ir::Reach, f: F) -> Self`: the Rust half of a native pair, forwarded to `AppBuilder::extension`; `name` is `module.member` as the `native(..)` declaration under `ext/` spells it. A plan calling a name nothing registers fails `build` with `BindError::UnknownExtension`.
* `route`, `route_override`, `not_found`, `handler`, `handler_override`, `middleware`, `middleware_override`, `source`, `source_override`, `source_impl`, `action`, `action_override`, `evaluator`: the `snapfire_fsr::AppBuilder` methods with the same signatures.
* `mount(self, mount: Mount) -> Self`: mounts a site, see `Mount`. `config(&self) -> &Config`: the configuration the builder was made from.
* `reloader<F>(self, f: F) -> Self where F: Fn() -> Result<HostBuilder, HostError> + Send + Sync + 'static`: how `Host::reload` rebuilds the tables, a builder for the application as it now stands on disk with whatever this builder was given added again.
* `build(self) -> Result<Host, HostError>`: reads `<app>/locales/*.toml` into the app's catalogs through `locale::load_catalogs`, a file that does not read or a message that is not a string, number or boolean being `HostError::Config`; imports the clients, builds the registry with trace and identity interceptors plus one `CredentialInterceptor::bearer(key).only(clients)` per custody key the clients' `bearer` name, mounts the provider (`DevProvider::from_toml` for `file`, `HostError::Config` naming the file when it cannot be read), registers the shell for the document module and `NullEvaluator` for the rest after any evaluators given, applies the contract, builds the app under the binding rule, refuses a bundle that carries a server module (`HostError::Leak`, see below), then the session layer and the static roots. Everything but the session layer is the host's tables, swapped whole by `reload`.

## 3. The Host

### Host

* `pub struct Host { .. }`: the tables a request reads, the app, the head, the static roots, the prerender directory, the locales, the identity flow and the report, behind one swappable pointer, plus the sessions, which outlive a reload. A request takes the tables once at the edge and keeps them for its lifetime.
* `report(&self) -> Arc<HostReport>`: what the host bound, as of the last reload; `listen(&self) -> &str`, the configured address.
* `locales(&self) -> Locales`: the locales the host serves and how it resolves a request's.
* `reload(&self) -> Result<Arc<HostReport>, HostError>`: rebuilds the tables through the builder's `reloader`, checks them the way `build` does and swaps them in; a request in flight finishes on the tables it started with. `Value("reload", ..)` without a reloader; `Value("session", ..)` when the new configuration's `[session]` differs from the one the running sessions were built from, since the store outlives the reload, and the tables are left alone. Calls `changed` on success.
* `reload_with(&self, builder: HostBuilder) -> Result<Arc<HostReport>, HostError>`: `reload` over a builder the caller made.
* `pub type Reloader = Box<dyn Fn() -> Result<HostBuilder, HostError> + Send + Sync>`.
* `catalogs(&self) -> Option<Arc<snapfire_fsr_ir::Catalogs>>`: the message catalogs loaded from `locales/`, `None` when the directory holds none. A document embeds the request locale's merged table as `<script type="application/json" data-sf-i18n="<tag>">`; a payload carries it as a `D` row unless the request's `x-sf-catalog` header names that locale.
* `render(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`; `path` may carry a locale prefix, stripped before the route matches and resolved into `ctx.locale`, and a query string, decoded into `ctx.query`. Services are bound with the session's identity and no credentials, and no CSRF token is minted; `handle` is where custody and the token ride. A prefixed request for the default locale carries `canonical` in its head.
* `render_to_string(&self, path, mode, session) -> Result<String, HostError>`.
* `intercept_for(&self, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)>`: the intercept a soft navigation to `path` renders, of the route's variants in file order: with `into`, the one whose slot it names; otherwise the first whose layouts, module for module from the shell down to the one declaring its slot, the route of `from`, the origin's path, shares. Both paths are without their query.
* `render_navigation(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`: the payload for a soft navigation: the intercept when `intercept_for` finds one, else `render` in `Payload` mode. `path` may carry its query; `from` may too.
* `render_navigation_to_string(&self, path, from, into, session) -> Result<String, HostError>`.
* `render_not_found(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<Option<BoxStream<'static, String>>, HostError>`: the application's not-found tree for `path`, with `params.path` set to the path without its query string, or `None` when the application has none.
* `auth_call(&self, flow: &AuthFlow, method: &str, path: &str, query: &str, body: &[u8], headers: &[(String, String)], session: SessionCell) -> Option<(u16, Vec<(String, String)>, String)>`: the three `/auth/` routes against a session cell rather than a cookie, for a caller that holds the session itself, as a status, the response headers and the body. `None` when no provider is mounted or the path is not one of the three. The identity the flow settles lands in `session`, the cell the caller renders pages with; the `Set-Cookie` comes back as a header and is the caller's to ignore. `fsr test` is the caller, and a journey starts at `GET /auth/login`, which is what puts a flow in progress; a callback with none is 400.
* `pub struct AuthFlow`, with `new()` and `Default`: what a cookie carries between the calls of one journey, the session id the flow is keyed by and the custody its state lives in. One per session, since a second `AuthFlow` is a second browser.
* `call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`: `call_action_in` under the default locale.
* `call_action_in(&self, id: &str, session: SessionCell, locale: Locale, input: Value) -> Result<Value, ActionError>`: runs the action with `locale` as its `ctx.locale`.
* `prerenderable(&self) -> Vec<String>`: the patterns one render serves for every request: no parameter, every source lowered and reading nothing of the request (`snapfire_fsr_ir::body_reads_request`), no Rust source, no page or layout on the plan reading its `identity` or `csrf_token` prop (`Component::reads_prop`).
* `prerenderable_anonymous(&self) -> Vec<String>`: the patterns one anonymous render serves for every anonymous request: no parameter, every source lowered and reading nothing of the request but `identity` and calls through a client whose `bearer` is set, no page or layout on the plan reading `csrf_token`. `handle` serves their files only to a request whose session carries no identity and renders a signed-in visitor live; `Display` lists them under `prerender` with `for anonymous visitors`.
* `prerender(&self, out: &Path) -> Result<Vec<(String, PathBuf)>, HostError>`: renders each of those, the anonymous class included, anonymously once per supported locale and writes `<out>/<path>/index.html` and `index.payload`, `/` at the top of `out`, a locale other than the default under its tag, `<out>/fr_FR/<path>/`; returns what it wrote, each path with its prefix.
* `changed(&self)`: tells every open `/__fsr/events` stream that something changed; nothing when `dev` is off.
* `publish(&self, topic: impl Into<String>)`: tells every open `/_sf/live` stream watching `topic` that it changed. A publish nobody is watching is a send into an empty channel. What a listener does with it is the client's: `live()` revalidates the route it is showing, so the loaders run again and the page follows.
* `invalidate(&self, plan_key: &str) -> usize`: drops every cached subtree under the plan `cache_key`, a lowered page's or layout's module name, and says how many went; zero without a `[cache]` section.
* `invalidate_tags<I, S>(&self, tags: I)`: drops every data cache answer under the named tags; nothing without `[cache.data]`.
* `services(&self) -> Arc<Services>`: the registry the routes call through.
* `prerendered(&self, path: &str, mode: RenderMode) -> Option<String>`: the text held under the prerender directory for the path, its locale prefix choosing the locale's directory and its query string ignored; `None` without a directory or a file.
* `preflight(&self, method: &str, path: &str, session: SessionCell) -> Result<Preflight, ActionError>`: runs the middleware with `{ method, path, payload }` as its input, `payload` true when the query carries `__payload`, the path stripped of its locale prefix, the locale in `ctx.locale` and the query string of `path` as `ctx.query`; `Preflight::pass()` when the application has none; `Internal` when the value is not one `Preflight::from_value` reads.
* `call_handler(&self, method: &str, path: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`: the handler matching the method and the path, its locale prefix stripped and resolved into `ctx.locale` and its query string becoming `ctx.query`, run with `input` as the request body; `NotFound` when none matches.
* `handle(&self, req: Request<Bytes>) -> Response<Body>`: static roots first, by prefix; then the locale, `Locales::resolve` over the path, the `Cookie` header and `Accept-Language`, or for the action route over the `x-sf-from` header's path instead of the request's, the stripped path standing in for the rest and a prefixed `/_sf/`, `/__fsr/` or, with a provider mounted, `/auth/` path answered `404`; then, with a provider mounted, the identity routes: `GET /auth/login` answers 303 to what `Auth::login` returns, its `return_to` the query's when that is a path on this origin, else the `Referer`'s path, else `/`; `/auth/callback` reads its params from a form-encoded or JSON `POST` body or from a `GET` query, answers 303 to the flow's destination, 303 to `<login>?error=denied&return_to=<pending>` on `AuthError::Denied` and 400 with the message on `Invalid`; `POST /auth/logout` verifies `_csrf` from the body or `x-sf-csrf` from the headers with `Sessions::verify_csrf`, 403 when it fails, else `Auth::logout`, `Sessions::destroy` and 303 `/` carrying the expiring cookie and no persist; a `GET` of the login page calls `Auth::ensure_flow` with the query's `return_to`, else the `Referer`'s path when that is not the login page, else `/`, then continues; then the middleware, whose redirect or response is answered at once, whose rewrite replaces the path for the rest and whose headers join whatever response follows, the shell's first with `site` in its input naming the mounted site the path belongs to or null, then, when the path is under a mounted site's prefix, that site's on the same path (the shell's rewrite re-matched once), which may redirect, respond, add headers or rewrite within its own prefix and is `Internal` when it rewrites outside it, headers appending in order; then `POST /_sf/action/<id>`, the id's `%XX` escapes decoded since a site's carries a colon: with a JSON body, `400` on a body that does not parse, the value as JSON, the failure kind's status on error; with a form-encoded body, `_csrf` taken out and verified with `Sessions::verify_csrf`, `403` when it fails, the remaining fields the input as strings, a success answered `303` to the `Referer`'s path on this origin (else `/`) and a failure as the JSON error; then a handler matching the method and path, its JSON body as the input or `null` when empty, answered with the value as JSON, `400` on a body that does not parse and the failure kind's status on error; then, for a `GET` the prerender directory holds, that text with `x-sf-prerendered: 1`, unless the route is prerendered for anonymous visitors and the session carries an identity; then a page, `__payload` in the query selecting the payload mode, in which an `enc` outside `PAYLOAD_ENCODINGS` is `406`, and in that mode an `x-sf-from` or `x-sf-into` header makes it `render_navigation`, which skips the prerender directory when an intercept applies; for no route, the not-found tree with status `404` when the application has one, else `404` with a line of text. The session is opened from `Cookie` and a `Set-Cookie` is appended when it changed; so is the locale cookie a `Resolution` asks for. Every body under `handle` runs with the session's token custody bound to its services and, once the session is identified, the session's CSRF token as `ctx.csrf`; an anonymous request carries no token, so its renders share the memo.
* `GET /_sf/live?topics=a,b` answers with a `text/event-stream`, whatever `dev` says: a `: open` comment frame at once, then one `data: {"topic":"a"}` per `publish` of a topic in the list, until the client goes away. No topics is 400. The path is framework-owned and never locale-prefixed.
* `GET /__fsr/sites` answers, whatever `dev` says, with `{"sites": [{"name", "at", "version", "hash"}, ..]}` for every mounted site, before statics, middleware and sessions.
* On a mounted site's routes the head gains the site's stylesheets and `<script type="module">` for its entry, and the payload an `E` row naming the entry, so the navigator loads the site's islands on first arrival.
* With `dev` on, `handle` answers `GET /__fsr/events` with a `text/event-stream` body, one `data: {"bundle":"<id>"}` event on open and one per `changed`, `POST /__fsr/changed` with 204 after calling `changed` and `POST /__fsr/reload` with 200 and the new report as text after `reload`, or 500 with the error, all before statics, middleware and sessions; static files gain `Cache-Control: no-cache`. The bundle id is a hash over every output `dist/.snapfire-build.json` lists, source maps aside, `-` without a bundle; a served document's head carries `dev_script` with the id of that moment and `prerender` writes the plain head.
* `service(self: &Arc<Self>) -> HostService`.
* `owner_of_source(&self, name: &str) -> Option<Owner>`.

### Server-mode islands

* `POST /_sf/island/<module>`, the module id percent-encoded, with a JSON body `{ props, state, handler, event }`: one round trip of an island in server mode. `state` may name only the component's state; `handler` is the index of the handler that fired or null to render as is; `event` is what the browser saw. Answers `200` with `{ state, html }`, the state after the handler and the island's markup rendered from it, `400` with `{ kind: "invalid", message }` for a body that is not the shape or a state key the component lacks, `404` for a module that is not lowered or a handler index it lacks, and the interpreter's own status for a failure. The document's locale is the island's, from `x-sf-from` the way an action's is. The session cookie is set as on every response.
* `pub fn island_step(lowered: Option<&IrEvaluator>, module: &str, body: &[u8], locale: &str) -> (StatusCode, serde_json::Value)`: the route's work without HTTP, which `fsr test` calls too.
* `Host::lowered(&self) -> Option<Arc<IrEvaluator>>`: the lowered components and their interpreter, `None` when the plan lowered no component.

### Mount

* `pub struct Mount { pub name: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub allow_engine: bool, pub config: Config, pub plan: String, pub contract: Option<Contract> }`: a site's artifact as the host mounts it, its configuration read through the shell's ladder, its plan and contracts as its build wrote them. Where it came from is the caller's business; `artifact`, `version` and `hash` reach the report.
* `Mount::load(name, artifact: impl Into<PathBuf>, version, hash, allow_engine: bool) -> Result<Mount, HostError>`: reads the artifact at `artifact`, a project directory with its `config/` beside its app, the way `Host::from` reads one.
* At `build`, a mount is refused (`HostError::Mount`) when its configuration has no `[site]` or names another site, when its prefix is already served, when it carries engine-owned rows without `allow_engine`, or when its bundle carries a server module. Otherwise its routes and intercepts are nested under the shell's `routes/layout.tsx#default`, or under the document when the shell has none, with ids renumbered; its sources, actions, components and handlers join the shell's tables; its contract is merged; its clients register under `<name>:<client>` with their bearer keys; its static roots under its prefix are served and the rest listed as ignored, with `session`, and `auth`, `locales` and `cache` when set; its import map entries the shell lacks are added; its middleware is held apart, see `handle`; its stylesheets and entry module join the head on its routes.

### Locales

* `pub struct locale::Locales { pub supported: Vec<String>, pub default: String, pub order: Vec<Source>, pub remember: bool, pub cookie: String }`, the checked section; `locale::Source` is `Prefix`, `Cookie` or `Header`.
* `Locales::single() -> Locales`: what a host without a `[locales]` section holds: `en` alone, no source consulted.
* `Locales::from_section(&LocalesSection) -> Result<Locales, String>`.
* `is_default(&self, tag) -> bool`; `locale(&self, tag) -> snapfire_fsr_runtime::Locale`, the tag with its default flag; `default_locale(&self) -> Locale`.
* `find(&self, tag) -> Option<&str>`: the supported locale `tag` spells, case and `-` against `_` ignored. `nearest(&self, tag) -> Option<&str>`: that, else the first supported locale of the same language.
* `split_prefix(&self, path) -> Option<(&str, &str)>`: the supported locale the first path segment spells and the rest of the path, `/` at least; `None` when there is no such prefix or `prefix` is not a source.
* `from_accept_language(&self, header) -> Option<&str>`: the nearest supported locale of the header's tags taken by descending weight, `*` ignored. `from_cookie(&self, header) -> Option<&str>`: the supported locale the cookie names.
* `resolve(&self, path, cookie: Option<&str>, accept_language: Option<&str>) -> Resolution`: the sources in `order`, the first answering; the default when none does.

### Resolution

* `pub struct locale::Resolution { pub locale: Locale, pub path: String, pub prefixed: bool, pub set_cookie: Option<String> }`: the locale, the path without its prefix, whether it had one, and the `Set-Cookie` value to append when `remember` is on and the prefix chose a locale the cookie does not hold, `sf_locale=<tag>; Path=/; Max-Age=31536000; SameSite=Lax`.

### Preflight

* `pub struct Preflight { pub action: PreflightAction, pub headers: Vec<(String, String)> }`, `PartialEq`.
* `pub enum PreflightAction { Continue, Rewrite(String), Redirect { to: String, status: u16 }, Respond { status: u16, body: Value } }`
* `Preflight::pass() -> Self`: `Continue` with no headers.
* `Preflight::from_value(value: &Value) -> Result<Self, String>`: null or an empty object continues; `redirect` wins over `status`, which wins over `rewrite`; a redirect's status is `status` or 307; `headers` must be an object of strings. Any other shape is the error's message.

### RenderMode

* `Html`, `Payload`.

### HostReport

* `pub struct HostReport { pub app: snapfire_fsr::Report, pub services: Vec<(String, String, String)>, pub statics: Vec<(String, PathBuf)>, pub cache: Option<(u64, String)>, pub locales: Vec<String>, pub auth: Option<(String, String)>, pub bearer: Vec<(String, String)>, pub extensions: Vec<String>, pub site: Option<(String, String)>, pub sites: Vec<SiteReport>, pub config: Vec<PathBuf>, pub inferred: Vec<String> }`
* `extensions: Vec<String>`: the native pairs registered beside the standard library, by name; `Display` prints them as `natives` rows labelled `rust`, after `bearer`.
* `catalogs: Vec<(String, usize)>`: each locale with a file under `locales/` and how many keys it holds; `Display` prints one `catalogs` row, `en_US 5 keys, fr_FR 5 keys`.
* `site: Option<(String, String)>`: the application's own `[site]`, name and prefix; `Display` prints one `site` row. `sites: Vec<SiteReport>`: the mounted sites; `Display` prints a `sites` row per mount, `billing at /billing from <artifact> <version> <hash>`, and a row naming what the mount ignored.

### SiteReport

* `pub struct SiteReport { pub name: String, pub at: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub ignored: Vec<String> }`: one mounted site; `ignored` lists the site's configuration the shell did not take, `static <route>` for a root outside the prefix or one the shell serves, and `session`, `auth`, `locales`, `cache` when the site set them.
* `Display` prints the app's report, then `services` rows as `<http, grpc or mock> <base url or responses file>`, `static` rows, `config` sources and `inferred` lines.
* `prerender: Option<PathBuf>`: the prerender directory when one is configured; `Display` lists each prerenderable pattern with it (`not configured` when there is none).
* `cache: Option<(u64, String)>`: the capacity and lifetime as written; `Display` prints one `cache` row when set.
* `dev: bool`: `Display` prints one `dev` row naming the two paths when true.
* `http2: bool`: `Display` prints one `http2` row when true, naming the ALPN list under TLS and saying it is h2c without.
* `tls: Option<TlsReport>`: `cert`, `key`, `alpn` and `reload`, the signal or `None`. `Display` prints the two files and what re-reads them.
* `locales: Vec<String>`: the configured locales, the default first, empty without a `[locales]` section; `Display` prints one `locales` row, `en_US (default, unprefixed), fr_FR`, when set.
* `auth: Option<(String, String)>`: the provider name (`file`, `service via <client>`, else `custom` for one the builder was handed) and the login page; `Display` prints one `auth` row naming both and the three routes, plus `bearer    none; no client carries a token` when `bearer` is empty.
* `bearer: Vec<(String, String)>`: client and custody key for every client whose `bearer` names one; `Display` prints a `bearer` row per client.
* `session: Option<String>`: the client the sessions live behind when `session.store` is `service`; `Display` prints one `session` row, `service via <client>`.
* `cached: Vec<(String, String)>`: `service.method` and its policy as text, `ttl 15s shared, stale 2m [tags]`; `Display` prints a `cached` row per method. `writers: Vec<(String, String)>`: `service.method` and `[tags]`; a `writes` row per method. The `auth` name for a `service` provider reads `service via <client>`.

### ServiceSessionStore

* `pub struct ServiceSessionStore`, a `SessionStore` over a client: `new(services: Arc<Services>, client: impl Into<String>) -> Self`. `load` calls `getSession { id }` and reads `record` from the answer, `None` on a `not_found` failure and, logged, on any other; `save` calls `putSession { id, record }`; `delete` calls `deleteSession { id }`. The record is `encode_record`'s string.
* `pub fn encode_record(record: &SessionRecord) -> String` and `pub fn decode_record(text: &str) -> Option<SessionRecord>`: the record as one JSON string in the payload encoding, `{ data, identity, tokens }` with `identity` as `{ subject, claims }` or `null`.

### ServiceProvider

* `pub struct ServiceProvider`, an `IdentityProvider` over a client: `new(services: Arc<Services>, client: impl Into<String>, login_path: impl Into<String>) -> Self`. `begin` sends the browser to the login page with `return_to`; `callback` sends the form's `user` and `password` to `authenticate` and reads `subject`, `claims` and `access_token` from a map answer. A failure of kind `unauthorized`, `not_found` or `invalid` is `AuthError::Denied` with the service's message; any other failure, a non-map answer or one without a `subject` is `AuthError::Invalid`.

### Body

* `pub type Body = UnsyncBoxBody<Bytes, std::io::Error>`.

### PAYLOAD_ENCODINGS

* `pub const PAYLOAD_ENCODINGS: &[&str] = &["json"]`: what a payload request may name in `enc`; the wire's `V` row names the one it got.

## 4. Serving

### HostService

* `pub struct HostService(pub Arc<Host>)`, `Clone`.
* `impl<B: http_body::Body + Send + 'static> tower::Service<Request<B>> for HostService` with `Response = Response<Body>` and `Error = Infallible`; the request body is collected before `handle`.

### hyper

* `Host::serve(self: Arc<Self>, listen: &str) -> std::io::Result<()>` binds and serves until the listener fails: HTTP/1.1, or HTTP/1.1 and h2c when `server.http2` is on.
* `Host::serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()>`. Each connection is served by `hyper::server::conn::http1`, or by `hyper_util::server::conn::auto` when `http2` is on, which reads the connection preface and answers either version. With `[server.tls]` the connection is a TLS one first, the version chosen by ALPN, and a handshake that fails is logged and dropped without touching the listener.
* `Host::reload_tls(&self) -> Result<(), HostError>`, the `tls` feature: re-reads the certificate and its key and swaps what the next handshake presents. Connections already up keep the certificate they started on, and a file that will not read leaves the running one in place, so a failed reload never takes the listener down. `serve_listener` calls it on the configured signal; nothing happens without `[server.tls]`.

### actix

Behind the `actix` feature.

* `actix::handle(req: HttpRequest, host: Data<Arc<Host>>, body: Bytes) -> HttpResponse`: maps the request onto `http::Request<Bytes>`, the response's status, headers and body stream back.
* `actix::serve(host: Arc<Host>, addr: (&str, u16)) -> std::io::Result<()>`.

## 5. The Shell

### DocumentShell

* `pub struct shell::DocumentShell`, an `Evaluator` emitting `<!doctype html><html lang="<locale>" data-sf-locale="<locale>"><head>`, the `head` slot, `</head><body><div id="app">`, the `content` slot, `</div></body></html>`. `lang` is the `locale` prop in BCP 47 spelling, `fr-FR` for `fr_FR`; `data-sf-locale` is the prop as written; `en` for both without the prop.

### head

* `pub fn shell::dev_script(bundle: &str) -> String`: the `<script>` a development document carries, with `bundle` as the id it was rendered against. It opens `EventSource("/__fsr/events")`; an event whose `bundle` differs reloads, the first event after a connect is otherwise ignored, and any later one re-links every stylesheet with a `__sf` query string and calls `window.__sf.refresh`, or reloads when nothing is registered there.
* `pub fn shell::head(title: &str, styles: &[String], import_map: Option<&str>, entry: Option<&str>) -> snapfire_fsr_runtime::Head`: a head whose default title is `title` and whose `rest` is `<meta charset>`, a viewport meta, a `<link rel="stylesheet">` per style, the import map inlined verbatim as `<script type="importmap">`, the entry as `<script type="module" src>`.

### canonical

* `pub fn shell::canonical(path: &str) -> String`: `<link rel="canonical" href="<path>">`, which a prefixed request for the default locale carries in its head.

## 6. Error Handling

### HostError

* `Io(PathBuf, std::io::Error)`
* `NoConfig(PathBuf)`
* `Config(PathBuf, String)`, the source and the loading, deserialising or unknown-key message.
* `Value(String, String)`, a setting and the value that did not parse.
* `Bind(BindError)`, transparent.
* `Import { document: String, error: ImportError }`
* `Transport(String, String)`, the client name and why its transport could not be built.
* `Contract(PathBuf, String)`, a contract file that did not parse or defines a type or service an earlier file already defined.
* `NotFound(String)`
* `Assemble(AssembleError)`, transparent.
* `Mount(String, String)`: a site could not be mounted, the site's name and why.
* `Leak(String)`: the bundle under `dist/` carries a server module. `build` reads `dist/.snapfire-build.json` when it exists, takes every module the plan's sources, actions and handlers name plus `middleware.ts` as server-only, maps each to its output by the `app/<path>.ts` to `dist/<path>.js` convention and refuses when an output is one of them or, by the manifest's `graph`, imports one; the message lists each with its reason.
