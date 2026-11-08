# API Reference: snapfire_fsr_host

The stock FSR host: `config/` and the build's artifacts served as a `tower::Service` over `http` types.

## Contents

* [1. Configuration](#1-configuration)
  * [Deployment](#deployment)
  * [config_paths](#config_paths)
  * [locate](#locate)
  * [Located](#located)
  * [Loader](#loader)
  * [Config](#config)
  * [RuntimeValue](#runtimevalue)
  * [config_schema](#config_schema)
  * [env_name](#env_name)
  * [host_sections](#host_sections)
  * [AppSection](#appsection)
  * [ServerConfig](#serverconfig)
  * [TlsSection](#tlssection)
  * [DocumentConfig](#documentconfig)
  * [Text](#text)
  * [Csp](#csp)
  * [ClientBuild](#clientbuild)
  * [Bundle](#bundle)
  * [PublicValue](#publicvalue)
  * [SessionSection](#sessionsection)
  * [CacheSection](#cachesection)
  * [DataCacheSection](#datacachesection)
  * [StoreSection](#storesection)
  * [ClientConfig](#clientconfig)
  * [SecretEntry](#secretentry)
  * [StaticRoot](#staticroot)
  * [RouteDecl](#routedecl)
  * [LocalesSection](#localessection)
  * [AuthSection](#authsection)
  * [MethodSection](#methodsection)
  * [BearerKey](#bearerkey)
  * [TypecheckSection](#typechecksection)
  * [VendorSection](#vendorsection)
  * [BuildSection](#buildsection)
  * [TraceSection](#tracesection)
  * [CrawlSection](#crawlsection)
  * [DesktopSection](#desktopsection)
  * [PwaSection](#pwasection)
  * [FilesSection](#filessection)
  * [MdxSection](#mdxsection)
  * [SiteSection](#sitesection)
  * [SitesSection](#sitessection)
  * [MountConfig](#mountconfig)
  * [DirsSection](#dirssection)
  * [MediaSection](#mediasection)
  * [ImagesSection](#imagessection)
  * [FontsSection](#fontssection)
  * [FontEntry](#fontentry)
  * [parse_duration](#parse_duration)
* [2. Building](#2-building)
  * [Artifact](#artifact)
  * [Host::from](#hostfrom)
  * [HostBuilder](#hostbuilder)
* [3. The Host](#3-the-host)
  * [Host](#host)
  * [Host::reload](#hostreload)
  * [Host::render](#hostrender)
  * [Host::prerender](#hostprerender)
  * [Host::handle](#hosthandle)
  * [Framework Routes](#framework-routes)
  * [IslandBody and IslandStep](#islandbody-and-islandstep)
  * [AuthFlow](#authflow)
  * [Request Helpers](#request-helpers)
  * [Probe](#probe)
  * [ReloadCheck](#reloadcheck)
  * [Candidate](#candidate)
  * [Mount](#mount)
  * [Locales](#locales)
  * [Resolution](#resolution)
  * [RenderMode](#rendermode)
  * [HostReport](#hostreport)
  * [TlsReport](#tlsreport)
  * [SiteReport](#sitereport)
  * [CrawlReport](#crawlreport)
  * [ServiceSessionStore](#servicesessionstore)
  * [ServiceProvider](#serviceprovider)
  * [AssetsManifest](#assetsmanifest)
  * [tera](#tera)
  * [Body](#body)
  * [Constants](#constants)
* [4. Serving](#4-serving)
  * [HostService](#hostservice)
  * [Host::serve](#hostserve)
  * [actix](#actix)
  * [tls](#tls)
  * [socket](#socket)
  * [client](#client)
* [5. Observing a Request](#5-observing-a-request)
  * [Collectors](#collectors)
  * [Trace Output](#trace-output)
  * [Spans](#spans)
* [6. The Shell](#6-the-shell)
  * [DocumentShell](#documentshell)
  * [Head Functions](#head-functions)
* [7. Conformance](#7-conformance)
  * [Class](#class)
  * [Route](#route)
  * [Breach](#breach)
  * [framework_routes and check](#framework_routes-and-check)
* [8. Error Handling](#8-error-handling)
  * [HostError](#hosterror)

## 1. Configuration

Module `config`. Every table refuses a key it does not declare.

### Deployment

The three environment values that choose the overlay files.

* `pub struct config::Deployment { pub release_env: String, pub app_env: String, pub region: Option<String> }`, `#[non_exhaustive]`.
* `Deployment::from_env() -> Deployment`: `RELEASE_ENV` (default `development`), `APP_ENV` (default `local`) and `APP_REGION` (none). An empty value counts as unset.
* `Default`: the same defaults without reading the environment.
* `release_env(self, release_env: String) -> Self`, `app_env(self, app_env: String) -> Self`, `region(self, region: Option<String>) -> Self`: setters for each field.

### config_paths

* `pub fn config::config_paths(dir: &Path, deployment: &Deployment) -> Vec<PathBuf>`: the files under `dir` for the stems `app`, `<release_env>`, `<app_env>`, `<region>` and `<app_env>-<region>`, in that order.
* A repeated stem is read once. Each stem is tried as `.toml` then `.yaml`. Only files that exist are returned.

### locate

* `pub fn config::locate(path: &Path) -> Result<Located, HostError>`: `locate_with` under `Deployment::from_env()`.
* `pub fn config::locate_with(path: &Path, deployment: &Deployment) -> Result<Located, HostError>`: what `path` names, by these rules.
  * A file is loaded alone. Its directory is the root. When that directory is named `config`, its parent is the root.
  * A directory holding `config/` is the root. Its sources are `config_paths` of `config/`.
  * A directory named `config` is the same, with its parent as the root.
  * A directory holding `app.toml` or `app.yaml` is both the root and the configuration directory.
  * Anything else is `NoConfig`.

### Located

* `pub struct config::Located { pub sources: Vec<PathBuf>, pub dir: PathBuf, pub root: PathBuf }`
* `extra(self, path: impl AsRef<Path>) -> Self`: appends one file. A relative path is joined onto `dir`.

### Loader

How an artifact is read: the path, the deployment, extra files and how secrets decrypt.

* `pub struct Loader { .. }`. `Clone`, `Debug`. `From<P: AsRef<Path>>` is `Loader::at`.
* `Loader::at(path: impl AsRef<Path>) -> Loader`: `path` is anything `locate` accepts, under `Deployment::from_env()`.
* `deployment(self, deployment: Deployment) -> Self`: the deployment to read, in place of the environment's.
* `extra(self, path: impl AsRef<Path>) -> Self`: one more file after the overlays, as `Located::extra`.
* `secrets<F>(self, f: F) -> Self where F: Fn(&mut SecretOptions) + Send + Sync + 'static`: adjusts c5store's `SecretOptions` after the defaults are set. `SecretOptions` is re-exported from `config`.
  * The defaults register the `base64` and `ecies_x25519` decryptors.
  * `secret_keys_path` is `private_keys` beside the configuration files when that directory exists (`config/private_keys` in the stock layout). The name is `config::PRIVATE_KEYS`.
  * `load_secret_keys_from_env` is on: `C5_SECRETKEY_<name>` holds a base64 key under the lowercased `<name>`.
* `keep_undecrypted(self, keep: bool) -> Self`: a `session.key` or secret value this process cannot decrypt is listed in `Config::undecrypted` instead of refused. The host never loads this way.
* `mount(&self, path: impl AsRef<Path>) -> Loader`: a loader for an artifact at `path` with this loader's deployment, secrets and `keep_undecrypted`. None of its extra files are carried over.
* `path(&self) -> &Path`
* `locate(&self) -> Result<Located, HostError>`: `locate_with` under the deployment, with the extras appended.
* `config(&self) -> Result<Config, HostError>`: the merged configuration.
  * `NoConfig` when nothing was located.
  * The sources are merged in order, later files overriding. Then `C5_*` environment variables apply, with `__` as the level separator. Then `Config::from_store`.
  * A `.c5encval` whose decryptor or key is missing decrypts to nothing. The section holding it then fails to read and the error names the section.
* `load(&self) -> Result<Artifact, HostError>`: `config`, then `Artifact::of`.

### Config

The configuration the host reads.

* `pub struct Config` with these public fields:
  * `root: PathBuf`, `app: PathBuf`, `runs: Runs`, `sources: Vec<PathBuf>`
  * `server: ServerConfig`, `document: DocumentConfig`, `session: SessionSection`, `cache: Option<CacheSection>`, `store: StoreSection`
  * `clients: BTreeMap<String, ClientConfig>`, `statics: Vec<StaticRoot>`, `routes: Vec<RouteDecl>`, `secrets: BTreeMap<String, SecretEntry>`
  * `locales: Option<LocalesSection>`, `auth: Option<AuthSection>`, `typecheck: Option<TypecheckSection>`, `vendor: Option<VendorSection>`, `build: Option<BuildSection>`, `trace: Option<TraceSection>`, `crawl: Option<CrawlSection>`, `desktop: Option<DesktopSection>`, `pwa: Option<PwaSection>`
  * `files: FilesSection`, `mdx: MdxSection`, `site: Option<SiteSection>`, `sites: Option<SitesSection>`, `dirs: DirsSection`, `images: ImagesSection`, `media: MediaSection`, `fonts: FontsSection`
  * `shell_vendor: Option<PathBuf>`, `favicon: Option<String>`, `public: BTreeMap<String, PublicValue>`, `bundle: Option<Bundle>`
  * `inferred: Vec<String>`, `ignored: Vec<String>`, `deployment: Option<Deployment>`, `undecrypted: Vec<String>`
* `secrets`: the `[secrets.<name>]` tables. A table declared with no keys is included.
* `deployment`: the deployment whose overlays `Loader::config` merged. `None` from `from_store`.
* `undecrypted`: the keys a `keep_undecrypted` loader could not decrypt.
* `public`: `[public]` as written.
* `ignored`: the top-level keys outside `host_sections()`.
* `bundle`: the module graph read from the build facts.
* `shell_vendor`: the shell's `app/vendor/`, when `[site] shell` names a `generated/shell.json` beside one. A host built from this configuration serves it at `/static/js/vendor`. A mount never reads it.
* `favicon`: the static path `/favicon.ico` is answered from, `<icons route>/favicon.ico`, when the icons directory holds that file.
* `Config::load(path: impl AsRef<Path>) -> Result<Config, HostError>`: `Loader::at(path).config()`.
* `Config::from_store_at<S: C5Store>(store: &S, root: impl AsRef<Path>) -> Result<Config, HostError>`: the configuration over a store the caller loaded, with `root` as the project root. Reads no configuration file.
* `Config::from_store<S: C5Store>(store: &S, located: Located) -> Result<Config, HostError>`: `from_store_at` over `located.root`. `located.sources` is kept as `sources` and is the path its errors report.
  * Reads the sections in `host_sections()` and lists any other top-level key in `ignored`.
  * `session` is required.
  * Refuses a `public` value that is not a scalar and a `public` key that is not an identifier.
  * Refuses an `auth.provider` outside `PROVIDERS`, an `auth.login` that is not a path and a `trace.expose` or `trace.inline` entry that is not a path or route pattern.
  * Infers a static root for `dist` at the build facts' `publicPath`.
  * Infers `document.entry` as `<publicPath>src/main.js` when the facts list that entry.
  * Infers `document.import_map` from `importmap.json` and `/static/js/vendor` from `vendor/`, under the site's prefix when `site` is set.
  * Infers `/static/css` and `/static/icons` from the directories `dirs` names.
  * Infers `/favicon.ico` from `favicon.ico` under the icons directory.
  * Infers `/static/css` from `styles/`, with `document.styles` as every `.css` file in it sorted by name, then every `.css` the build facts list under `styles/themes/`.
  * Infers `shell_vendor` as above.
  * Infers each client's `document` as `clients/<name>.openapi.json`.
  * A written value wins over an inference. Every inference is listed in `inferred`.
* `resolve(&self, relative: &str) -> PathBuf`: `relative` joined onto `app`.
* `config_dir(&self) -> PathBuf`: the directory of the first file loaded, the project root when none. `auth.users` resolves here.
* `origin(&self) -> Result<Option<String>, HostError>`: `document.origin` checked to be a scheme and a host with nothing after it.
* `trusted_origins(&self) -> Vec<String>`: `server.trusted_origins`, then each sign-in issuer that posts its callback back.
* `session_ttl(&self) -> Result<Duration, HostError>`
* `cache_ttl(&self) -> Result<Option<Duration>, HostError>`: `None` without a `[cache]` section. An unparsable lifetime is `HostError::Value("cache.ttl", ..)`.
* `dev(&self) -> bool`: `server.dev` when written, else whether `deployment` is `development`. `Deployment::from_env()` when `deployment` is `None`.
* `sent_secrets(&self) -> Vec<&str>`: the secrets named in the `credential` of every client that is not a mock, sorted.
* `Config::default_runtime_keys() -> &'static [&'static str]`: `["session.key"]`, the keys `runtime_values` always reports on.
* `runtime_values(&self) -> Vec<RuntimeValue>`: the values a deployment must hold that its files may leave to the environment, each with its state.
  * `session.key` is `Encrypted` when listed in `undecrypted`, `Placeholder` when it is `SESSION_KEY_PLACEHOLDER`, `Missing` when absent outside development and `Set` otherwise.
  * Then `secrets.<name>.value` for each secret `sent_secrets` names: `Set`, `Encrypted` or `Missing`, in every deployment.
  * The host refuses a placeholder key at boot.

### RuntimeValue

* `pub struct config::RuntimeValue { pub key: String, pub env: String, pub state: ValueState }`, `#[non_exhaustive]`.
* `RuntimeValue::new(key: impl Into<String>, state: ValueState) -> Self`: `env` is `env_name(key)`.
* `pub enum config::ValueState { Set, Placeholder, Encrypted, Missing }`, `#[non_exhaustive]`.

### config_schema

* `pub fn config::config_schema() -> serde_json::Value`: the merged configuration as a JSON Schema (draft 2020-12).
* One property per section in `host_sections()`, typed from the type the host reads it into. `additionalProperties: true` at the top level. Each table refuses a key its type does not declare.
* A value the environment may set carries `x-runtime` with its `C5_` variable. The host never reads the schema.

### env_name

* `pub fn config::env_name(key: &str) -> String`: the `C5_` variable that sets `key`, each part uppercased and joined with `__`. `session.key` is `C5_SESSION__KEY`.

### host_sections

* `pub fn config::host_sections() -> &'static [&'static str]`: the top-level sections the host reads.
* `app`, `server`, `document`, `session`, `cache`, `store`, `clients`, `static`, `route`, `locales`, `auth`, `typecheck`, `vendor`, `build`, `trace`, `crawl`, `desktop`, `pwa`, `files`, `mdx`, `site`, `sites`, `public`, `dirs`, `images`, `media`, `fonts` and `secrets`.

### AppSection

`[app]`.

* `pub struct config::AppSection { pub dir: String, pub runs: Runs }`, `#[non_exhaustive]`.
* `dir` (default `app`): the app directory, relative to the project root.
* `runs: Runs` (default `web`). `Config::runs` carries it.
* `pub enum config::Runs { Web, Terminal }`: `web` serves pages to a browser or a desktop window. `terminal` is a program whose root page `fsr` draws in the terminal. The host reads nothing of a `terminal` application.

### ServerConfig

`[server]`.

* `pub struct config::ServerConfig`, `#[non_exhaustive]`, with the fields below.
* `listen: String` (default `127.0.0.1:8080`).
* `plan: String` (default `generated/plan.sexp`).
* `contracts: String` (default `generated/contracts`): a directory whose `*.json` files are merged in name order at boot.
* `prerender: Option<String>`: the directory, relative to the app, that `prerender` writes and the host reads. Absent by default.
* `dev: Option<bool>`: whether the document carries the live-refresh script and the host answers `/__fsr/events` and `/__fsr/changed`. Absent, it follows `RELEASE_ENV`.
* `render: Option<String>`: any value is `HostError::Config` naming the plan.
* `max_body: usize` (default 1048576): the most bytes a request body may carry. `serve` stops reading a larger body and answers 413 with a text naming the key. `handle` answers the same for a body handed to it whole, before a session is opened.
* `max_upload: usize` (default 0, no cap beyond `max_body`): the most bytes one part of a `multipart/form-data` body may carry. A larger part answers 413 with a `{kind, message}` body naming the field and the setting, before the CSRF token is read and before the action runs.
* `hosts: Vec<String>` (default empty): the hosts a request's `Host` is matched against, which a body reads as `ctx.host`. Compared lowercased and whole, port included. Empty, the header is never read and `ctx.host` is null.
* `request_headers: Vec<String>` (default empty): the headers a middleware reads as `request.headers`, by lowercase name. Any other header never reaches a body.
* `static_max_age: Option<u64>` (default unset): how long, in seconds, a browser may reuse a file from a `[[static]]` root whose name carries no content hash.
  * Unset sends `Cache-Control: no-cache` and answers 304 while the file is unchanged.
  * A number sends `public, max-age=<n>`. `0` sends no `Cache-Control`.
  * Ignored under `dev`, where every static answer is `no-cache`.
  * Never applies to a module or stylesheet of the application's bundle or a module of the embedded client. See [Bundle](#bundle).
* `http2: bool` (default false): whether a served connection negotiates HTTP/2 as well as HTTP/1.1. Without `[server.tls]` this is h2c, which a browser does not use. `HostBuilder::http2` overrides it.
* `trusted_origins: Vec<String>` (default empty): origins beside this site's own a write may come from, each `scheme://host[:port]`.
* `shutdown_grace: u64` (default 20): seconds `serve_listener` waits after SIGTERM or SIGINT for open connections to finish. It stops accepting at once and closes idle keep-alive connections at once. Connections still open at the end are cut and logged.
* `health: String` (default `/health`) and `ready: String` (default `/ready`): the liveness and readiness probe paths.
  * A GET or HEAD on either answers 200 `ok` with `Cache-Control: no-store`, before statics, middleware and sessions.
  * `ready` answers 503 `shutting down` once `Host::begin_drain` has been called.
  * An empty string turns a probe off.
  * A plan route or mounted site at the same path is `HostError::Config` at boot.
* `tls: Option<TlsSection>`: `[server.tls]`. Absent, the listener is plain TCP. Present on a host built without the `tls` feature, `build` is `HostError::Config`.

### TlsSection

`[server.tls]`.

* `pub struct config::TlsSection { pub dir: Option<String>, pub cert: String, pub key: String, pub alpn: Option<Vec<String>>, pub reload: String }`, `#[non_exhaustive]`.
* `dir`: the directory `cert` and `key` are read under. Absent, each resolves against the project root. An absolute path is taken as written.
* `cert` (default `cert.pem`): the certificate chain in PEM, leaf first.
* `key` (default `key.pem`): the private key in PEM, as PKCS#8, PKCS#1 or SEC1.
* `alpn`: the protocols the handshake offers, in order. Absent, `["h2", "http/1.1"]` with `server.http2` on and `["http/1.1"]` without.
* `reload` (default `hup`): the signal that re-reads both files, one of `hup`, `usr1`, `usr2` or `none`. Any other value is `HostError::Config`. Unix only.
* `files(&self, root: &Path) -> (PathBuf, PathBuf)`: the certificate and key paths as resolved.

### DocumentConfig

`[document]`.

* `pub struct config::DocumentConfig`, with the fields below.
* `title: Text` (default empty).
* `title_template: Option<Text>`: how a route's title is written, with `{page}` the route's title and `{title}` the document's. Absent, the route's title is used as given.
* `entry: Option<String>`, `import_map: Option<String>` and `styles: Option<Vec<String>>`: the entry module, the import map and the stylesheet URLs linked in order. Each is inferred when absent.
* `shell: String` (default `shell#document`).
* `origin: Option<String>`.
* `module_preload: bool` (default false): whether the head carries a `<link rel="modulepreload">` for every module the page fetches before the first island can mount.
  * That is `entry`'s static imports from `Config::bundle`, the import map's answer for each bare specifier the bundle declares and what the embedded client reaches from those.
  * A module loaded with `import()` and `entry` itself are left out.
  * A mounted site adds its own links beside its entry script, minus those the shell already has.
* `client: ClientBuild` (default `auto`): which build of the embedded client `/static/js/fsr/<name>.js` answers with.
  * While the host answers that prefix, the document's import map carries each of `client::import_entries()` that the bundle's `externals` name, wherever the application's map has no entry of its own.
  * Without the bundle's facts it carries all of them.
* `empty_icon: bool` (default true): when nothing under `icons/` gives the head an icon, the head gains `<link rel="icon" href="data:,">`.
  * The document drops it when a route's `meta` names another icon.
  * With a `csp` or `csp_report_only`, `data:` is added to `img-src` through `Csp::widen`.
* `csp: Option<Csp>`: `[document.csp]`, the `Content-Security-Policy` every HTML response carries.
  * The host adds the inline import map's hash and the capture script's hash to `script-src`.
  * When the policy restricts scripts, each live HTML response gets a fresh nonce. The development refresh script carries it and a page or layout gets it as its `csp_nonce` prop.
  * A prerendered document and a preview carry no nonce.
  * The hash of each `<style>` with literal content a lowered component writes goes into `style-src`.
  * Each of these goes in through `Csp::admit_inline`. A directive that allows `'unsafe-inline'` gains none.
  * A policy with `style-src` or `default-src` and no `style-src-attr` gains `style-src-attr 'unsafe-inline'`. A declared `style-src-attr` is left as written.
  * Absent, no policy is sent.
  * A payload and a fragment carry no policy.
* `csp_report_only: Option<Csp>`: `[document.csp_report_only]`, composed the same way and sent as `Content-Security-Policy-Report-Only`. Both may be set at once.
* `head: Vec<BTreeMap<String, String>>`: not a key. It holds what the host inferred from `icons/`, which a route's `meta` folds over.
* `head_meta(&self) -> Result<Meta, HostError>`: `head` as the outermost `Meta`.

### Text

Text the document carries: a literal or a message key with the literal it falls back to.

* `pub enum config::Text { Literal(String), Keyed { key: String, default: String } }`. `Debug`, `Clone`, `PartialEq`, `Eq`.
* `Default` is an empty literal. `Deserialize` takes a string or a table with `key` and `default`.
* `literal(&self) -> &str`: the text itself or what a key falls back to.
* `is_keyed(&self) -> bool`
* `resolve(&self, catalogs: &Catalogs, locale: &str) -> String`: the text under `locale`. A key reads `catalogs`, which falls back to the default locale's table, then to the literal.

### Csp

* `pub struct config::Csp(pub BTreeMap<String, Vec<String>>)`: one directive per key with its sources, deserialised from `[document.csp]`.
* `add(&mut self, directive: &str, source: impl Into<String>)`: adds a source once.
* `widen(&mut self, directive: &str, source: impl Into<String>)`: adds a source without narrowing the directive. A directive left to `default-src` starts from `default-src`'s sources. A policy naming neither is left alone.
* `admit_inline(&mut self, directive: &str, hash: impl Into<String>)`: `widen` with the hash of inline content, unless the directive allows `'unsafe-inline'`. An unset directive is read through `default-src`.
* `declares(&self, directive: &str) -> bool`: whether the policy has the directive.
* `names(&self, directive: &str, source: &str) -> bool`
* `header(&self) -> Option<String>`: the header value, `directive source source; directive source`. A directive with no sources is written bare. `None` when the policy is empty.

### ClientBuild

* `pub enum config::ClientBuild { Auto, Readable, Minified }`: `document.client` as `"auto"`, `"readable"` or `"minified"`.
* The choice applies to the module a page's entry imports. The minified modules import their siblings as `.min.js`, which the host answers in either build.
* `minified(self, dev: bool) -> bool`: `Auto` is minified unless `dev`. The other two ignore `dev`. Always true in a binary built without `client_readable`.

### Bundle

The bundle's own facts, read from `dist/.snapfire-build.json`.

* `pub struct config::Bundle { pub route: String, pub graph: BTreeMap<String, Vec<String>>, pub externals: Vec<String>, pub minified: Option<String>, pub outputs: BTreeSet<String>, pub styles: Vec<String>, pub dir: PathBuf, .. }`. `None` on `Config` when no facts file was read.
* `route`: the URL prefix the bundle is served under, with no trailing slash.
* `graph`: each entry's transitive static imports, as paths in the output directory.
* `externals`: the bare specifiers the bundle imports.
* `minified`: the suffix of each output's minified twin, when the bundle was built minified.
* `outputs`: every file the compiler wrote.
* `styles`: the stylesheets of the Vue and Svelte components it compiled.
* `dir`: the directory the outputs are in. That is the `[[static]]` root that serves `route` when one does, else the directory the facts file was read from.
* `minified_url(&self, url: &str) -> String`: `url`'s minified twin when `url` is under `route` and `outputs` holds the twin, else `url`.
* `hashed(&self) -> &BTreeMap<String, String>`: each `.js` and `.css` output against its name carrying the first eight hex digits of its SHA-256, `src/main.3f2a91c0.js` for `src/main.js`. Read from `dir` on the first call. An output that cannot be read is left out.
* `served_url(&self, url: &str) -> String`: `url`'s minified twin, then its hashed name. Outside development the host names every bundle URL this way: the entry, the stylesheets, the preload links and the import map's values.
* `hashed_modules(&self) -> Vec<(String, String)>`: each served module's own URL against its hashed one. Outside development the host adds these to the import map.
* Outside development a hashed name answers with `Cache-Control: public, max-age=31536000, immutable`. A hashed name the bundle no longer holds is 404. The output's plain name answers `no-cache` whatever `static_max_age` says.
* `pub fn config::is_module_extension(ext: &str) -> bool`: whether the compiler writes a module under `ext`, `js` or a framework component's own extension.

### PublicValue

* `pub enum config::PublicValue { Str(String), Int(i64), Float(f64), Bool(bool) }`: one `[public]` value as written.
* `ts(&self) -> &'static str`: the TypeScript type a body sees.
* `to_value(&self) -> Value`: what `ctx.config` carries.
* `Display`: the value as the boot report prints it.

### SessionSection

`[session]`. The section may be left out. Every field then takes its default.

* `pub struct config::SessionSection`, with the fields below.
* `key: Option<String>`: the signing key.
  * Absent, `HostBuilder::build` generates one on a development host. Any other host that builds its own key ring refuses with `HostError::Config`.
  * `config::SESSION_KEY_PLACEHOLDER` (`replace-this-with-a-generated-key`) is refused on any host.
  * A key written but empty, such as a `.c5encval` that did not decrypt, refuses the load.
* `previous_keys: Vec<String>` (default empty): keys that still verify a cookie signed before `key` replaced them, in order.
* `key` and `previous_keys` fill the `Keyring` the stock codec and a `derived` CSRF scheme sign with, `key` first.
* `store: String` (default `memory`): `memory` or `service`. `service` keeps every record behind the client `client` names, through `ServiceSessionStore`. Any other value is `HostError::Value`.
* `client: Option<String>`: required with `service`. It must be a `[clients]` entry, else `HostError::Config`.
* `ttl: String` (default `8h`): the length of a new session. Only `SessionCell::extend` moves the end.
* `capacity: u64` (default 4096).
* `secure: bool` (default false).
* `max_bytes: u64` (default 16384): about the most one session may hold. A write that would go over is refused.
* `csrf: String` (default `origin`): when a CSRF token is minted and verified, beside the origin check every write passes. Any other value is `HostError::Config`.
  * `origin`: no token.
  * `identified`: a token once the session is identified.
  * `always`: a token for any session whose page renders it. The response that rendered it establishes a fresh session. A page rendering no token sets no cookie.
* `csrf_scheme: String` (default `single_use`): `single_use`, `session` or `derived`. Any other value is `HostError::Config`.
* `csrf_outstanding: u32` (default 8, at least 1): under `single_use`, how many tokens stay valid at once.

### CacheSection

`[cache]`. Absent, nothing is cached.

* `pub struct config::CacheSection { pub capacity: u64, pub ttl: String, pub data: Option<DataCacheSection> }`.
* `capacity` (default 1000) and `ttl` (default `1m`). When the section is present, `build` installs `FibreCache::bounded(capacity, ttl)` on the app.
* `data`: present and not `enabled = false`, `build` installs the service layer's `DataCache` over every method whose contract declares `cache`. Absent, no method is cached.
* `pub fn data_cache(&self) -> Option<&DataCacheSection>`: `data` when it turns the data cache on.

### DataCacheSection

`[cache.data]`.

* `pub struct config::DataCacheSection { pub enabled: Option<bool>, pub capacity: Option<u64> }`.
* `enabled`: `false` keeps the data cache off with the section written. Any key turns it on.
* `capacity`: entries per policy, `cache.capacity` when absent.
* A `[cache.data]` with no keys is `HostError::Config` from `Config::load` and `Config::from_store`. `Config::from_store_at` cannot see it and reads it as absent.

### StoreSection

`[store]`.

* `pub struct config::StoreSection { pub slot_order: Vec<String> }`.
* `slot_order` (default empty): the slot names that settle a store key two parallel slots at one depth both seed. The later name wins and `content` stands for the page.
* A slot the list leaves out loses to every slot it names and is ordered by name among the others. Empty, name order decides throughout.
* The document carries it as `data-sf-slot-order`, a JSON array. The browser store merges by it.
* An empty name or a name listed twice is `HostError::Config`.
* A mounted site's own `[store]` is not read.

### ClientConfig

`[clients.<name>]`. The table key is the service name.

* `pub struct config::ClientConfig { pub document: Option<String>, pub base_url: Option<String>, pub transport: Option<String>, pub responses: Option<String>, pub bearer: Option<BearerKey>, pub credential: Option<CredentialConfig>, pub user_agent: Option<String>, pub timeout: Option<String> }`.
* `document`: inferred as `clients/<name>.openapi.json` when absent, else `clients/<name>.proto` or `clients/<name>.graphql` when only that file exists.
* A `.proto` document is reached with `GrpcTransport`, a `.graphql` one with `GraphqlTransport` posting to `base_url` and any other with `HttpTransport`.
* `base_url`: required unless the transport is `mock`.
* `transport`: `mock` answers from `responses` over a `MockTransport` and reaches nothing. Any other value is `HostError::Value`.
* `responses`: the mock's file relative to the app directory, `clients/<name>.mock.json` when absent. It is an object of method name to a response in the payload's JSON encoding. A response may instead be `{"$fail": {"kind": "<FailureKind>", "message": "..."}}`.
* `bearer`: which custody entry the client's calls carry as a bearer token. Absent, none.
* `credential`: an application credential sent on every call. A mock client sends nothing.
* `user_agent`: the `User-Agent` an HTTP client's calls send. Absent, `snapfire_fsr_service::HTTP_USER_AGENT`.
* `timeout`: how long an HTTP client's call waits for the whole answer, a duration string. Absent, 30 seconds. One that is not a duration is `HostError::Config` naming `clients.<name>.timeout`.
* `user_agent` or `timeout` on a `.proto` or `.graphql` client is `HostError::Config`.
* `pub struct config::CredentialConfig { pub secret: String, pub header: Option<String>, pub query: Option<String> }`.
  * `secret` names a `[secrets.<name>]`.
  * Without `header` or `query` the value is sent as `authorization: Bearer <value>`. With `header` it is that header's value. With `query` it is that query parameter.
  * A `.proto` client sends it as call metadata.
  * Loading refuses a credential naming an undeclared secret, one with both `header` and `query`, `query` on a `.proto` client and an `authorization` credential on a client with `bearer` set.
* `is_mock(&self) -> bool`
* `responses_file(&self, name: &str) -> String`

### SecretEntry

`[secrets.<name>]`.

* `pub struct config::SecretEntry { pub description: Option<String>, pub value: Option<Secret> }`.
* The value comes from `C5_SECRETS__<NAME>__VALUE` or a `.c5encval`. Written as a plain TOML string, it refuses to load outside development, naming the variable.
* A value that does not decrypt refuses to load, unless `Loader::keep_undecrypted` lists `secrets.<name>.value` in `Config::undecrypted`.
* `pub struct config::Secret`: `Secret::new(value: impl Into<String>) -> Secret`, `expose(&self) -> &str`. `Debug` prints `Secret(***)`.
* The host refuses to start when a secret `Config::sent_secrets` names has no value, naming its variable.

### StaticRoot

`[[static]]`.

* `pub struct config::StaticRoot { pub route: String, pub dir: String }`: `dir` is relative to the app directory.
* A trailing slash on `route` is ignored.
* A written root with the same `route` as an inferred one replaces it.
* A root at `/` answers only the paths that name a file under `dir`. Every other path goes on to the routes.

### RouteDecl

`[[route]]`.

* `pub struct config::RouteDecl { pub pattern: String }`: a pattern the application registers with `HostBuilder::route`.
* A declared pattern nothing registers is listed in `HostReport::unanswered_routes`.

### LocalesSection

`[locales]`.

* `pub struct locale::LocalesSection { pub supported: Vec<String>, pub default: Option<String>, pub order: Vec<String>, pub remember: bool, pub cookie: String }`, `#[non_exhaustive]`.
* `LocalesSection::new(supported: Vec<String>) -> Self`: every other field at its default.
* `supported`: at least one tag of letters, digits, `_` and `-`, spelled as the application uses it.
* `default`: one of `supported`, the first when absent.
* `order`: the sources consulted, any of `prefix`, `cookie` and `header`. All three in that order when absent.
* `remember` (default false). `cookie` (default `sf_locale`).
* Checked at `build`, which fails with `HostError::Config` naming the problem.

### AuthSection

`[auth]`. Needs the `auth` feature. Without it a configuration with `[auth]` makes `build` a `HostError::Config` naming the feature. An `oidc` or `passkey` method without its own feature does the same.

* `pub struct config::AuthSection { pub provider: Option<String>, pub login: String, pub users: Option<String>, pub client: Option<String>, pub methods: Vec<MethodSection>, pub second_factor: Option<String>, .. }`. For a single method it also carries every `MethodSection` field from `issuer` to `verification`.
* One method is `provider` with its `users` or `client`. Several are `[[auth.methods]]`. Naming both or neither is refused.
* `provider`: one of `config::PROVIDERS`, `["file", "service", "oidc", "passkey"]`.
* `login` (default `/login`): the application's login page. It must start with `/`.
* `users` (default `auth.toml`): the `file` provider's table, relative to `config_dir`. It is read on its own, not merged with the overlays.
* `client`: the `[clients]` entry a `service` provider sends its steps to. Required with `service` and checked against the table.
* `second_factor`: one of two or more methods that every sign-in through the others must also pass. Built as `Sequence` over `Methods` of the rest. Without it several methods are `Methods` in their order.
* `all_methods(&self) -> Vec<MethodSection>`: every method. A single `provider` is one method named after itself. An OIDC method is named after its `preset` or `oidc`.

### MethodSection

One `[[auth.methods]]` entry.

* `pub struct config::MethodSection { pub name: String, pub provider: String, pub users: Option<String>, pub client: Option<String>, pub issuer: Option<String>, pub preset: Option<String>, pub tenant: Option<String>, pub client_id: Option<String>, pub client_secret: Option<String>, pub scopes: Option<Vec<String>>, pub claims: BTreeMap<String, String>, pub team_id: Option<String>, pub key_id: Option<String>, pub signing_key: Option<String>, pub store: Option<String>, pub rp_name: Option<String>, pub verification: Option<String> }`.
* `name`: what `/auth/login?method=` names. A name used twice is refused.
* An `oidc` method needs `client_id`. It also needs `issuer` or a `preset` of `config::PRESETS`, `["google", "microsoft", "apple"]`.
  * `apple` also needs `team_id`, `key_id` and `signing_key`.
  * `client_secret` and `signing_key` name `[secrets]` entries, checked against the table.
  * The provider is `snapfire_fsr_oidc::OidcProvider` with redirect URI `<document.origin>/auth/callback`. In development without an origin it is `http://<server.listen>`.
  * `microsoft` uses `tenant`, `common` when absent. `scopes` overrides the scopes and `claims` adds renamed claims.
  * In development, while a named secret has no value, the method is `DevProvider::stand_in` over its `users` table. The boot report names it `oidc <preset or issuer>, a stand-in until secrets.<name> has a value`.
  * Outside development a secret with no value is a boot error naming `C5_SECRETS__<NAME>__VALUE`. So is a missing `document.origin`.
* A `passkey` method is `snapfire_fsr_passkey::Passkeys` over `FilePasskeys` at `passkeys.json` inside `store`, a directory relative to `Config::root`.
  * Loading refuses a passkey method without `store`, a `store` whose first component is `config` and a section where every method is a passkey.
  * The relying party is `document.origin`'s host and the origin itself. In development without an origin it is `localhost` and `http://localhost:<port of server.listen>`. Outside development a missing origin is a boot error.
  * `rp_name`: `document.title` when absent, else the RP ID.
  * `verification`: `required` or `preferred`. Any other value is refused.
  * The boot report names it `passkey for <rp id>`.

### BearerKey

* `pub enum config::BearerKey { Toggle(bool), Named(String) }`, untagged: `true`, `false` or a string in the file.
* `key(&self) -> Option<&str>`: `access_token` for `true`, the string for `Named` and `None` for `false`.

### TypecheckSection

`[typecheck]`. Read by `fsr` when it runs `snapfiretc`. The host reads none of it.

* `pub struct config::TypecheckSection { pub version: Option<String>, pub sha512: Option<String>, pub tsc: Option<String>, pub enabled: Option<bool> }`.
* `version`: the TypeScript version a build checks with, the checker's default when absent. `fsr` writes the version it resolved when the file names none.
* `sha512`: the integrity a fetched version must match when the checker pins no hash for it.
* `tsc`: a compiler to use as given.
* `enabled`: whether a build checks types, on when absent.

### VendorSection

`[vendor]`. Read by `fsr add` and `fsr use`. The host reads none of it.

* `pub struct config::VendorSection { pub hosts: Vec<String> }`.
* `hosts`: the hosts a package is fetched from, tried in order. Each answers the esm.sh API. esm.sh alone when absent or empty.

### BuildSection

`[build]`. Read by `fsr`. The host reads none of it.

* `pub struct config::BuildSection { pub strict: Option<bool>, pub target: Option<String>, pub lib: Option<Vec<String>>, pub before: Vec<String>, pub after: Vec<String> }`.
* `before` and `after`: Cargo projects, relative to the project root, that `fsr build` and `fsr dev` run in order before the app build and after it. An empty project name refuses the configuration.
* `strict`: refuses a build that leaves a component to render in the browser alone. Off when absent.
* `target` and `lib`: what every tsconfig the build writes carries. `config::DEFAULT_TARGET` (`es2022`) and the target's own library when absent.
* Refused: a target older than `es2017`, a target other than `esnext` or `es` followed by a four-digit edition and an empty `lib` entry.

### TraceSection

`[trace]`.

* `pub struct config::TraceSection { pub expose: Vec<String>, pub inline: Vec<String> }`. Each is empty when not written.
* `expose`: the paths whose responses carry an `x-sf-request` token that `GET /__fsr/trace/{token}` reads back.
* `inline`: the paths whose HTML pages end with their own trace.
* Entries are `matchit` route patterns: `/` is the home page alone and `/docs/{*rest}` is every path under `/docs`.

### CrawlSection

`[crawl]`.

* `pub struct config::CrawlSection { pub robots: Option<String>, pub sitemap: Option<String>, pub disallow: Vec<String> }`.
* `robots` and `sitemap`: the paths each file is served at. Absent, that file is not served.
* `disallow`: the paths `robots.txt` disallows beside `/__fsr/`, `/_sf/` and, when `[auth]` is set, `/auth/`.
* Loading refuses a value that is not a path, a section naming neither file and a section without `document.origin`.
* On a host with `[site]` the sitemap is served under the prefix and `robots` is not served.

### DesktopSection

`[desktop]`. Read by `fsr --desktop`. The host reads none of it.

* `pub struct config::DesktopSection { pub name: Option<String>, pub serve: DesktopServe, pub cookies: DesktopCookies }`.
* `name`: the start of the window title, before the page's own title.
* `pub enum config::DesktopServe { Loopback, Scheme }`: `"loopback"` (default) or `"scheme"`.
* `pub enum config::DesktopCookies { Memory, Disk }`: `"memory"` (default) or `"disk"`.
* Loading refuses any other value.

### PwaSection

`[pwa]`.

* `pub struct config::PwaSection { pub name: Option<String>, pub short_name: Option<String>, pub description: Option<String>, pub theme_color: Option<String>, pub background_color: Option<String>, pub display: Option<String>, pub start_url: Option<String>, pub file: Option<String>, pub worker: Option<String>, pub offline: Option<String> }`.
* Loading refuses `file` beside any manifest field and a section with neither `file` nor `name`.
* `/manifest.webmanifest` answers `application/manifest+json`: the file `file` names, else a manifest from the fields.
  * `start_url` defaults to `/` and `display` to `standalone`.
  * Its `icons` are each PNG, WebP or SVG in the icons directory whose name carries `<w>x<h>`, smallest first. An SVG without a size is `any`.
* `/sw.js` answers outside development only: the file `worker` names, else a worker the host writes at boot.
  * It precaches the bundle's hashed names as `sf-build-<version>` and keeps the previous build's set.
  * It warms the client, the import map's local URLs, the document's other stylesheets, the manifest and `offline` into `sf-runtime`.
  * It answers the bundle cache first and every other same-origin GET network first. It keeps a response that is not `private` or `no-store`. It also keeps the document of a kept payload.
  * It leaves `/_sf/`, `/__fsr/` and non-GET requests alone.
* Both routes send `Cache-Control: no-cache`.
* Every page carries `<link rel="manifest" href="/manifest.webmanifest">` in place of any inferred manifest link.
* Every page carries `<meta name="theme-color">` from `theme_color`, unless a head row sets one.
* Outside development every page carries `<meta name="sf:worker" content="/sw.js">`.

### FilesSection

`[files]`.

* `pub struct config::FilesSection { pub roots: Vec<String> }`: the directories a body's `file` calls read and write, each relative to the app directory or absolute.
* The builder gives the app `snapfire_fsr_core::ext::FileRoots` over `config.app` and these roots. A relative path resolves against the app directory.
* A path outside every root, symbolic links followed, is refused naming `[files] roots`.
* Empty, `file` reaches nothing.

### MdxSection

`[mdx]`. Read by `fsr`. The host reads none of it.

* `pub struct config::MdxSection { pub gfm: bool, pub heading_ids: bool, pub frontmatter: bool, pub highlight: Option<bool>, pub collections: BTreeMap<String, String> }`.
* `Default`: `gfm` and `heading_ids` true, `frontmatter` false, `highlight` absent and no collection.
* Loading refuses a key outside these five, a flag that is not `true` or `false` and a `[mdx.collections]` value that is not a directory under the app, is absolute or reaches `..`.

### SiteSection

`[site]`: this application as a site another host mounts.

* `pub struct SiteSection { pub name: String, pub at: String, pub shell: Option<String>, pub probe: Vec<Probe> }`. `Deserialize`, `Clone`, `PartialEq`, `Eq`.
* `name`: lowercase letters, digits, `_` and `-`.
* `at`: a path with no trailing slash and no parameter.
* `shell`: the path, against the project root, of the shell contract the site's build reads.
* `probe`: the paths a shell renders against a candidate holding this site before it swaps it in. See [Probe](#probe).
* `prefix(&self) -> String`: `<name>:`.
* `under(&self, path: &str) -> String`: `at` joined with `path`. `/` is `at` itself.
* An application with both `[site]` and `[sites]` is refused.
* Its `styles/` root is inferred under `<at>/static/css`.

### SitesSection

`[sites]`: the sites a shell mounts.

* `pub struct SitesSection { pub root: Option<String>, pub poll: Option<String>, pub store: Option<String>, pub mounts: BTreeMap<String, MountConfig> }`, `#[non_exhaustive]`.
* `root`: where `name@version` artifacts resolve. Required by a mount spelled that way and by `store`.
* `poll`: a duration. Absent, nothing polls.
* `store`: a directory of archives or an `http(s)` URL that `snapfire_fsr_sites` fetches a missing version from.
* `root`, `poll` and `store` cannot be mount names.

### MountConfig

`[sites.<name>]`.

* `pub struct MountConfig { pub artifact: Option<String>, pub hash: Option<String>, pub allow_engine: bool }`, `#[non_exhaustive]`.
* `MountConfig::new(artifact: Option<String>, hash: Option<String>, allow_engine: bool) -> Self`.
* `artifact`: `name@version` under the root or a path against the project root.
* `hash`: pins the content hash.
* `allow_engine`: admits an artifact with engine-owned rows.
* A row without `artifact` or with any other key is refused at load.

### DirsSection

`[dirs]`: where each kind of asset lives under the app directory. The routes stay fixed whatever directory a key names.

* `pub struct config::DirsSection { pub styles: String, pub fonts: String, pub images: String, pub icons: String, pub public: String }`, `#[non_exhaustive]`. Each defaults to its own name.
* `styles`, `fonts` and `icons` are scanned from the directory the key names.
* `images` is the directory `fsr build` scans into the asset manifest for a template's `fsr_picture`.
* `public`: when the directory exists and the configuration is not a site's, it is inferred as a static root at `/`. It is not inferred when a written root is already at `/` or already serves the directory.
* A static root at `/` answers a path only when it names a file under the directory, read percent-decoded with no `.` or `..` component. It is tried after every other root.

### MediaSection

`[media]`.

* `pub struct config::MediaSection { pub embed_stills: EmbedStills }`, `#[non_exhaustive]`.
* `pub enum config::EmbedStills { Provider, Local }`, `provider` by default.
* `provider`: an `Embed`'s still loads from the provider's CDN.
* `local`: the build fetches each still once into `generated/embeds/` and the page serves it.
* A placeholder with no still, because the fetch failed or the `id` is computed, shows the title alone. The build report names it.

### ImagesSection

`[images]`: the variant policy every image is built under and how a remote image is addressed.

* `pub struct config::ImagesSection { pub widths: Vec<u32>, pub formats: Vec<String>, pub quality: BTreeMap<String, u8>, pub rewrite: bool, pub strip: bool, pub base: Option<String>, pub sources: BTreeMap<String, ImageSource> }`, `#[non_exhaustive]`.
* `widths` (default `[640, 960, 1280, 1920, 2560]`): each at least 1.
* `formats` (default `["avif", "webp"]`): each `avif` or `webp`.
* `quality`: keyed by format, 0 to 100. A format left out keeps 60 for AVIF and 80 for WebP.
* `rewrite` (default `true`): whether an `<img>` whose `src` is an imported asset is lowered as a `Picture`.
* `strip` (default `true`): whether the served original of a JPEG, PNG, WebP or GIF is `snapfire_media::strip::strip` of the source. It keeps the EXIF orientation, creator, copyright and the ICC profile. Off, the original is served as saved.
* `base`: `scheme://host/path` with no trailing slash, prefixed onto every image URL at build time.
* `sources`: `[images.sources.<name>] template = "…"`, a URL template a string `src` goes through. It must carry `{src}` and may carry `{width}`.
* `pub struct config::ImageSource { pub template: String }`.
* Each rule above is refused at load with the key named.

### FontsSection

`[fonts]`.

* `pub struct config::FontsSection { pub base: Option<String>, pub faces: BTreeMap<String, FontEntry> }`, `#[non_exhaustive]`.
* `base`: where font files are served from when a CDN answers them, `scheme://host/path` with no trailing slash.
* `faces`: one `[fonts.<key>]` per family. A key is lowercase letters, digits and `-`. It names `--font-<key>`.
* A family the fonts directory holds and no key names is inferred from the files' name tables, under the key its family name slugs to.

### FontEntry

One `[fonts.<key>]`. Every field is optional. An empty table is not read.

* `pub struct config::FontEntry { pub family: Option<String>, pub files: Vec<String>, pub fallback: Option<String>, pub display: Option<String>, pub preload: Option<bool>, pub remote: Option<String>, pub variable: Option<String>, pub scripts: Vec<String> }`, `#[non_exhaustive]`.
* `family`: the family name, in place of what the files' tables say.
* `files`: the files under `[dirs] fonts` the key covers. Empty, every file whose family matches.
* `fallback`: the system face the fallback is sized against, one of `snapfire_fsr_assets::font::fallbacks`. `Arial` when absent.
* `display`: `font-display`, `swap` when absent.
* `preload`: whether the faces are preloaded. Absent, the regular weight only.
* `remote`: a provider's stylesheet URL, which the host links and preconnects to. It needs `family` and excludes `files`.
* `variable`: the CSS variable the family is defined as. It must start with `--`. `--font-<key>` when absent.
* `scripts`: the Unicode scripts a card image draws with these faces before any other. A name Unicode does not have refuses the boot of a host with card images.

### parse_duration

* `pub fn config::parse_duration(raw: &str) -> Option<Duration>`: `<n>ms`, `<n>`, `<n>s`, `<n>m`, `<n>h` or `<n>d`. A bare number is seconds. `None` for a value that overflows.

## 2. Building

### Artifact

A built application as the host reads it.

* `pub struct Artifact { pub config: Config, pub plan: String, pub contract: Option<Contract> }`, `#[non_exhaustive]`. `Clone`, `Debug`.
* `Artifact::new(config: Config, plan: String, contract: Option<Contract>) -> Self`: the three fields as given. Nothing is read.
* `Artifact::of(config: Config) -> Result<Artifact, HostError>`: reads the plan file and the contracts directory the configuration names. The contract files are merged one by one with `Contract::merge`. `Io` when the plan does not read.

### Host::from

* `Host::from(loader: impl Into<Loader>) -> Result<HostBuilder, HostError>`: `loader.load()`, then `from_artifact`. The builder keeps the loader for `Host::reload`. A path is `Loader::at(path)`.
* `Host::from_cwd() -> Result<HostBuilder, HostError>`: `Host::from(".")`.
* `Host::from_config(config: Config) -> Result<HostBuilder, HostError>`: `from_artifact(Artifact::of(config)?)`.
* `Host::from_artifact(artifact: Artifact) -> Result<HostBuilder, HostError>`: a builder over an artifact in memory. No loader is kept. `HostError::Config` naming the plan when `server.render` is set.

### HostBuilder

* `services_over(self, transport: Arc<dyn Transport>) -> Self`: every client's calls go to this transport. The contract still comes from the documents.
* `services(self, services: Arc<Services>) -> Self`: a registry built elsewhere, in place of the clients and of any `service`.
* `service<T>(self, service: Arc<T>) -> Self where T: Transport + DeclaredService + 'static`: a `#[service]` block served in process under `T::NAME`, through the same registry, interceptors and data cache a client uses.
  * `T::contract()` is merged with the contracts directory's through `Contract::adopt`. A disagreement fails `build` with `HostError::Service`.
  * `services_over` leaves it in place.
  * The report lists it as a `services` row of kind `rust` with the Rust type's path.
* `session_store(self, store: Arc<dyn SessionStore>) -> Self`
* `csrf(self, scheme: Arc<dyn CsrfScheme>) -> Self`: the CSRF scheme, in place of the one `session.csrf_scheme` names. A builder given one cannot be rebuilt by a sites reload.
* `codec(self, codec: Arc<dyn CookieCodec>) -> Self`: the cookie codec, in place of `HmacCodec` over the configuration's keys. `key` and `previous_keys` then feed only a `derived` scheme. A builder given one cannot be rebuilt by a sites reload.
* `keyring(self, ring: Arc<Keyring>) -> Self`: the ring the stock codec and a `derived` scheme sign with, in place of one built from `key` and `previous_keys`. A reload leaves it alone. A builder given one cannot be rebuilt by a sites reload.
* `http2(self, on: bool) -> Self`: overrides `server.http2`.
* `topics<F>(self, rule: F) -> Self where F: Fn(&str, &SessionCell, Option<&Identity>) -> bool + Send + Sync + 'static`: who may follow which topic on `/_sf/live` and `/_sf/socket`.
  * Asked once per topic when a stream opens, with the session the request's cookie names.
  * One refused topic refuses the whole stream with 403 naming it.
  * Without a rule anyone may follow any topic.
  * `pub type TopicRule = Arc<dyn Fn(&str, &SessionCell, Option<&Identity>) -> bool + Send + Sync>`.
* `sockets(self, sockets: Arc<socket::Sockets>) -> Self`, `ws` feature: the registry to serve from. Without it the host makes its own.
* `socket<F>(self, handler: F) -> Self where F: Fn(&socket::Who, socket::On) -> socket::Reply + Send + Sync + 'static`, `ws` feature: the answer to what a page sends over `/_sf/socket`.
  * Called once when a connection joins a topic, once per row it sends and once when it leaves.
  * Its reply goes out to the topic as store rows.
  * Without one, `/_sf/socket` is 404.
* `shell(self, evaluator: Arc<dyn Evaluator>) -> Self`: the document shell.
* `prerendered(self, dir: impl Into<PathBuf>) -> Self`: where prerendered documents are read from, over `server.prerender`.
* `meta(self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self`: describes the segment whose data source is `name` once its data has loaded. As `AppBuilder::meta`.
* `identity(self, provider: Arc<dyn IdentityProvider>) -> Self`, `auth` feature: the provider behind the `/auth/` routes, in place of the one `[auth]` names. The login page is `auth.login` when the section is written, else `/login`.
* `extension<F>(self, name: impl Into<String>, reach: Reach, f: F) -> Self where F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static`: the Rust half of a native pair, forwarded to `AppBuilder::extension`.
  * `name` is `module.member`, as the `native(..)` declaration under `ext/` spells it.
  * A plan calling a name nothing registers fails `build` with `BindError::UnknownExtension`.
  * `Reach`, `Ambient`, `Fail`, `FailureKind` and `Catalogs` are re-exported from `snapfire_fsr_core::ext` at the crate root. The module itself is re-exported as `ext`.
* `traces(self, traces: Option<trace::Traces>) -> Self`: the collector for `/__fsr/traces` under development and for `/__fsr/trace/{token}` on the paths `trace.expose` lists. Takes what the [collector functions](#collectors) return.
* `island_handler<F, Fut>(self, module: impl Into<String>, name: impl Into<String>, f: F) -> Self where F: Fn(RequestCtx, IslandEvent) -> Fut, Fut: Future<Output = Result<Value, ActionError>>`: one handler of an island a template renders.
  * `module` is what the placement names. `name` is what its markup binds with `data-sf-on="click:<name>"`.
  * The handler answers with the state the module is rendered from next.
  * The boot report lists each as an `islands` row, `<module> <name>`.
* `route`, `route_override`, `not_found`, `handler`, `handler_override`, `middleware`, `middleware_override`, `source`, `source_override`, `source_impl`, `action`, `action_override`, `evaluator`: the `snapfire_fsr::AppBuilder` methods with the same signatures.
* `native(self, name: impl Into<String>, module: Arc<dyn Native>) -> Self`: the application's own Rust, reached from a body as `ctx.native.<name>.<method>()`.
* `bodies(self, bodies: &'static [bodies::Native]) -> Self`: forwarded to `AppBuilder::bodies`, once for the shell's `generated/bodies.rs` and once for each mounted site's.
  * `snapfire_fsr_host::bodies` re-exports `snapfire_fsr_ir::native`, which the generated file is written against.
  * A production `fsr build` writes every loader it can into the file. A development build writes an empty `BODIES`.
* `mount(self, mount: Mount) -> Self`: mounts a site. See [Mount](#mount).
* `config(&self) -> &Config`: the configuration the builder was made from.
* `loader(&self) -> Option<&Loader>`: how the artifact was read. `None` for a builder made from an `Artifact` in memory.
* `reload_check(self, check: Arc<dyn ReloadCheck>) -> Self`: a check every candidate must pass before a checked reload swaps it in, after the host's own probes. The host keeps it across reloads.
* `sites_mounter<F>(self, f: F) -> Self where F: Fn(HostBuilder) -> Result<HostBuilder, HostError> + Send + Sync + 'static`: how the sites are mounted again on a reload. It is handed a builder made from the shell's boot inputs and returns it with the sites mounted.
  * `pub type SitesMounter = Box<dyn Fn(HostBuilder) -> Result<HostBuilder, HostError> + Send + Sync>`.
* `reloader<F>(self, f: F) -> Self where F: Fn() -> Result<HostBuilder, HostError> + Send + Sync + 'static`: how `Host::reload` rebuilds the tables. It returns a builder for the application as it now stands on disk, with whatever this builder was given added again.
  * `pub type Reloader = Box<dyn Fn() -> Result<HostBuilder, HostError> + Send + Sync>`.
* `build(self) -> Result<Host, HostError>`: builds the host. The steps, in order:
  * With the `tera` feature, reads every file `template_files` lists into one `Tera` through `tera::evaluator`, each named by its path under the app. It registers that evaluator for every module `is_template_module` matches, unless an evaluator given to `evaluator` already answers `.tera`.
  * Reads `<app>/locales/*.toml` into the app's catalogs through `locale::load_catalogs`. A file that does not read is `HostError::Config`. So is a message that is not a string, number or boolean.
  * Imports the clients and builds the registry with trace and identity interceptors. Adds one `CredentialInterceptor::bearer(key).only(clients)` per custody key the clients' `bearer` names.
  * Mounts the identity provider. For `file` it is `DevProvider::from_toml`. `HostError::Config` names the file when it cannot be read.
  * Registers the shell for the document module and `NullEvaluator` for the rest, after any evaluators given.
  * Applies the contract and builds the app under the binding rule.
  * Checks every template module a route, intercept or not-found plan names. `Uncovered` when no evaluator answers it. `TemplateMissing` when the stock evaluator does but read no template of that name.
  * Refuses a bundle that carries a server module with `HostError::Leak`.
  * Builds the session layer and the static roots.
  * Everything except the session layer is the host's tables, which `reload` replaces whole.
  * When `LIFELINE` is set in the environment, starts watching stdin.

## 3. The Host

### Host

The running host: the tables a request reads and the sessions, which outlive a reload.

* `pub struct Host { .. }`
* `report(&self) -> Arc<HostReport>`: what the host bound, as of the last reload.
* `listen(&self) -> &str`: the configured address.
* `locales(&self) -> Locales`: the locales the host serves and how it resolves a request's.
* `public(&self) -> ValueMap`: `[public]` as `ctx.config` sees it.
* `catalogs(&self) -> Option<Arc<Catalogs>>`: the message catalogs loaded from `locales/`. `None` when there are none.
* A document embeds the request locale's merged catalog as `<script type="application/json" data-sf-i18n="<tag>">`. A payload carries it as a `D` row, unless the request's `x-sf-catalog` header names that locale.
* `lowered(&self) -> Option<Arc<IrEvaluator>>`: the lowered components and their interpreter. `None` when the plan lowered no component.
* `services(&self) -> Arc<Services>`: the registry the routes call through.
* `owner_of_source(&self, name: &str) -> Option<Owner>`
* `changed(&self)`: tells every open `/__fsr/events` stream that something changed. Does nothing when `dev` is off.
* `failed(&self, message: impl Into<String>)`: tells every open development document that the last build was refused and why. Does nothing when `dev` is off.
* `types(&self, message: impl Into<String>)`: tells every open development document what the typechecker found. An empty message clears it. Does nothing when `dev` is off.
* `publish(&self, topic: impl Into<String>)`: tells every open `/_sf/live` stream watching `topic` that it changed.
* `sockets(&self) -> Arc<socket::Sockets>`, `ws` feature: every open socket, by topic.
* `async fn invalidate(&self, plan_key: &str) -> usize`: drops every cached subtree under the plan `cache_key`, a lowered page's or layout's module name. Returns how many went. Zero without `[cache]`.
* `invalidate_tags<I, S>(&self, tags: I) where I: IntoIterator<Item = S>, S: AsRef<str>`: drops every data cache answer under the named tags. Does nothing without `[cache.data]`.
* `async fn refresh_sitemap(&self) -> Result<(), HostError>`: builds the sitemap again from what the host serves now and swaps it in. Requests meanwhile get the one held. A failed build keeps it and returns `HostError::Sitemap`. `Ok(())` without `[crawl] sitemap`.
* `async fn call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`: `call_action_in` under the default locale.
* `async fn call_action_in(&self, id: &str, session: SessionCell, locale: Locale, input: Value) -> Result<Value, ActionError>`: runs the action with `locale` as `ctx.locale`.
* `async fn call_handler(&self, method: &str, path: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`: the handler matching `method` and `path`, run with `input` as the request body.
  * The locale prefix is stripped and resolved into `ctx.locale`. The query string becomes `ctx.query`.
  * `NotFound` when no handler matches.
* Work an action or handler queued with `after` starts when it returns.
* `async fn auth_call(&self, flow: &AuthFlow, method: &str, path: &str, query: &str, body: &[u8], headers: &[(String, String)], session: SessionCell) -> Option<(u16, Vec<(String, String)>, String)>`, `auth` feature: the three `/auth/` routes against a session cell instead of a cookie.
  * Returns the status, the response headers and the body.
  * `None` when no provider is mounted or the path is not one of the three.
  * The identity the flow settles lands in `session`. The `Set-Cookie` comes back as a header.
  * A journey starts at `GET /auth/login`. A callback with no flow in progress is 400.
* `async fn after_settled(&self)`: resolves once the work every request queued with `after` has finished.
  * `handle` starts a request's work when its response body has ended or been dropped, on the tokio runtime it is polled on. Without a runtime the work is dropped with a warning.
  * `serve_until` waits for it after the connections close, within `server.shutdown_grace`.

### Host::reload

* `reload(&self) -> Result<Arc<HostReport>, HostError>`: rebuilds the tables, checks them as `build` does and swaps them in. A request in flight finishes on the tables it started with. Calls `changed` on success.
  * With a `reloader`, the builder it returns is the rebuild.
  * Without one, the artifact is read again through the loader `Host::from` kept and the `sites_mounter` runs over the builder.
  * `HostError::Value("reload", ..)` when the host was built from an `Artifact` in memory.
  * `HostError::Value("reload", ..)` when its builder was given services, a session store, a CSRF scheme, an evaluator, an identity provider or a prerendered directory.
  * `HostError::Value("reload", ..)` when anything was registered on the app by hand.
  * `HostError::Value("reload", ..)` when a `Mount` was given by hand with no `sites_mounter`.
  * `HostError::Value("session", ..)` when the new `[session]` differs from the running one, keys aside.
  * A changed `key` or `previous_keys` is applied to the running ring with `Keyring::replace` when the host built the ring itself.
* `reload_sites(&self) -> Result<Arc<HostReport>, HostError>`: rebuilds the tables with the sites read again and the shell exactly as it booted. Only the artifacts the `[sites]` table names are read.
  * Needs a `sites_mounter`. `HostError::Value("sites.reload", ..)` without one.
  * `HostError::Value("sites.reload", ..)` when the builder was given any of the things that block `reload`.
* `reload_with(&self, builder: HostBuilder) -> Result<Arc<HostReport>, HostError>`: `reload` over a builder the caller made.
* `async fn reload_checked(&self)`, `async fn reload_sites_checked(&self)` and `async fn reload_with_checked(&self, builder: HostBuilder)`, each `-> Result<Arc<HostReport>, HostError>`: the same three with every probe run over the candidate before the swap.
  * The probes, in order: each warmable source loaded once, a document for every path the routes that read nothing of the request answer, each `paths` set of a pattern with a parameter, each mounted site's `[site] probe` list, then every `reload_check`.
  * The host's own probes refuse on any 5xx kind and on `request_timeout`. They pass any other 4xx kind.
  * A `[site] probe` entry or a `reload_check` refuses on any failure.
  * The first refusal is `HostError::Probe`. The running tables keep serving.
* SIGHUP, the sites poll and `POST /__fsr/sites/reload` reload with the probes. `POST /__fsr/reload` swaps whatever assembles.

### Host::render

* `async fn render(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`: the route for `path`.
  * `path` may carry a locale prefix, stripped before the route matches and resolved into `ctx.locale`.
  * `path` may carry a query string, decoded into `ctx.query`.
  * Services are bound with the session's identity and no credentials. No CSRF token is minted.
  * A prefixed request for the default locale carries `canonical` in its head.
* `async fn render_to_string(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<String, HostError>`
* `async fn render_with_status(&self, path: &str, mode: RenderMode, session: SessionCell, locale: Option<&str>) -> Result<(StatusCode, String), HostError>`: the text with the status `handle` answers.
  * `locale` stands for the request's `Accept-Language`. A path prefix outranks it.
  * The status is the page loader's failure kind through `FailureKind::http_status` when it failed, else `200`.
* `intercept_for(&self, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)>`: the intercept a soft navigation to `path` renders, among the route's variants in file order.
  * With `into`, the variant whose slot it names.
  * Otherwise the first variant whose layouts, from the shell down to the one declaring its slot, the route of `from` shares.
  * Both paths are without their query.
* `async fn render_navigation(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`: the payload for a soft navigation. The intercept when `intercept_for` finds one, else `render` in `Payload` mode.
  * `path` and `from` may carry their query.
  * Under an intercept, the layouts the origin's route shares with the intercept's load under the origin's path, params and query through `assemble_under`. Their `$path` is the target's.
  * Under an intercept every node's `ctx.document` is the origin's path and `ctx.address` is the target's request.
  * An intercepted render carries `from`, locale prefix stripped, as `RequestCtx::document`. Every other render leaves it `None`.
* `async fn render_navigation_to_string(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell) -> Result<String, HostError>`
* `async fn render_navigation_with_status(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell, locale: Option<&str>) -> Result<(StatusCode, String), HostError>`: `locale` as for `render_with_status`.
* `async fn render_not_found(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<Option<BoxStream<'static, String>>, HostError>`: the application's not-found tree for `path`, with `params.path` set to the path without its query. `None` when the application has none.

### Host::prerender

* `prerenderable(&self) -> Vec<String>`: the patterns one render serves for every request.
  * Every source is lowered and reads nothing of the request (`snapfire_fsr_ir::body_reads_request`). No source is Rust.
  * No page or layout on the plan reads its `identity` or `csrf_token` prop (`Component::reads_prop`).
  * No parameter, unless the page loader's `paths` names the sets to render (`App::paths`).
* `prerenderable_anonymous(&self) -> Vec<String>`: the patterns one anonymous render serves for every anonymous request.
  * No parameter, unless `paths` names its sets.
  * Every source is lowered and reads nothing of the request except `identity` and calls through a client whose `bearer` is set.
  * No page or layout on the plan reads `csrf_token`.
  * `handle` serves their files only to a session with no identity. A signed-in visitor is rendered live.
* `async fn prerender(&self, out: &Path) -> Result<Vec<(String, PathBuf)>, HostError>`: writes the prerender directory. It may run while the host serves.
  * First the warm pass: every source `App::warmable` names is loaded once per supported locale with no request behind it. The loads are written to `<out>/LOADS_FILE`. The result replaces the memo the host booted with.
  * A source whose load fails is logged and left out.
  * Then every prerenderable pattern, the anonymous class included, is rendered anonymously once per supported locale into `<out>/<path>/index.html` and `index.payload`.
  * `/` is at the top of `out`. A locale other than the default is under its tag, `<out>/fr_FR/<path>/`.
  * A pattern in `App::paths` is rendered once per set its body returns for the locale, at the path the set fills in. A set missing a parameter or giving one an empty value is `HostError::Paths`.
  * Each file is written beside its place and renamed in.
  * A page whose loader fails is logged and keeps the file the last run wrote.
  * After each page's `index.html` its card images are drawn beside it as `<stem>.png`, `<stem>.jpg` or `<stem>/<id>` for a listed image. A card that fails is logged and keeps its file from the last run.
  * When the host answers `client::ROUTE` from the binary and a document was written, the client modules are written under `<out>/static/js/fsr/`, in the build the host serves.
  * Then the render pass: every subtree in `App::renderable` is assembled once per locale and per `paths` set, with `WarmRenders::record` on. The entries are written to `<out>/RENDERS_FILE` and stay in this host's memo. A subtree whose render fails is logged and left out.
  * Files the last run listed in `<out>/WRITTEN_FILE` that this run did not write are removed, with any directories left empty.
  * Then the run writes its own list to `WRITTEN_FILE` and the build it rendered from to `<out>/BUILD_FILE`. The build is a hash of the plan and every mounted site's plan, the templates and each assets manifest.
  * Returns what it wrote. A page is under its path with its prefix, a client module under its URL and the loads file and written list under their own names.
* A host whose build differs from the one `BUILD_FILE` names reads none of the directory's documents, loads or renders at boot. `HostReport::prerender_stale` is set. The same holds for a directory with a written list and no `BUILD_FILE`.
* Under `dev` the host never answers from the directory and does not read its loads or renders files at boot.
* Under `dev` a `GET` whose query carries `__prerendered` is rendered as `prerender` would write it, anonymously with the plain head. It answers with `x-sf-prerendered: preview`. It answers 404 naming why when the route reads the request, the path is outside its `paths` or its loader failed.
* `prerendered(&self, path: &str, mode: RenderMode) -> Option<String>`: the prerendered text for `path`. The locale prefix chooses the locale's directory and the query string is ignored. `None` without a directory or a file.

### Host::handle

* `async fn handle(&self, req: Request<Bytes>) -> Response<Body>`: answers one request. The steps below run in order. The [framework routes](#framework-routes) that answer before statics, middleware and sessions come first.
* Static roots, by prefix.
* The locale: `Locales::resolve` over the path, the `Cookie` header and `Accept-Language`. The action route resolves over the path in the `x-sf-from` header instead. The stripped path stands in for the rest of the steps.
* A locale-prefixed `/_sf/`, `/__fsr/` or (with a provider mounted) `/auth/` path is 404.
* With a provider mounted, the identity routes. Each answers the `Step` `Auth` returns.
  * `GET /auth/login` calls `Auth::begin` with `Start::sign_in`. Its `return_to` is the query's when that is a path on this origin, else the `Referer`'s path, else `/`. Its `method` is the query's.
  * `POST /auth/enrol` verifies `_csrf` or `x-sf-csrf` (403 when it fails). It answers 401 to an anonymous session and calls `Auth::begin` with `Start::enrol` for the session's identity.
  * `/auth/callback` reads its params from a form-encoded or JSON `POST` body or a `GET` query, then calls `Auth::advance`.
  * A request whose `Accept` names `application/json` gets 200 with `{ "next": "redirect", "to" }`, `{ "next": "present", "kind", "data" }` or `{ "next": "complete", "to" }`. A refusal gets `{ "error": "denied" | "invalid", "message" }` with the error's status.
  * Any other request gets 303 to the redirect, 303 to `<login>?return_to=<pending>` for a step the page presents and 303 to the destination on completion.
  * `AuthError::Denied` is 303 to `<login>?error=denied&return_to=<pending>`. `Invalid` is 400 with the message.
  * A completed sign-in renews the session id and rotates the CSRF scheme. A completed enrolment keeps both.
  * `POST /auth/logout` verifies `_csrf` from the body or `x-sf-csrf` with `Sessions::verify_csrf` (403 when it fails). Then `Auth::logout`, `Sessions::destroy` and 303 to `/` with the expiring cookie.
  * A `GET` of the login page calls `Auth::ensure_flow` with the query's `return_to`, else the `Referer`'s path when that is not the login page, else `/`. The request then continues.
* The middleware chain, run by `snapfire_fsr_runtime::drive_middleware`.
  * The shell's middleware runs first, with `site` in its input naming the mounted site the path belongs to. `site` is null outside every site.
  * Its `next` runs the middleware of the site the path belongs to. That `next` runs the route.
  * Each answers an `HttpResponse` value. That is its own response, the route's response with the status and headers it left or nothing, which continues.
  * The route's body streams under the head the chain answered.
  * A site's middleware handing `next` a path outside its prefix is `Internal`.
* `POST /_sf/action/<id>`, with the id's `%XX` escapes decoded.
  * 403 when the request's `Origin` names neither its `Host`, `[document] origin` nor a trusted origin. A request with no `Origin` is not refused by this check.
  * 415 for a body that is neither `application/json` nor a form.
  * A JSON body: 400 when it does not parse. The answer is the value as JSON, with the action's `redirect` target in `x-sf-redirect`. A failure answers its kind's status.
  * A form body (`application/x-www-form-urlencoded` or `multipart/form-data`): `_csrf` is taken out and verified with `Sessions::verify_csrf` (403 when it fails). The remaining fields are read against the action's declared input type.
  * A form success is 303 to the path the action passed to `redirect`, else the `Referer`'s path on this origin, else `/`. A form failure is 303 to that page with the failure on the session.
  * A form caller naming `application/json` in `Accept` is answered as a JSON body is.
* A route handler matching the method and path.
  * A JSON body is the input, `null` when empty. A form-encoded body has its `_csrf` verified and its fields read against the handler's declared input type.
  * The answer is the value as JSON. An `HttpResponse` value answers with its own status, headers and body.
  * 400 on a body that does not parse. A failure answers its kind's status.
* A `GET` the prerender directory holds answers that text with `x-sf-prerendered: 1`. A route prerendered for anonymous visitors is rendered live for a session with an identity.
* A page.
  * `__payload` in the query selects payload mode. In payload mode an `enc` outside `PAYLOAD_ENCODINGS` is 406.
  * In payload mode an `x-sf-from` or `x-sf-into` header makes it `render_navigation`, which skips the prerender directory when an intercept applies.
  * `__fragment` in the query selects a fragment. See [RenderMode](#rendermode).
  * When the route's own page loader failed, the status is its failure kind's, 404 for `not_found`. The document is still rendered around the error segment. A payload or fragment carries the same status.
  * A failing layout or slot degrades its own segment and the status stays 200.
* No route: the not-found tree with 404 when the application has one, else 404 with a line of text.
* The session is opened from `Cookie`. A `Set-Cookie` is appended when it changed. So is the locale cookie a `Resolution` asks for.
* Every body runs with the session's token custody bound to its services. Once the session is identified, its CSRF token is `ctx.csrf`. An anonymous request carries no token.

### Framework Routes

The routes the host answers itself.

* `server.health` and `server.ready`, before statics, middleware and sessions. See [ServerConfig](#serverconfig).
* `GET /__fsr/sites`, before statics, middleware and sessions, whatever `dev` says: `{"sites": [{"name", "at", "version", "hash"}, ..]}` for every mounted site.
* `POST /__fsr/sites/reload`, `sites_reload` feature: `reload_sites_checked`. 200 with `{"reloaded": true, "sites": [...]}`. 409 with `{"reloaded": false, "error"}` when the candidate is refused. One reload runs at a time. The host does not guard this route.
* `GET /_sf/live?topics=a,b`, whatever `dev` says, after `HostBuilder::topics` allows every topic: a `text/event-stream`.
  * A `: open` comment frame at once, then one `data: {"topic":"a"}` per `publish` of a listed topic, until the client goes away.
  * No topics is 400.
* `GET /_sf/socket?topic=a`, `ws` feature: upgrades to a WebSocket.
  * `HostBuilder::topics` decides whether this visitor may open it. `HostBuilder::socket` answers what it sends.
  * Each frame is one row: `{"key": ..., "value": ...}` in, `{"rows": [...]}` out.
  * No handler is 404, no topic is 400, a topic the rule refuses is 403 and a request that is not an upgrade is 400.
  * `serve_listener` serves each connection `with_upgrades`.
* `POST /_sf/island/<module>`: one step of an island in server mode. See [IslandBody and IslandStep](#islandbody-and-islandstep).
* `GET /static/js/fsr/<name>`: the embedded client, unless a `StaticRoot` claims the prefix. See [client](#client).
* `/manifest.webmanifest` and `/sw.js`, with `[pwa]`. See [PwaSection](#pwasection).
* `robots.txt` and the sitemap at the paths `[crawl]` names. See [CrawlSection](#crawlsection).
* A `GET` or `HEAD` an `App::images` row matches: the card image, after the session opens.
  * Outside `dev`, a prerendered file for the path when the directory is current. Else the bytes kept for the path and props. Else a draw.
  * A data source's `not_found` and an unlisted id are 404. A draw failure is 500 with the reason.
  * A card element's `class` is drawn from the table at `CARD_CLASSES_FILE` under the app. Each class's declarations, in the table's order and filtered by the card's width, are written before the element's own `style`.
  * A mounted site's images read the site's own table. A missing table reads as empty. A table that does not parse is logged on target `fsr::card` and reads as empty.
* With `dev` on, before statics, middleware and sessions:
  * `GET /__fsr/events`: a `text/event-stream` with one `data: {"bundle":"<id>","boot":"<id>","generation":<n>,"styles":[...]}` event on open and one per `changed`. `styles` is the document's current stylesheets.
  * `POST /__fsr/changed`: 204 after calling `changed`.
  * `POST /__fsr/reload`: 200 with the new report as text after `reload`. 409 with the error when it fails.
  * Each of the three is 403 for a request carrying `Origin`.
  * The bundle id is a hash over every output `dist/.snapfire-build.json` lists, source maps aside. `-` without a bundle.
  * A served document's head carries `dev_script` with that moment's bundle id. `prerender` writes the plain head.
  * Static files answer `Cache-Control: no-cache`.
  * `GET /__fsr/traces`: the last 50 traces as `trace::to_value` writes them, newest last. `[]` when no collector was given to `traces`.
* With a collector given to `traces`, on a path `trace.expose` covers (any path with `dev` on):
  * The response carries `x-sf-request: <id>.<hmac>`: the trace id and an HMAC-SHA256 of `fsr.trace:<id>` under the session keyring's current key.
  * A document's head carries `<meta name="sf-request" content="<token>">`.
  * `GET /__fsr/trace/{token}` answers that trace as `trace::one_value` writes it, with `Cache-Control: no-store`, once the request has finished and while the collector's ring holds it.
  * A token that verifies under no key in the ring, an id the ring no longer holds and a trace still in flight are 404 `no such trace`.
  * With `expose` empty and `dev` off, no response carries a token and the path is not a route.
* With a collector given to `traces`, on an HTML page `trace.inline` covers:
  * The body ends with `<script type="application/json" id="sf-trace">`, written before the last `</body>` once the rest of the body has been sent.
  * It holds the request's trace as `trace::one_value` writes it, with `<` escaped and `ms` measured up to that point.
  * The response is `Cache-Control: private, no-cache` and has no `Content-Length`.
  * A payload or fragment request gets no script.

Every page or payload response also carries the following.

* The theme, when the plan or a mounted site's plan has `theme-of` rows or a `theme` body.
  * After the middleware, the route's static theme is read down its plan's `content` chain. The deepest row wins unless a row above it forces its own. Of two forced rows the outer wins.
  * The shell's `theme` body runs with `{ name, forced }` as input, then the mounted site's with the shell's answer as `name`. Each answers `{ name, scheme }`, strings or null.
  * A body that fails or answers another shape is logged on target `fsr::theme` and leaves the choice as it was.
  * The answer is written onto the opening `<html>` of every document, live and prerendered, as `data-sf-theme` and `data-sf-scheme`. An empty `data-sf-theme-forced` is added when the route forces its theme and the answer kept it.
  * An attribute already there is replaced and one the answer leaves out is removed.
  * A payload carries the theme in its eager `H` row. A prerendered payload's row is rewritten to match.
  * A fragment request is left as it is.
* On a mounted site's routes, the site's stylesheets and a `<script type="module">` for its entry in the head. The payload gets an `E` row naming the entry.
* Each sheet the plan's `module-sheets` rows name for the request's layouts, page, fallbacks and error boundaries, when the bundle holds it, by its served URL. It is an owned `<link rel="stylesheet" data-sf-css>` in the head and an entry in the payload's `C` row. A component sheet no row names stays linked on every document.

### IslandBody and IslandStep

`POST /_sf/island/<module>`, the module id percent-encoded, with a JSON body `{ props, state, handler, event }`: one step of an island in server mode.

* A module the plan lowered is stepped by the interpreter. A module `island_handler` registered is stepped by its own handlers.
* 403 from another origin, under the same rule as an action. 415 for a body that is not `application/json`.
* `handler` null renders the island as it is. `event` is what the browser saw.
* The document's locale is the island's, from `x-sf-from` as for an action. The session cookie is set as on every response.
* A failure of kind `internal` is logged on target `fsr::island` with the module and its message. Outside development its answer is `{ kind: "internal", message: "the island step failed" }`.
* A lowered component:
  * `state` names the component's state. A component rendered inside it is under `<address>/<name>`.
  * `handler` is the index of the handler that fired. For a handler of a component inside it, `handler` is `<address>/<index>`. That is the token its `data-sf-on` carries.
  * The step reads the store from the `$store` prop.
  * Every action the handler called is dispatched in order before the answer, with this request's session, locale and host. The first failure is answered as the action route would answer it and nothing after it runs.
  * 200 with `IslandStep::json`.
  * 400 with `{ kind: "invalid", message }` for a body that is not this shape, a state key the component lacks or a `handler` that is a name.
  * 404 for a module that is not lowered, a handler index it lacks or an address its render does not reach. A failure of the interpreter answers its own status.
  * A slot in the rendered markup is 500.
* A template island:
  * `state` is whatever its handlers agreed on. `handler` is a name. 400 when it is not.
  * Runs the named handler with the props, the state and the event. Then renders the module through its own evaluator with `snapfire_fsr_runtime::island_data`: the props without `$s` and `$k`, plus the answered state under `state`.
  * 200 with `{ state, html, revalidate }`. `revalidate` is true when the handler wrote the session.
  * 404 for a name the module does not have. A handler failure answers its kind's status.
  * A slot the template emits is 500.
* `pub fn island_body(body: &[u8], locale: &str) -> Result<IslandBody, (StatusCode, serde_json::Value)>`: reads a step's body. `locale` joins the props as it does for a page.
* `pub struct IslandBody { pub props: ValueMap, pub state: Value, pub handler: Value, pub event: Value }`, `#[non_exhaustive]`.
* `pub fn island_step(lowered: Option<&IrEvaluator>, module: &str, body: &[u8], locale: &str) -> Result<IslandStep, (StatusCode, serde_json::Value)>`: a lowered component's step, without dispatching its actions. The error is the status and body to answer with.
* `pub struct IslandStep { pub state: ValueMap, pub store: ValueMap, pub html: String, pub acts: Vec<(String, Value)> }`, `#[non_exhaustive]`.
  * `store`: the store keys the handler wrote.
  * `acts`: the actions the handler called, with their inputs, in order.
  * `json(&self) -> serde_json::Value`: `{ state, store, html, revalidate }`, with `revalidate` true when `acts` is not empty.

### AuthFlow

`auth` feature.

* `pub struct AuthFlow`: the session id and token custody one identity journey is keyed by, for `Host::auth_call`. `Clone`.
* `AuthFlow::new() -> AuthFlow`. `Default` is the same.
* One per session.

### Request Helpers

* `pub fn fragment_of(raw_query: &str) -> Option<Option<String>>`: the fragment a query asks for, read as `application/x-www-form-urlencoded`.
  * `Some(None)` for the bare key or an empty value (`__fragment`, `__fragment=`).
  * `Some(Some(slot))` for a named one. The key and the slot are percent-decoded and `+` is a space.
  * The first `__fragment` pair decides.
* `pub fn preview_of(raw_query: &str) -> bool`: whether any pair's decoded key is `__prerendered`.
* `pub fn payload_of(raw_query: &str) -> bool`: whether any pair's decoded key is `__payload`, whatever its value.
* `pub fn with_fragment(location: &str, slot: Option<&str>) -> String`: `location` with `__fragment` asked for again.
* `pub fn referer_path(referer: &str) -> Option<String>`: the path of a `Referer` on this origin. `None` for another origin.
* `pub fn percent_decoded(segment: &str) -> String`: a path segment with each `%` and two hex digits decoded. `+` and any other `%` are kept. A segment that decodes to invalid UTF-8 is returned as given.
* `pub fn middleware_request(method: &str, path: &str, query: &str, site: Option<&str>, headers: ValueMap) -> Value`: the `request` a middleware reads: `method` uppercased, `path`, `payload` (`payload_of(query)`), `site` or null and `headers`.

### Probe

* `pub enum Probe { Path(String), As { path: String, subject: String } }`: one entry of `[site] probe`. `Deserialize`, untagged.
* `"/billing"` is a `Path`. `{ path = "/billing/overdue", as = "probe" }` is rendered by a visitor signed in as that subject with no claims.
* `path(&self) -> &str`
* `subject(&self) -> Option<&str>`

### ReloadCheck

* `pub trait ReloadCheck: Send + Sync { fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>>; }`: `Err` refuses the candidate with the reason.
* Implemented for any `Fn(&Candidate) -> BoxFuture<Result<(), String>>` that is `Send + Sync`.

### Candidate

* `pub struct Candidate<'a>`: the tables a checked reload would swap in, rendered through the running host's sessions, evaluators and services and never served.
* `report(&self) -> &HostReport`
* `async fn render(&self, path: &str, session: SessionCell) -> Result<(StatusCode, String), HostError>`: the path, with its query, as a document in the default locale, with the status `handle` would answer. `NotFound` for a path no route matches.
* `async fn render_as(&self, path: &str, subject: &str) -> Result<(StatusCode, String), HostError>`: the same, signed in as `subject`.
* Middleware does not run under a candidate render.

### Mount

A site's artifact as a host mounts it.

* `pub struct Mount { pub name: String, pub version: String, pub hash: String, pub allow_engine: bool, pub artifact: Artifact }`. `artifact.config.root`, `version` and `hash` reach the report.
* `Mount::new(name: impl Into<String>, version: impl Into<String>, hash: impl Into<String>, allow_engine: bool, artifact: Artifact) -> Mount`.
* At `build` a mount is refused with `HostError::Mount` when its configuration has no `[site]` or names another site, when its prefix is already served, when it carries engine-owned rows without `allow_engine` or when its bundle carries a server module.
* A mounted site's routes and intercepts are nested under the shell's `routes/layout.tsx#default`. When the shell has none, they are nested under the document. Their ids are renumbered.
* Its sources, actions, components and handlers join the shell's tables. Its contract is merged.
* Its clients register under `<name>:<client>` with their bearer keys.
* Its static roots under its prefix are served. The rest are listed as ignored, with `session`, `auth`, `locales` and `cache` when set.
* Its import map entries the shell lacks are added.
* Its middleware and its `theme` body run after the shell's. See [Host::handle](#hosthandle).
* Its `theme-of` rows join the shell's. Its stylesheets and entry module join the head on its routes.

### Locales

* `pub struct locale::Locales { pub supported: Vec<String>, pub default: String, pub order: Vec<Source>, pub remember: bool, pub cookie: String }`: the checked `[locales]` section.
* `pub enum locale::Source { Prefix, Cookie, Header }`.
* `Locales::single() -> Locales`: `en` alone with no source consulted, for a host without `[locales]`.
* `Locales::from_section(section: &LocalesSection) -> Result<Locales, String>`
* `is_default(&self, tag: &str) -> bool`
* `locale(&self, tag: &str) -> Locale`: the tag with its default flag.
* `default_locale(&self) -> Locale`
* `find(&self, tag: &str) -> Option<&str>`: the supported locale `tag` spells, ignoring case and `-` against `_`.
* `nearest(&self, tag: &str) -> Option<&str>`: `find`, else the first supported locale of the same language.
* `split_prefix<'p>(&self, path: &'p str) -> Option<(&str, &'p str)>`: the supported locale the first path segment spells and the rest of the path, at least `/`. `None` when there is no such prefix or `prefix` is not a source.
* `from_accept_language(&self, header: &str) -> Option<&str>`: the nearest supported locale of the header's tags by descending weight. `*` is ignored.
* `from_cookie(&self, header: &str) -> Option<&str>`: the supported locale the cookie names.
* `resolve(&self, path: &str, cookie: Option<&str>, accept_language: Option<&str>) -> Resolution`: the sources in `order`, the first that answers. The default when none does.
* `pub fn locale::load_catalogs(app: &Path, default: &str) -> Result<Catalogs, String>`: the catalogs under `<app>/locales/*.toml`, one file per locale named by its tag.
  * Tables nest: `[cart]` then `empty = "…"` is the key `cart.empty`.
  * A string, number or boolean is kept as text. Any other value is refused, naming the file and the key.
  * No directory is no catalogs.

### Resolution

* `pub struct locale::Resolution { pub locale: Locale, pub path: String, pub prefixed: bool, pub set_cookie: Option<String> }`.
* `path`: the path without its locale prefix. `prefixed`: whether it had one.
* `set_cookie`: the `Set-Cookie` value to append when `remember` is on and the prefix chose a locale the cookie does not hold, `sf_locale=<tag>; Path=/; Max-Age=31536000; SameSite=Lax`.
* `requested(&self) -> String`: the path as the browser holds it, the prefix put back when the request carried one.

### RenderMode

* `pub enum RenderMode { Html, Payload, Fragment(Option<String>) }`, `#[non_exhaustive]`. `Debug`, `Clone`, `PartialEq`, `Eq`. Not `Copy`.
* `Fragment(None)` renders the route's page segment alone. `Fragment(Some(slot))` renders the parallel slot of that name, wherever it sits on the route.
* A fragment is bare markup: no shell, no segment delimiters and no sidecar. Every deferred segment is resolved before anything is written.
* A fragment is followed by the route's store seed as `<script type="application/json" data-sf-store>` when it has one.
* `handle` chooses a fragment from `__fragment` in the query: bare for the page, `__fragment=<slot>` for a slot. It answers `text/html`.
* `parse_query` drops the key. No loader or segment key sees it.

### HostReport

What the host bound. `Display` prints the full report and `brief` prints what needs attention.

* `pub struct HostReport` with these public fields:
  * `app: snapfire_fsr::Report`
  * `http2: bool`, `tls: Option<TlsReport>`
  * `services: Vec<(String, String, String)>`, `session: Option<String>`, `cached: Vec<(String, String)>`, `writers: Vec<(String, String)>`, `shared: Vec<(String, Vec<String>, Vec<String>)>`
  * `statics: Vec<(String, PathBuf)>`, `unanswered_routes: Vec<String>`, `client: Option<(&'static str, usize, usize, bool)>`
  * `prerender: Option<PathBuf>`, `prerender_stale: bool`, `warmed: usize`, `rendered: usize`, `cache: Option<(u64, String)>`, `dev: bool`
  * `locales: Vec<String>`, `hosts: Vec<String>`, `origin: Option<String>`, `crawl: Option<CrawlReport>`
  * `auth: Option<(String, String)>`, `bearer: Vec<(String, String)>`, `extensions: Vec<String>`, `catalogs: Vec<(String, usize)>`
  * `site: Option<(String, String)>`, `sites: Vec<SiteReport>`
  * `import_map_csp: Option<String>`, `csp: Option<String>`, `csp_report_only: Option<String>`
  * `config: Vec<PathBuf>`, `inferred: Vec<String>`, `public: Vec<(String, String)>`, `ignored: Vec<String>`
* `services`: the service name, its kind (`http`, `grpc`, `mock` or `rust`) and its base URL, responses file or Rust type. `Display` prints a `services` row each.
* `session`: the client sessions live behind when `session.store` is `service`. `Display` prints `service via <client>`.
* `cached`: `service.method` and its policy as text, `ttl 15s shared, stale 2m [tags]`. A `cached` row each.
* `writers`: `service.method` and `[tags]` for each method that drops cached answers. A `writes` row each.
* `shared`: every `@` tag with the methods cached under it and the methods that drop it, across the shell and every site. A `shared` row each, `cached by [a:cat.list], dropped by [b:ledger.pay]`.
* `unanswered_routes`: the `[[route]]` patterns nothing registered. A `routes` row each.
* `client`: the embedded client's prefix, module count, total size and whether the minified build is served. `None` when a static root claims the prefix. One `client` row after the `static` rows, naming the build as `minified` or `readable`.
* `prerender`: the prerender directory when configured. `Display` lists each prerenderable pattern with it. It prints `not configured` without one. The anonymous class is marked `for anonymous visitors` and a pattern `Report::paths` names is marked `per paths`.
* `prerender_stale`: the directory was written by another build. `Display` says so on each row.
* `warmed`: how many keys the loads file answered at boot. `Display` lists each of `app.warmable`'s sources under `warm`, the first row with the count or `not warmed` when zero.
* `rendered`: how many memo entries the renders file answered at boot. `Display` lists each of `app.renderable`'s subtrees under `render`, with the pattern and module, the first row with the count or `not rendered`.
* `cache`: the render memo's capacity and lifetime as written. One `cache` row when set.
* `dev`: one `dev` row naming the two development paths when true.
* `http2`: one `http2` row when true, naming the ALPN list under TLS or saying h2c without.
* `tls`: `Display` prints the two files and what re-reads them.
* `locales`: the configured locales, the default first. Empty without `[locales]`. One `locales` row, `en_US (default, unprefixed), fr_FR`.
* `hosts`: `server.hosts`. `origin`: `document.origin`.
* `crawl`: `[crawl]` when configured. See [CrawlReport](#crawlreport).
* `auth`: the provider name (`file`, `service via <client>`, else `custom` for one given to the builder) and the login page. One `auth` row naming both and the four routes. When `bearer` is empty it adds `bearer    none; no client carries a token`.
* `bearer`: client and custody key for every client whose `bearer` names one. A `bearer` row each.
* `extensions`: the native pairs registered beside the standard library, by name. `natives` rows labelled `rust`, after `bearer`.
* `catalogs`: each locale with a file under `locales/` and its key count. One `catalogs` row, `en_US 5 keys, fr_FR 5 keys`.
* `site`: the application's own `[site]` name and prefix. One `site` row.
* `sites`: the mounted sites. A `sites` row each, `billing at /billing from <artifact> <version> <hash>`. Another row names what the mount ignored.
* `import_map_csp`: the `script-src` source covering the document's inline import map, `'sha256-<base64>'`. `None` without an import map. It is computed from the merged map of the shell and every mounted site. A `csp` row after the `client` row.
* The capture script's source is the hash of `CAPTURE_SCRIPT_BODY`, the same on every page.
* `csp`: `document.csp` with the import map's source substituted, the header an HTML response carries. `None` when unset. Printed on the line under the `csp` row, which says when it is unset.
* `csp_report_only`: the same for `document.csp_report_only`.
* `Display` prints the app's report, then the `services` rows, the `static` rows, the `config` sources and the `inferred` lines, with the rows above.
* `brief(&self) -> Brief<'_>`: `Display` of only what needs attention: `unanswered_routes`, each site's ignored rows, each prerenderable pattern when the directory is stale, `crawl.unlisted` and `ignored`. `fsr serve` prints it unless `--verbose`.

### TlsReport

* `pub struct TlsReport { pub cert: PathBuf, pub key: PathBuf, pub alpn: Vec<String>, pub reload: Option<String> }`: the configured certificate, what the handshake offers and the signal that re-reads it, `None` for no signal.

### SiteReport

* `pub struct SiteReport { pub name: String, pub at: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub ignored: Vec<String> }`: one mounted site.
* `ignored`: the site's configuration the shell did not take. It holds `static <route>` for a root outside the prefix or one the shell serves. It holds `session`, `auth`, `locales` and `cache` when the site set them.

### CrawlReport

* `pub struct CrawlReport { pub robots: Option<String>, pub sitemap: Option<String>, pub unlisted: Vec<String> }`.
* `robots` and `sitemap`: where `[crawl]` answers. The sitemap path is already under a site's prefix.
* `unlisted`: the patterns with a parameter and no `paths` export, which the sitemap cannot list.
* `Display` prints a `crawl` row per file and one row per unlisted pattern.

### ServiceSessionStore

* `pub struct ServiceSessionStore`: a `SessionStore` over a client.
* `ServiceSessionStore::new(services: Arc<Services>, client: impl Into<String>, ttl: Duration) -> Self`: `ttl` is the end given to a stored record that has none.
* `load` calls `getSession { id }` and reads `record` from the answer. `None` on a `not_found` failure. Any other failure is logged and is also `None`.
* `save` calls `putSession { id, record }`. `delete` calls `deleteSession { id }`.
* The record is one JSON string in the payload encoding, `{ data, identity, tokens, csrf, expires }`.
  * `identity` is `{ subject, claims }` or `null`.
  * `expires` is an integer count of seconds from the Unix epoch.

### ServiceProvider

`auth` feature.

* `pub struct ServiceProvider`: an `IdentityProvider` over a client.
* `ServiceProvider::new(services: Arc<Services>, client: impl Into<String>) -> Self`.
* When the contract declares `beginSignIn` and `advanceSignIn`, `begin` and `advance` call them.
  * `beginSignIn { intent: "sign_in" | "enrol", return_to, method, identity }`.
  * `advanceSignIn { params, state }`.
  * Each answers `{ next: "redirect", to, state }`, `{ next: "present", kind, data, state }` or `{ next: "complete", subject, claims, tokens }`.
* `refresh` calls `refreshSignIn { tokens }` when declared. It answers `{ tokens }` or null.
* An open map (`identity`, `params`, `state`, `data`, `claims` or `tokens`) is sent as one string in the payload's JSON encoding. An answer may write one as a map or as such a string.
* A client declaring only `authenticate` is a password service. `begin` presents the password form. `advance` sends `user` and `password` to `authenticate` and reads `subject`, `claims` and `access_token` from a map answer.
* A failure of kind `unauthorized`, `not_found` or `invalid` is `AuthError::Denied` with the service's message. Any other failure or a malformed answer is `AuthError::Invalid`.
* The host refuses at boot a client declaring neither `authenticate` nor both step methods.

### AssetsManifest

Module `assets`. `generated/assets.json`, `assets::ASSETS_FILE`, read at boot when present. `assets::VERSION` is 1.

* `pub struct AssetsManifest { pub version: u32, pub images: ImagePolicy, pub entries: Vec<ImageEntry>, pub fonts: Fonts, pub media: Vec<MediaFile>, pub local_stills: bool }`.
  * `media`: absent from the file when empty.
  * `local_stills`: `[media] embed_stills = "local"`, absent when false. The document carries it as `sf:embed-stills`.
  * `AssetsManifest::read(app: &Path) -> Option<AssetsManifest>`
  * `policy_json(&self) -> String`
  * `image(&self, source: &str) -> Option<&ImageEntry>`
* `pub struct MediaFile { pub source: String, pub url: String, pub path: String }`: a video, audio file or caption track a module imports, placed under `dist/` by its hash.
* `pub struct ImagePolicy { pub widths: Vec<u32>, pub formats: Vec<String>, pub quality: BTreeMap<String, u8>, pub base: Option<String>, pub sources: BTreeMap<String, String> }`.
* `pub struct ImageEntry { pub source: String, pub src: String, pub path: String, pub hash: String, pub width: u32, pub height: u32, pub passthrough: bool, pub stripped: bool, pub widths: Vec<u32>, pub quality: BTreeMap<String, u8>, pub variants: Vec<Variant> }`.
  * `path`: the hashed original, under the bundle's output directory.
  * `stripped`: the original is written without its metadata. `hash` is then of the stripped bytes.
* `pub struct Variant { pub width: u32, pub format: String, pub url: String, pub path: String }`: `path` is under the bundle's output directory.
* `pub struct Fonts { pub base: Option<String>, pub faces: Vec<Face>, pub css: String, pub preload: Vec<String>, pub remote: Vec<Remote>, pub referenced: Vec<FontFile>, pub variables: BTreeMap<String, String> }`.
* `pub struct FontFile { pub source: String, pub url: String, pub path: String }`: a font a module or stylesheet names outside the fonts directory. Placed under `dist/` like a face and written into no `@font-face`.
* `pub struct Face { pub key: String, pub family: String, pub weight: u16, pub weight_range: Option<(u16, u16)>, pub style: String, pub source: String, pub url: String, pub path: String, pub unicode_range: Option<String>, pub preload: bool }`.
* `pub struct Remote { pub key: String, pub family: String, pub href: String, pub preconnect: Vec<String> }`.

What a document gains from the manifest:

* `<meta name="sf:images">` with the policy, `ImagePolicy` as JSON, which the client's `Picture` reads.
* The font CSS inlined as `<style data-sf-fonts>`, with its hash widened into `style-src`.
* One `<link rel="preload" as="font" crossorigin>` per face the manifest marks.
* For a provider's stylesheet, a `<link rel="preconnect">` per origin and the `<link rel="stylesheet">`.
* `font-src`, `img-src` and `style-src` widened with the origin of `fonts.base`, `images.base`, each source template and each preconnect.
* A plan component's `head` rows merged into the document of every page whose plan places that module. That includes the preload of a priority image. A row is merged by identity. Two pages placing one image preload it once.
* A static file whose name carries a content hash, `hero.0a1b2c3d.png` or `hero.0a1b2c3d.640.avif`, answers `Cache-Control: public, max-age=31536000, immutable` in place of `static_max_age`.
* With the `tera` feature, `fsr_picture(src, alt, sizes, priority, class)` writes what the `Picture` tag writes for an image the manifest holds, by its path under the app. An image the manifest lacks fails the render.
* With the `tera` feature, `fsr_fonts()` writes the inline style and the font links.

### tera

`tera` feature.

* `pub fn tera::evaluator(app: &Path, assets: Option<&AssetsManifest>) -> Result<Option<(TeraEvaluator, Vec<String>)>, HostError>`: every file `template_files(app)` lists, in one `Tera` with the markers registered, each named by its path relative to `app` with `/` separators. `None` when the app holds no template.
* A template that fails to parse is `HostError::Template`.

### Body

* `pub type Body = UnsyncBoxBody<Bytes, std::io::Error>`.

### Constants

* `pub const PAYLOAD_ENCODINGS: &[&str] = &["json"]`: what a payload request may name in `enc`. The wire's `V` row names the one it got.
* `pub const CARD_CLASSES_FILE: &str = "generated/tailwind.cards.json"`: the card class table, relative to the app.
* `pub const SHARE_CARDS: bool`: whether this host was built with `share_cards`.
* `pub const HTTP: bool`: whether this host was built with `http`.
* `pub const LOADS_FILE: &str = "loads.json"`: the warm pass's file in the prerender directory. Every memoizable source's data under the key a request composes for it, in the payload crate's JSON encoding. An absent or unreadable file is not a boot error.
* `pub const RENDERS_FILE: &str = "renders.json"`: the render pass's file. Every subtree `App::renderable` names, once per locale, under its memo key, as `{ node, segments, digest }` with the node in the payload's row form. A boot reads it into a `WarmRenders` in front of the configured cache or `NoCache`. An absent or unreadable entry is not a boot error.
* `pub const WRITTEN_FILE: &str = "prerendered.json"`: the files a `prerender` run wrote, as a JSON list of paths relative to the directory. An absent file removes nothing.
* `pub const BUILD_FILE: &str = "prerender.build"`: the build a `prerender` run rendered from.
* `pub const TEMPLATE_EXTENSIONS: &[&str] = &["tera"]`: the extensions of a route or layout rendered by a template evaluator.
* `pub fn is_template_module(module: &ModuleId) -> bool`: whether a module's path ends in one of `TEMPLATE_EXTENSIONS`.
* `pub fn template_files(app: &Path) -> Result<Vec<PathBuf>, HostError>`: every template the host reads at boot, sorted. Each file under `app` ending in a template extension, outside dot directories and outside `vendor/`, `dist/`, `generated/`, `node_modules/`, `tests/` and `types/` directly under `app`. Compiled without the `tera` feature too. `HostError::Io` when a directory does not read.

## 4. Serving

### HostService

* `pub struct HostService(pub Arc<Host>)`. `Clone`.
* `impl<B> tower::Service<Request<B>> for HostService where B: http_body::Body + Send + 'static, B::Data: Send, B::Error: Debug`, with `Response = Response<Body>` and `Error = Infallible`. The request body is collected before `handle`. A body that fails to collect is empty.
* `Host::service(self: &Arc<Self>) -> HostService`

### Host::serve

* `async fn serve(self: Arc<Self>, listen: &str) -> std::io::Result<()>`: binds `listen`, then `serve_listener`.
* `async fn serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()>`: `serve_until` on `shutdown_signal()`. Returns `Ok(())` once a SIGTERM or SIGINT has drained.
* `async fn serve_until(self: Arc<Self>, listener: tokio::net::TcpListener, shutdown: impl Future<Output = ()>) -> std::io::Result<()>`: serves until `shutdown` resolves, then calls `begin_drain`.
  * Each connection is served by `hyper::server::conn::http1`. With `http2` on it is `hyper_util::server::conn::auto`.
  * With `[server.tls]` each connection is TLS first, with the version chosen by ALPN. A failed handshake is logged and dropped.
  * After the connections close it waits for `after` work, within `server.shutdown_grace`.
* `pub async fn shutdown_signal()`: resolves on SIGTERM or SIGINT. Where there are no signals it resolves on Ctrl-C.
* `begin_drain(&self)`: turns `server.ready` to 503 `shutting down` for the rest of the process and ends the open `/_sf/live` and `/__fsr/events` streams. `server.health` keeps answering 200.
* `draining(&self) -> bool`: whether `begin_drain` has been called.
* A server that owns the listener, such as an axum router or an actix `HttpServer` the host is in, calls `begin_drain` itself before it stops.
* `reload_tls(&self) -> Result<(), HostError>`, `tls` feature: re-reads the certificate and key and swaps what the next handshake presents.
  * Connections already up keep the certificate they started on.
  * A file that will not read leaves the running certificate in place.
  * `serve_listener` calls it on the configured signal. Does nothing without `[server.tls]`.
* `LIFELINE: &str = "FSR_HOST_LIFELINE"`: set in a host's environment, `HostBuilder::build` watches stdin. When stdin closes the process sends itself SIGTERM, then exits after five seconds.

### actix

`actix` feature.

* `pub async fn actix::handle(req: HttpRequest, host: Data<Arc<Host>>, payload: web::Payload) -> HttpResponse`: the host as an actix handler. Maps the request onto `http::Request<Bytes>` and the response's status, headers and body stream back. With `ws`, `/_sf/socket` upgrades here.
* `pub async fn actix::serve(host: Arc<Host>, addr: (&str, u16)) -> std::io::Result<()>`: serves until SIGTERM or SIGINT, then calls `begin_drain` and stops with `server.shutdown_grace` (at least one second) for open requests. actix's own signal handling is off.

### tls

`tls` feature.

* `pub struct tls::Tls`: a loaded certificate and the files it came from.
* `Tls::load(cert: PathBuf, key: PathBuf, alpn: Vec<String>) -> Result<Tls, HostError>`
* `acceptor(&self) -> tokio_rustls::TlsAcceptor`: the acceptor in force.
* `reload(&self) -> Result<(), HostError>`: re-reads both files for the handshakes that follow. Connections already up keep their certificate. A file that will not read leaves the running one in place.
* `cert(&self) -> &Path`, `key(&self) -> &Path`, `alpn(&self) -> Vec<String>`

### socket

`ws` feature. Module `socket`.

* `pub struct Row { pub key: String, pub value: Value }`: one store row.
* `Row::new(key: impl Into<String>, value: Value) -> Row`
* `pub enum On { Joined, Said(Row), Left }`: what happened on a topic.
* `pub struct Who { pub topic: String, pub session: SessionCell, pub identity: Option<Identity>, pub connection: u64 }`, `#[non_exhaustive]`: who it happened to.
  * `connection` is unique per socket.
  * `session` is the session the socket's cookie named when it opened.
* `pub struct Reply { pub everyone: Vec<Row>, pub others: Vec<Row>, pub sender: Vec<Row> }`: the rows to send and to whom. `everyone` includes the sender.
  * `Reply::everyone(rows: impl IntoIterator<Item = Row>) -> Reply`, `Reply::others(rows) -> Reply`, `Reply::sender(rows) -> Reply`
  * `Reply::default()` sends nothing.
* `pub struct Sockets`: every open socket, by topic. `Default`.
  * `Sockets::new() -> Sockets`
  * `on(&self, topic: &str) -> usize`: how many sockets a topic holds.
  * `connections(&self, topic: &str) -> Vec<u64>`: which ones.
  * `push(&self, topic: &str, rows: impl IntoIterator<Item = Row>)`: rows to every socket on the topic.
  * `push_to(&self, topic: &str, connection: u64, rows: impl IntoIterator<Item = Row>)`: rows to one socket.
  * `push_to_each(&self, topic: &str, connections: &[u64], rows: impl IntoIterator<Item = Row>)`: the same rows to several sockets.
  * A send encodes its rows once, however many sockets take them.
  * A connection leaves its topic when the browser closes the socket or the connection breaks.
* `pub type SocketHandler = Arc<dyn Fn(&Who, On) -> Reply + Send + Sync>`.
* `pub async fn serve<S>(stream: S, sockets: Arc<Sockets>, handler: SocketHandler, who: Who)` where `S` is a tungstenite `Stream` and `Sink` of `Message`: serves one upgraded connection.

### client

Module `client`: the browser client, carried in the binary and served at `client::ROUTE` unless a `StaticRoot` claims that prefix.

* Each module answers with a strong `ETag`, the first eight bytes of its SHA-256. A request naming it in `If-None-Match` answers 304 with no body.
* Outside development a hashed name answers `immutable` and a plain name answers `no-cache`.
* `pub const ROUTE: &str = "/static/js/fsr"`.
* `pub const MEDIA_TYPE: &str = "text/javascript; charset=utf-8"`.
* `pub const FILES: &[(&str, &str)]`: every module by file name, `index.js` and `boot.js` among them. The bodies are the readable build with `client_readable` on and the minified build without it.
* `pub const MINIFIED: &[(&str, &str)]`: the same modules minified, keyed by the same plain names. Always present.
* `pub const TYPES: &[(&str, &str)]`: the matching `.d.ts` declarations, which `fsr types` writes into an application.
* `pub const HAS_READABLE: bool`: `cfg!(feature = "client_readable")`. Without it a configured `readable` serves the minified build and the report's `client` row says so.
* `pub const MAPS: &[(&str, &str)]`, `client_source_maps` feature: each module's source map with its TypeScript in `sourcesContent`, keyed `<name>.js.map` for the readable build and `<name>.min.js.map` for the minified one.
* `pub fn map(name: &str) -> Option<&'static str>`: one map by the name a module's comment gives it, served at `/static/js/fsr/<name>` as `application/json`. `None` without the feature.
* `pub fn get(name: &str, minified: bool) -> Option<&'static str>`: one module by file name, from either build.
  * A `<name>.min.js` name is the minified module whatever `minified` says.
  * A name holding `/` or `\` matches nothing.
  * A hashed name answers with its module when the hash is this build's.
  * Without `client_source_maps` the module has no trailing `sourceMappingURL` comment.
* `pub fn is_hashed(name: &str, minified: bool) -> bool`: whether `name` is a module's hashed name in this build.
* `pub fn hashed_modules(minified: bool) -> Vec<(String, String)>`: each module, by the name the build's modules import it under, against its hashed URL. The hash is the first eight hex digits of the body's SHA-256, before `.js`: `/static/js/fsr/index.min.js` against `/static/js/fsr/index.min.d5e87cbc.js`.
* `pub fn with_hashed_urls(text: &str, minified: bool) -> String`: an import map with every URL under `ROUTE` naming its hashed URL, plus a row from each module's own URL to its hashed one. Text that is not a JSON object is returned as it is.
* `pub fn import_entries() -> Vec<(String, String)>`: the import map entry for every module `ROUTE` answers. `@snapfire/fsr/client` to `index.js`, `@snapfire/fsr/client/<name>` to `<name>.js`, `@snapfire/fsr` to `native.js` and `DESKTOP_SPECIFIER` to `DESKTOP_FILE`.
* `pub fn with_import_entries(text: &str) -> String`: adds `import_entries()` to an import map's `imports` wherever it has no entry of its own.
* `pub fn with_imported_entries(text: &str, imported: &[String]) -> String`: the same, for only the specifiers in `imported`.
* `pub const DESKTOP_SPECIFIER: &str = "@snapfire/fsr/desktop"` and `pub const DESKTOP_FILE: &str = "desktop.js"`: the module a page in an `fsr --desktop` window imports. Not in `FILES`, `MINIFIED`, `TYPES` or `MAPS`.
* `pub const DESKTOP_TYPES: &str`: its declaration, which `fsr types` writes to `types/@snapfire/fsr/desktop/index.d.ts`.
* `pub fn desktop(name: &str, minified: bool) -> Option<&'static str>`: `desktop.js`, `desktop.min.js` and, with `client_source_maps`, their maps. Every host serves them under `ROUTE` and `write_to` writes them.
* `pub fn desktop_entry() -> (String, String)`: its import map entry.
* `pub fn imports(name: &str, minified: bool) -> Vec<&'static str>`: the static imports of that module, read from its text. `./sibling.js` for another embedded module, a bare specifier for anything the import map resolves. A dynamic `import()` is not listed.
* `pub fn bytes(minified: bool) -> usize`: the total size of that build's modules.
* `pub fn write_to(dir: &Path, minified: bool) -> std::io::Result<Vec<PathBuf>>`: writes the modules into `dir`. With `minified` the `.min.js` names are written beside them. With `client_source_maps` the maps are written too.

## 5. Observing a Request

Module `trace`, over `fibre_tracing`. Re-exports `Trace`, `Span`, `TraceLayer` and `Traces`. Re-exports `fibre_logging::InitResult` as `LoggingGuard`.

### Collectors

* `install`, `install_with` and `observe` return no collector when a global subscriber is already set.
* `pub fn layer() -> (TraceLayer, Traces)`: the collector as a layer and its handle. Sets nothing global.
* `pub fn install() -> Option<Traces>`: the collector alone, set as the global subscriber.
* `pub fn install_with<L>(other: L) -> Option<Traces> where L: Layer<Registry> + Send + Sync + 'static`: the collector beside `other`, set as the global subscriber. Never sets the `log` bridge.
* `pub fn observe(config: &Path) -> (Option<Traces>, Option<fibre_logging::InitResult>, Option<String>)`: `fibre_logging` from `config` and the collector, on one registry.
  * The `InitResult` must be held. Its `Drop` flushes the appenders.
  * A configuration that cannot be read installs the collector alone and the third field says why.
* `pub const LOGGING_FILE: &str = "fibre_logging.yaml"`: the logging configuration `fsr serve` and `fsr dev` read beside a project's `config/`.
* `pub type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync>`.
* `pub fn project_logging<W>(root: &Path, fallback: W) -> (BoxedLayer, Option<fibre_logging::InitResult>, Option<String>) where W: for<'w> MakeWriter<'w> + Send + Sync + 'static`: `fibre_logging` from `LOGGING_FILE` under `root` when there is one. Otherwise warnings and errors to `fallback`, plus a component's `console` calls from debug up. A file that does not load falls back too and the reason is returned.
* `pub fn observe_project(root: &Path, collect: bool) -> (Option<Traces>, Option<fibre_logging::InitResult>, Option<String>)`: `project_logging` to stderr set as the global subscriber, with the collector beside it when `collect`.

### Trace Output

* `pub fn to_value(traces: &[Trace]) -> Value`: one entry per trace with `id`, `ms` and `spans`.
  * Each span carries `name`, `depth`, `at`, `ms`, its `fields` and `outcome` when it has one.
  * Durations are milliseconds to three decimal places.
  * Fields beginning `fibre.` are left out.
* `pub fn one_value(trace: &Trace) -> Value`: the single entry `to_value` writes for one trace. `/__fsr/trace/{token}` answers this.
* `pub fn tail_script(trace: &Trace) -> String`: `<script type="application/json" id="sf-trace">` holding `one_value(trace)` with `<` escaped.
* `pub fn server_timing(trace: &Trace) -> String`: a `Server-Timing` value with one entry per span below the root. Each is described by the span's `id`, `module` or `method` when it has one.

### Spans

All on target `fsr::trace`.

| Span | Where | Fields |
| --- | --- | --- |
| `request` | `Host::handle`, the root of every trace | `method`, `path`, `status`, `fibre.outcome` of `ok`, `rejected` for a 4xx or `error` for a 5xx |
| `session` | `Host::handle`, opening the session and saving it before the response leaves | `op` of `open` or `save` |
| `middleware` | `Host::handle`, the application's and a mounted site's middleware for a page or action request | none |
| `match` | resolving a path to its route's plan | `pattern`, the route the path matched |
| `source` | each plan node, in the parallel load | `id`, `node`, `fibre.outcome` of `ok` or `failed` |
| `render` | each plan node, nested as the plan nests | `module`, `cache` of `hit` or `miss` when the node is memoized |
| `call` | `TraceInterceptor`, on every transport | `service`, `method`, `cache` of `hit`, `miss` or `none`, `fibre.outcome` of the failure kind or `ok` |

## 6. The Shell

Module `shell`.

### DocumentShell

* `pub struct shell::DocumentShell { pub slot_order: Vec<String> }`. `Default`. An `Evaluator`.
* It emits `<!doctype html><html lang="<locale>" data-sf-locale="<locale>"><head>`, the `head` slot, `</head><body><div id="app">`, the `content` slot and `</div></body></html>`.
* The `<html>` element carries `data-sf-slot-order="<json array>"` when `slot_order` is not empty.
* `lang` is the `locale` prop in BCP 47 spelling, `fr-FR` for `fr_FR`. `data-sf-locale` is the prop as written. Both are `en` without the prop.

### Head Functions

* `pub fn shell::head(title: &str, styles: &[String], import_map: Option<&str>, preload: &[String], entry: Option<&str>) -> Head`: a head whose default title is `title`.
  * Its `rest` is `<meta charset>`, a viewport meta, a `<link rel="stylesheet">` per style, the import map inlined as `<script type="importmap">`, a `<link rel="modulepreload">` per `preload` href and the entry as `<script type="module" src>`, in that order.
* `pub fn shell::dev_script(bundle: &str, styles: &[String], nonce: Option<&str>) -> String`: the `<script>` a development document carries. `bundle` is the bundle id it was rendered against, `styles` the stylesheets it was rendered with and `nonce` the response's nonce.
  * It opens `EventSource("/__fsr/events")`.
  * An event whose `bundle` differs reloads the page.
  * An event naming the boot and generation the page last saw does nothing.
  * Otherwise an event whose `styles` adds or removes one sheet links or unlinks it in place. Any other change to the list reloads.
  * Then it re-links every stylesheet with a `__sf` query string and calls `window.__sf.refresh`. It reloads when nothing is registered there.
  * It shows the message of a refused build and the typechecker's findings over the page.
  * It lists each Content-Security-Policy violation with the `[document.csp]` or `[document.csp_report_only]` key that would admit it.
  * The stream closes on `pagehide` and reopens when the page returns from the back-forward cache.
* `pub fn shell::canonical(origin: Option<&str>, path: &str) -> String`: `<link rel="canonical" href="<origin><path>">`, without the origin when it is `None`. A prefixed request for the default locale carries it.
* `pub fn shell::preload_link(href: &str) -> String`: one `<link rel="modulepreload">`.
* `pub fn shell::site_entry(entry: &str) -> String`: `<script type="module" src="<entry>"></script>`, a mounted site's entry on its own routes.
* `pub fn shell::trace_meta(trace_id: &[u8]) -> String`: `<meta name="sf-trace" content="<hex id>">`.
* `pub fn shell::request_meta(token: &str) -> String`: `<meta name="sf-request" content="<token>">`.
* `pub fn shell::route_meta(pattern: &str) -> String`: `<meta name="sf-route" content="<pattern>">`, the route pattern that served the page.
* `pub fn shell::public_config(values: &ValueMap) -> String`: `<script type="application/json" id="sf-config">` holding `[public]` in the payload's JSON encoding with `<` escaped, which `@snapfire/fsr/config` reads.
* `pub fn shell::catalog_script(tag: &str, json: &str) -> String`: `<script type="application/json" data-sf-i18n="<tag>">` holding `json` with `</` escaped.

## 7. Conformance

Module `conformance`: hostile requests sent to every route a host answers before an application's pages, each answer judged by its route's class.

* `pub const AUTHORITY: &str = "app.example"`: the `Host` every probe names. It is also the origin of this site's own pages.

### Class

* `pub enum Class { BrowserWrite, HandlerWrite, DevControl, CrossOrigin, Read }`.
* `BrowserWrite`: changes state for the visitor whose session it carries. Refused from another origin, from a sibling subdomain and with a `text/plain` body or no content type.
* `HandlerWrite`: a route handler's write. Refused from another origin and a sibling subdomain. Taken from this site's origin with a `text/plain` body.
* `DevControl`: called by `fsr dev` alone, which sends no `Origin`. Refused whenever a request carries one.
* `CrossOrigin`: called from another origin by design, `POST /auth/callback`.
* `Read`: changes nothing.

### Route

* `pub struct Route { pub method: Method, pub path: String, pub class: Class, pub body: Bytes, pub content_type: Option<String> }`: a request this site's own pages send. `Debug`, `Clone`.
* `Route::read(path: impl Into<String>) -> Route`: `GET`.
* `Route::write(path: impl Into<String>, json: &str) -> Route`: a `BrowserWrite` sent as `application/json`.
* `Route::form(path: impl Into<String>, fields: &str) -> Route`: a `BrowserWrite` sent as a form. Its own-origin probe is not judged.
* `Route::action(id: &str, json: &str) -> Route`: `/_sf/action/<id>`.
* `Route::island(module: &str, json: &str) -> Route`: `/_sf/island/<module>`, with `#` percent-encoded.
* `Route::handler(path: impl Into<String>, json: &str) -> Route`: a `HandlerWrite` answering `POST`.

### Breach

* `pub struct Breach { pub method: Method, pub path: String, pub probe: &'static str, pub status: u16, pub wanted: &'static str }`: an answer that broke its route's contract.
* `Display` prints `POST /api/cart with a foreign Origin: 200, wanted a 4xx refusal`.

### framework_routes and check

* `pub fn framework_routes(host: &Host) -> Vec<Route>`: `/__fsr/sites`, `/_sf/live`, `/__fsr/trace/<token>` and the client modules. With `dev` on, also `/__fsr/traces`, `/__fsr/events` and the three control routes. With `[auth]`, also `/auth/login`, `/auth/logout` and `/auth/callback`.
* `pub async fn check(host: &Host, routes: &[Route]) -> Vec<Breach>`: sends each route a foreign `Origin`, a sibling subdomain's, this site's, HTTP/1.0 and no `Host`.
  * A write is also sent a `text/plain` body, no content type and a malformed, empty and oversized body.
  * A read is also sent as a `POST`.
  * A socket upgrade from another origin is sent once.
  * Any probe answered with a 5xx is a breach.

## 8. Error Handling

### HostError

`pub enum HostError`, `#[non_exhaustive]`.

* `Io(PathBuf, std::io::Error)`
* `Config(PathBuf, String)`: the source and the loading, deserialising or unknown-key message.
* `NoConfig(PathBuf)`: no configuration was found under the path.
* `Sitemap(String)`: `refresh_sitemap` could not build the sitemap. The one held is kept.
* `Value(String, String)`: a setting and the value that did not parse. A refused reload uses it with the reason.
* `Bind(BindError)`, transparent.
* `Import { document: String, error: ImportError }`
* `Transport(String, String)`: the client name and why its transport could not be built.
* `Service(String, String)`: a `service` whose contract disagrees with the contracts directory. The second field is the duplicate the merge refused.
* `Contract(PathBuf, String)`: a contract file that did not parse. Also a file that defines a type or service an earlier file already defined.
* `NotFound(String)`: `no route matches <path>`.
* `NoSlot(String)`: a `Fragment` named a slot the route does not have. `handle` answers it 404 with ``no slot named `<name>` on this route``.
* `Assemble(AssembleError)`, transparent.
* `Leak(String)`: the bundle under `dist/` carries a server module.
  * `build` reads `dist/.snapfire-build.json` when it exists.
  * Every module the plan's sources, actions and handlers name, plus `middleware.ts`, is server-only.
  * Each maps to its output as `app/<path>.ts` to `dist/<path>.js`. `build` refuses when an output is one of them or, by the manifest's `graph`, imports one.
  * The message lists each with its reason.
* `Mount(String, String)`: a site could not be mounted, with the site's name and why.
* `Probe(String)`: a checked reload's candidate was refused, `reload refused by a probe: <which>`.
  * `source <id>: <kind> <message>` for a warm load.
  * `<path>: the page failed with <kind>` for one of the host's own renders.
  * `site <name>, <path>[ as <subject>]: ...` for a `[site] probe` entry.
  * The check's own reason for a `reload_check`.
* `Paths(String, String)`: a route's `paths` failed during `prerender`, with the pattern and why. The body failed, returned something other than a list of objects or gave a set that left a parameter unnamed or empty.
* `Uncovered(String)`: the plan names a template module no evaluator answers. The message names the `tera` feature and `HostBuilder::evaluator`.
* `TemplateMissing(String)`: the plan names a template module the stock evaluator answers and no template of that name is under the app.
* `Template(PathBuf, String)`: a template under the app did not parse, with Tera's message.
