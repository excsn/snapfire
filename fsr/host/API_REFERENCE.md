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
  * [DocumentConfig](#documentconfig)
  * [SessionSection](#sessionsection)
  * [ClientConfig](#clientconfig)
  * [StaticRoot](#staticroot)
  * [parse_duration](#parse_duration)
* [2. Building](#2-building)
  * [Host::from](#hostfrom)
  * [HostBuilder](#hostbuilder)
* [3. The Host](#3-the-host)
  * [Host](#host)
  * [RenderMode](#rendermode)
  * [HostReport](#hostreport)
  * [Body](#body)
* [4. Serving](#4-serving)
  * [HostService](#hostservice)
  * [hyper](#hyper)
  * [actix](#actix)
* [5. The Shell](#5-the-shell)
  * [DocumentShell](#documentshell)
  * [head](#head)
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

* `pub struct Config { pub root: PathBuf, pub app: PathBuf, pub sources: Vec<PathBuf>, pub server: ServerConfig, pub document: DocumentConfig, pub session: SessionSection, pub clients: BTreeMap<String, ClientConfig>, pub statics: Vec<StaticRoot>, pub inferred: Vec<String> }`
* `Config::load(path) -> Result<Config, HostError>`: `locate`, then `load_located`.
* `Config::load_located(located: Located) -> Result<Config, HostError>`: `NoConfig` when `sources` is empty; otherwise c5store over the sources in that order with default options, later files overriding, then `C5_*` environment variables with `__` as the level separator, then `from_store`.
* `Config::from_store<S: C5Store>(store: &S, located: Located) -> Result<Config, HostError>`: reads the sections, refuses a top-level key outside `app`, `server`, `document`, `session`, `clients` and `static`, requires `session`, then infers: a static root for `dist` at the build facts' `publicPath`, `document.entry` as `<publicPath>src/main.js` when the facts list that entry, `document.import_map` from `importmap.json`, `/static/js/vendor` from `vendor/`, `/static/css` from `styles/` with `document.styles` as every `.css` file in it sorted by name, plus each client's `document` as `clients/<name>.openapi.json`. Written values win; every inference is listed in `inferred`.
* `Config::resolve(&self, relative: &str) -> PathBuf` joins onto `app`.
* `Config::session_ttl(&self) -> Result<Duration, HostError>`.
* Every table refuses unknown keys.

### AppSection

* `dir` (default `app`), relative to the project root.

### ServerConfig

* `listen` (default `127.0.0.1:8080`), `plan` (default `generated/plan.json`), `contracts` (default `generated/contracts`), a directory whose `*.json` files are merged in name order at boot.

### DocumentConfig

* `title` (default empty), `entry: Option<String>`, `import_map: Option<String>` and `styles: Option<Vec<String>>`, stylesheet URLs linked in order, all three inferred when absent, `shell` (default `shell#document`).

### SessionSection

* `key: String`, required. `store` (default `memory`, the only value accepted), `ttl` (default `8h`), `capacity` (default 4096), `secure` (default false).

### ClientConfig

* `document: Option<String>`, inferred as `clients/<name>.openapi.json` when absent, falling back to `clients/<name>.proto` when only that file exists; `base_url: String`. A `.proto` document is reached with `GrpcTransport`, anything else with `HttpTransport`. The table key is the service name.

### StaticRoot

* `route: String`, `dir: String` relative to the app directory. A trailing slash on `route` is ignored. A written root with the same `route` as an inferred one replaces it.

### parse_duration

* `pub fn config::parse_duration(raw: &str) -> Option<Duration>`: `<n>`, `<n>s`, `<n>m`, `<n>h`, `<n>d`.

## 2. Building

### Host::from

* `Host::from(path: impl AsRef<Path>) -> Result<HostBuilder, HostError>` locates and loads the configuration per `locate`, then the plan file and the contracts directory when it exists, merged with `Contract::merge` file by file.
* `Host::from_cwd() -> Result<HostBuilder, HostError>` is `Host::from(".")`.
* `Host::from_located(located: config::Located) -> Result<HostBuilder, HostError>` is `Host::from_config(Config::load_located(located)?)`.
* `Host::from_config(config: Config) -> Result<HostBuilder, HostError>`.

### HostBuilder

* `services_over(self, transport: Arc<dyn Transport>) -> Self`: every client's calls go to this transport; the contract still comes from the documents.
* `services(self, services: Arc<Services>) -> Self`: a registry built elsewhere, in place of the clients.
* `session_store(self, store: Arc<dyn SessionStore>) -> Self`.
* `shell(self, evaluator: Arc<dyn Evaluator>) -> Self`.
* `route`, `route_override`, `source`, `source_override`, `source_impl`, `action`, `action_override`, `evaluator`: the `snapfire_fsr::AppBuilder` methods with the same signatures.
* `build(self) -> Result<Host, HostError>`: imports the clients, builds the registry with trace and identity interceptors, registers the shell for the document module and `NullEvaluator` for the rest after any evaluators given, applies the contract, builds the app under the binding rule, the session layer and the static roots.

## 3. The Host

### Host

* `pub struct Host { pub report: HostReport, .. }`
* `report(&self) -> &HostReport`; `listen(&self) -> &str`, the configured address.
* `render(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`; `path` may carry a query string, decoded into `ctx.query`. Services are bound with the session's identity and no credentials.
* `render_to_string(&self, path, mode, session) -> Result<String, HostError>`.
* `call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`.
* `handle(&self, req: Request<Bytes>) -> Response<Body>`: static roots first, by prefix; then `POST /_sf/action/<id>` with a JSON body, `400` on a body that does not parse, the failure kind's status on error; then a page, `__payload` in the query selecting the payload mode, `404` for no route. The session is opened from `Cookie` and a `Set-Cookie` is appended when it changed.
* `service(self: &Arc<Self>) -> HostService`.
* `owner_of_source(&self, name: &str) -> Option<Owner>`.

### RenderMode

* `Html`, `Payload`.

### HostReport

* `pub struct HostReport { pub app: snapfire_fsr::Report, pub services: Vec<(String, String, String)>, pub statics: Vec<(String, PathBuf)>, pub config: Vec<PathBuf>, pub inferred: Vec<String> }`
* `Display` prints the app's report, then `services` rows as `<http or grpc> <base url>`, `static` rows, `config` sources and `inferred` lines.

### Body

* `pub type Body = UnsyncBoxBody<Bytes, std::io::Error>`.

## 4. Serving

### HostService

* `pub struct HostService(pub Arc<Host>)`, `Clone`.
* `impl<B: http_body::Body + Send + 'static> tower::Service<Request<B>> for HostService` with `Response = Response<Body>` and `Error = Infallible`; the request body is collected before `handle`.

### hyper

* `Host::serve(self: Arc<Self>, listen: &str) -> std::io::Result<()>` binds and serves HTTP/1 until the listener fails.
* `Host::serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()>`.

### actix

Behind the `actix` feature.

* `actix::handle(req: HttpRequest, host: Data<Arc<Host>>, body: Bytes) -> HttpResponse`: maps the request onto `http::Request<Bytes>`, the response's status, headers and body stream back.
* `actix::serve(host: Arc<Host>, addr: (&str, u16)) -> std::io::Result<()>`.

## 5. The Shell

### DocumentShell

* `pub struct shell::DocumentShell`, an `Evaluator` emitting `<!doctype html><html lang="en"><head>`, the `head` slot, `</head><body><div id="app">`, the `content` slot, `</div></body></html>`.

### head

* `pub fn shell::head(title: &str, styles: &[String], import_map: Option<&str>, entry: Option<&str>) -> Node`: `<meta charset>`, a viewport meta, an escaped `<title>` when non-empty, a `<link rel="stylesheet">` per style, the import map inlined verbatim as `<script type="importmap">`, the entry as `<script type="module" src>`.

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
