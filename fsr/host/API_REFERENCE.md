# API Reference: snapfire_fsr_host

The stock host: `config/` plus the build's artifacts as a `tower::Service` over `http` types.

## Contents

* [1. Configuration](#1-configuration)
  * [Deployment](#deployment)
  * [config_paths](#config_paths)
  * [locate and Located](#locate-and-located)
  * [Loader](#loader)
  * [Config](#config)
  * [RuntimeValue](#runtimevalue)
  * [AppSection](#appsection)
  * [ServerConfig](#serverconfig)
  * [TlsSection](#tlssection)
  * [socket](#socket)
  * [Text](#text)
  * [DocumentConfig](#documentconfig)
  * [Csp](#csp)
  * [ClientBuild](#clientbuild)
  * [Bundle](#bundle)
  * [PublicValue](#publicvalue)
  * [SessionSection](#sessionsection)
  * [CacheSection](#cachesection)
  * [DataCacheSection](#datacachesection)
  * [ClientConfig](#clientconfig)
  * [SecretEntry](#secretentry)
  * [StaticRoot](#staticroot)
  * [RouteDecl](#routedecl)
  * [client](#client)
  * [LocalesSection](#localessection)
  * [AuthSection](#authsection)
  * [BearerKey](#bearerkey)
  * [TypecheckSection](#typechecksection)
  * [BuildSection](#buildsection)
  * [TraceSection](#tracesection)
  * [CrawlSection](#crawlsection)
  * [DesktopSection](#desktopsection)
  * [PwaSection](#pwasection)
  * [MdxSection](#mdxsection)
  * [MountConfig](#mountconfig)
  * [SiteSection](#sitesection)
  * [SitesSection](#sitessection)
  * [DirsSection](#dirssection)
  * [ImagesSection](#imagessection)
  * [MediaSection](#mediasection)
  * [FontsSection](#fontssection)
  * [FontEntry](#fontentry)
  * [parse_duration](#parse_duration)
* [2. Building](#2-building)
  * [Artifact](#artifact)
  * [Host::from](#hostfrom)
  * [HostBuilder](#hostbuilder)
* [3. The Host](#3-the-host)
  * [Host](#host)
  * [Probe](#probe)
  * [ReloadCheck](#reloadcheck)
  * [Mount](#mount)
  * [Server-mode islands](#server-mode-islands)
  * [Locales](#locales)
  * [Resolution](#resolution)
  * [RenderMode](#rendermode)
  * [HostReport](#hostreport)
  * [ServiceProvider](#serviceprovider)
  * [ServiceSessionStore](#servicesessionstore)
  * [SiteReport](#sitereport)
  * [CrawlReport](#crawlreport)
  * [Assets](#assets)
  * [AssetsManifest](#assetsmanifest)
  * [Body](#body)
  * [PAYLOAD_ENCODINGS](#payload_encodings)
* [4. Serving](#4-serving)
  * [HostService](#hostservice)
  * [hyper](#hyper)
  * [actix](#actix)
* [5. Observing a Request](#5-observing-a-request)
  * [Installing](#installing)
  * [Reading](#reading)
  * [The Spans](#the-spans)
* [6. The Shell](#6-the-shell)
  * [DocumentShell](#documentshell)
  * [head](#head)
  * [canonical](#canonical)
* [7. Conformance](#7-conformance)
  * [Class](#class)
  * [Route](#route)
  * [Breach](#breach)
  * [framework_routes and check](#framework_routes-and-check)
* [8. Error Handling](#8-error-handling)
  * [HostError](#hosterror)

## 1. Configuration

### Deployment

* `pub struct config::Deployment { pub release_env: String, pub app_env: String, pub region: Option<String> }`, `#[non_exhaustive]`: build one with `from_env`, `Default` and the setters.
* `Deployment::release_env(self, release_env: String) -> Self`, `Deployment::app_env(self, app_env: String) -> Self`, `Deployment::region(self, region: Option<String>) -> Self`: setters for the field of the same name.
* `Deployment::from_env()`: `RELEASE_ENV` (default `development`), `APP_ENV` (default `local`), `APP_REGION` (none), an empty value counting as unset. `Default` is the same without reading the environment.

### config_paths

* `pub fn config::config_paths(dir: &Path, deployment: &Deployment) -> Vec<PathBuf>`: the stems `app`, `<release_env>`, `<app_env>`, `<region>` and `<app_env>-<region>` in that order, repeated stems once, each as `.toml` then `.yaml`, keeping only files that exist.

### locate and Located

* `pub fn config::locate(path: &Path) -> Result<Located, HostError>`: `locate_with` under `Deployment::from_env()`.
* `pub fn config::locate_with(path: &Path, deployment: &Deployment) -> Result<Located, HostError>`: a file is loaded alone, with its directory as the root or the parent when that directory is named `config`; a directory holding `config/` is a root whose sources are `config_paths` of that directory; a directory named `config` is the same with its parent as root; a directory holding `app.toml` or `app.yaml` is its own root and config directory. Anything else is `NoConfig`.
* `pub struct Located { pub sources: Vec<PathBuf>, pub dir: PathBuf, pub root: PathBuf }`
* `Located::extra(self, path: impl AsRef<Path>) -> Self` appends one file, a relative path joined onto `dir`.

### Loader

* `pub struct Loader { .. }`: how an artifact is read, the path to locate, the deployment whose ladder applies, extra files and how secrets decrypt. `Clone` and `Debug`. `From<P: AsRef<Path>>` is `Loader::at`, so `Host::from` takes either.
* `Loader::at(path: impl AsRef<Path>) -> Loader`: `path` is one of the four things `locate` accepts, under `Deployment::from_env()`.
* `deployment(self, deployment: Deployment) -> Self`: the ladder to read, over the environment's.
* `extra(self, path: impl AsRef<Path>) -> Self`: one more file after the ladder, as `Located::extra`.
* `secrets<F>(self, f: F) -> Self where F: Fn(&mut SecretOptions) + Send + Sync + 'static`: adjusts c5store's `SecretOptions` after the defaults are set. The defaults register the `base64` and `ecies_x25519` decryptors, set `secret_keys_path` to `private_keys` beside the configuration files when that directory exists (`config/private_keys` in the stock layout, the `PRIVATE_KEYS` constant) and turn `load_secret_keys_from_env` on, so `C5_SECRETKEY_<name>` holds a base64 key under the lowercased `<name>`. `SecretOptions` is re-exported from `config`.
* `keep_undecrypted(self, keep: bool) -> Self`: a `session.key` written as a secret this process cannot decrypt is listed in `Config::undecrypted` rather than refused. For a check run on a machine without the deployment's keys: `fsr doctor` without `--runtime` and the deploy tree's layout. The host never loads this way.
* `mount(&self, path: impl AsRef<Path>) -> Loader`: a loader for an artifact mounted from `path` under this loader's deployment, secrets and `keep_undecrypted`, with none of its extra files. What `snapfire_fsr_sites::mount_all` reads each site through.
* `path(&self) -> &Path`.
* `locate(&self) -> Result<Located, HostError>`: `locate_with` under the deployment, the extras appended.
* `config(&self) -> Result<Config, HostError>`: `NoConfig` when nothing was located; otherwise c5store over the sources in that order with the secrets options above, later files overriding, then `C5_*` environment variables with `__` as the level separator, then `Config::from_store`. A `.c5encval` whose decryptor or key is missing decrypts to nothing, so the section holding it fails to read and the error names the section.
* `load(&self) -> Result<Artifact, HostError>`: `config`, then `Artifact::of`.

### Config

* `pub struct Config { pub root: PathBuf, pub app: PathBuf, pub sources: Vec<PathBuf>, pub server: ServerConfig, pub document: DocumentConfig, pub session: SessionSection, pub cache: Option<CacheSection>, pub store: StoreSection, pub clients: BTreeMap<String, ClientConfig>, pub statics: Vec<StaticRoot>, pub routes: Vec<RouteDecl>, pub locales: Option<LocalesSection>, pub auth: Option<AuthSection>, pub typecheck: Option<TypecheckSection>, pub build: Option<BuildSection>, pub trace: Option<TraceSection>, pub crawl: Option<CrawlSection>, pub desktop: Option<DesktopSection>, pub pwa: Option<PwaSection>, pub mdx: MdxSection, pub site: Option<SiteSection>, pub sites: Option<SitesSection>, pub dirs: DirsSection, pub images: ImagesSection, pub media: MediaSection, pub fonts: FontsSection, pub shell_vendor: Option<PathBuf>, pub favicon: Option<String>, pub public: BTreeMap<String, PublicValue>, pub bundle: Option<Bundle>, pub inferred: Vec<String>, pub ignored: Vec<String>, pub deployment: Option<Deployment>, pub undecrypted: Vec<String>, pub secrets: BTreeMap<String, SecretEntry> }`: `secrets` the `[secrets.<name>]` tables, one declared with no keys included; `deployment` the one whose ladder `Loader::config` merged, `None` from `from_store`; `undecrypted` the secrets a `keep_undecrypted` loader could not decrypt; `public` is `[public]` as written, `ignored` the top-level keys outside the host's sections, left for the application's own store, `bundle` the module graph read out of the build facts, `shell_vendor` the shell's `app/vendor/` when `[site] shell` names a `generated/shell.json` beside one, which a host built from this configuration serves at `/static/js/vendor` and a mount never reads, `favicon` the static path `/favicon.ico` is answered from, `<icons route>/favicon.ico`, when the icons directory holds that file.
* `Config::load(path) -> Result<Config, HostError>`: `Loader::at(path).config()`.
* `Config::from_store_at<S: C5Store>(store: &S, root: impl AsRef<Path>) -> Result<Config, HostError>`: the same over a store the caller loaded and a root it names, for an application running the host inside itself: `Config::from_store_at(&store.branch("fsr"), root)`. Nothing here reads the filesystem for configuration.
* `Config::from_store<S: C5Store>(store: &S, located: Located) -> Result<Config, HostError>`: `from_store_at` over `located.root`, keeping `located.sources` as the configuration's provenance and the path its errors report. Reads the sections `app`, `server`, `document`, `session`, `cache`, `store`, `secrets`, `clients`, `static`, `locales`, `auth`, `typecheck`, `build`, `trace`, `site`, `sites`, `public`, `dirs`, `images` and `fonts`, leaving any other top-level key alone and naming it in `ignored`, requires `session`, refuses a `public` value that is not a scalar or a `public` key that is not an identifier, refuses an `auth.provider` outside `PROVIDERS`, an `auth.login` that is not a path and a `trace.expose` or `trace.inline` entry that is not a path or not a route pattern, then infers: a static root for `dist` at the build facts' `publicPath`, `document.entry` as `<publicPath>src/main.js` when the facts list that entry, `document.import_map` from `importmap.json`, `/static/js/vendor` from `vendor/`, under the site's prefix when `site` is set, `/static/css` and `/static/icons` from the directories `dirs` names, `/favicon.ico` from `favicon.ico` under the icons directory, `shell_vendor` from the shell's `app/vendor/` when `site.shell` names a `generated/shell.json` beside one, `/static/css` from `styles/` with `document.styles` as every `.css` file in it sorted by name followed by every `.css` the build facts list under `styles/themes/`, plus each client's `document` as `clients/<name>.openapi.json`. Written values win; every inference is listed in `inferred`.
* `Config::resolve(&self, relative: &str) -> PathBuf` joins onto `app`.
* `Config::config_dir(&self) -> PathBuf`: the directory of the first file loaded, the project root when none; where `auth.users` resolves.
* `Config::session_ttl(&self) -> Result<Duration, HostError>`.
* `Config::dev(&self) -> bool`: `server.dev` when written, else whether the loaded `deployment` is `development`; `Deployment::from_env()` when `deployment` is `None`.
* `Config::runtime_values(&self) -> Vec<RuntimeValue>`: the values a deployment must hold that its files may leave to the environment, each with its state: then `secrets.<name>.value` for each secret `sent_secrets` names, `Set`, `Encrypted` or `Missing` in every deployment; `session.key` is `Encrypted` when listed in `undecrypted`, `Placeholder` when it is `SESSION_KEY_PLACEHOLDER`, `Missing` when absent outside development and `Set` otherwise. The host refuses a placeholder at boot; `fsr doctor` reports the rest.
* `Config::cache_ttl(&self) -> Result<Option<Duration>, HostError>`: `None` without a `[cache]` section; a lifetime nobody can parse is `HostError::Value("cache.ttl", ..)`.
* Every table refuses unknown keys.

### RuntimeValue

* `pub struct config::RuntimeValue { pub key: String, pub env: String, pub state: ValueState }`, `#[non_exhaustive]`: `RuntimeValue::new(key, state)` fills `env` with `env_name(key)`.
* `pub enum config::ValueState { Set, Placeholder, Encrypted, Missing }`, `#[non_exhaustive]`.
* `Config::sent_secrets(&self) -> Vec<&str>`: the secrets a client that is not a mock names in its `credential`, sorted.
* `pub fn config::config_schema() -> serde_json::Value`: the merged configuration as a JSON Schema (draft 2020-12), one property per section in `host_sections()` typed from the type the host reads it into, `additionalProperties: true` at the top for an application's own tables, each table refusing a key its type does not declare. A value the deployment's environment may set carries `x-runtime` with its `C5_` variable. `fsr build` writes it to `generated/config.schema.json`; nothing in the host reads it.
* `pub fn config::env_name(key: &str) -> String`: the `C5_` variable c5store layers over the files for `key`, each part uppercased and joined with `__`: `session.key` is `C5_SESSION__KEY`.
* `pub fn config::host_sections() -> &'static [&'static str]`: the top-level sections the host reads; any other is in `Config::ignored`.

### AppSection

* `dir` (default `app`), relative to the project root.

### ServerConfig

* `listen` (default `127.0.0.1:8080`), `plan` (default `generated/plan.sexp`), `contracts` (default `generated/contracts`), a directory whose `*.json` files are merged in name order at boot.
* `prerender: Option<String>`: the directory, relative to the app, that `prerender` writes and the host reads; absent by default.
* `dev: Option<bool>`: whether the document carries the live-refresh script and the host answers `/__fsr/events` and `/__fsr/changed`; absent, it follows `RELEASE_ENV`.
* `render: Option<String>`, unset by default: any value is `HostError::Config` naming the plan, since every page renders on the server and every island in the browser. It is read only so that a configuration naming it is refused with that reason rather than as an unknown key.
* `max_body: usize` (default 1048576): the most bytes a request body may carry. `serve` stops reading a larger body and answers 413 with a text naming the key; `handle` answers the same for a body handed to it whole, before a session is opened.
* `max_upload: usize` (default 0, no cap beyond `max_body`): the most bytes one part of a `multipart/form-data` body may carry. A part over it answers 413 with a `{kind, message}` body naming the field and the setting, before the action's body runs and before the CSRF token is read.
* `static_max_age: Option<u64>` (default unset): how long a browser may reuse, without asking again, a file from a `[[static]]` root whose name carries no content hash or a module of the embedded client. Unset sends `Cache-Control: no-cache`, so a deploy never pairs fresh HTML with a module the browser kept from the one before; the answer is a 304 while the file is unchanged. A number sends `public, max-age=<n>` and `0` sends no `Cache-Control` at all. Ignored under `dev`, where every static answer is `no-cache`. A module or stylesheet of the application's bundle is never kept by this lifetime: outside `dev` the host names it by its content and its own name answers `no-cache` (see [Bundle](#bundle)).
* `http2: bool` (default false): whether a served connection negotiates HTTP/2 as well as HTTP/1.1. The listener carries no TLS and so no ALPN, which makes this h2c: a client opening with the HTTP/2 preface is served and a browser, which speaks HTTP/2 only over TLS, is not. `HostBuilder::http2` overrides it for a host built in Rust. Nothing else changes: the same `Host::handle` answers both versions.
* `shutdown_grace: u64` (default 20): seconds `serve_listener` waits after SIGTERM or SIGINT for the connections it holds to finish what they are answering. It stops accepting at once and an idle keep-alive connection closes at once. Whatever is still open when the time runs out is cut and logged.
* `health: String` (default `/health`) and `ready: String` (default `/ready`): the paths a liveness and a readiness probe ask. `handle` answers a GET or HEAD on either with 200 `ok` and `Cache-Control: no-store` before statics, middleware and sessions. `ready` answers 503 `shutting down` once `Host::begin_drain` has been called, which `serve_until` and `actix::serve` do when their shutdown begins. An empty string turns a probe off. A route the plan or a mounted site serves at the same path is a `HostError::Config` at boot.
* `tls: Option<TlsSection>`, the `[server.tls]` table: absent, the listener is plain TCP. Present, the host must be built with the `tls` feature or `build` is a `HostError::Config` saying so, since a configured certificate must never be answered with plaintext.

### TlsSection

* `dir: Option<String>`: the directory `cert` and `key` are read under. Absent, each resolves against the project root; an absolute path is taken as written.
* `cert` (default `cert.pem`): the certificate chain in PEM, leaf first.
* `key` (default `key.pem`): the private key in PEM, PKCS#8, PKCS#1 or SEC1.
* `alpn: Option<Vec<String>>`: what the handshake offers, in order. Absent, `["h2", "http/1.1"]` when `server.http2` is on and `["http/1.1"]` when it is not.
* `reload` (default `hup`): the signal that re-reads both files and swaps the certificate for the handshakes that follow. `hup`, `usr1`, `usr2` or `none`; anything else is a `HostError::Config` naming the four. Unix only: elsewhere a new certificate needs a restart.
* `files(&self, root: &Path) -> (PathBuf, PathBuf)`: the two paths as resolved.

### Text

`pub enum Text { Literal(String), Keyed { key: String, default: String } }`. `Debug`, `Clone`, `PartialEq`, `Eq`, `Default` (an empty literal), `Deserialize` from a string or a table with `key` and `default`. Text the document carries: a literal or a message key read from the catalog of the request's locale with the literal it falls back to.

* `pub fn literal(&self) -> &str`: the text itself or what a key falls back to.
* `pub fn is_keyed(&self) -> bool`
* `pub fn resolve(&self, catalogs: &Catalogs, locale: &str) -> String`: the text under `locale`; a key reads `catalogs`, which answers from the default locale's table when `locale` has none; absent there too, the literal.

### DocumentConfig

* `title: Text` (default empty), `title_template: Option<Text>`, how a route's title is written into the document, `{page}` the route's and `{title}` the document's, absent meaning as given, `entry: Option<String>`, `import_map: Option<String>` and `styles: Option<Vec<String>>`, stylesheet URLs linked in order, all three inferred when absent, `shell` (default `shell#document`), `origin: Option<String>`.
* `module_preload: bool` (default false): whether the head carries a `<link rel="modulepreload">` for every module the page fetches before the first island can mount, which is `entry`'s own static imports out of `Config::bundle`, the import map's answer for each bare specifier the bundle declares and whatever the embedded client reaches from those. A module an island pulls in with `import()` is left out, since which islands a document holds is not known until it renders. `entry` itself is left out too, being a `<script type="module">` already. A mounted site adds its own links beside its entry script, minus whatever the shell already covers.
* `client: ClientBuild` (default `auto`): which build of the embedded client `/static/js/fsr/<name>.js` answers with.
* `empty_icon: bool` (default true): when nothing under `icons/` gives the head an icon, `head` gains `<link rel="icon" href="data:,">`, so a browser does not request `/favicon.ico` and log the 404. The document drops it when a route's `meta` names any other icon. With a `csp` or `csp_report_only`, `data:` is added to `img-src` through `Csp::widen`. `false` turns it off.
* `csp: Option<Csp>`: `[document.csp]`, the `Content-Security-Policy` every HTML response carries, as directives and their sources. The host merges in the sources only it knows: the inline import map's hash into `script-src`, plus under `dev` a nonce for its own refresh script, whose text carries the bundle id it was rendered against and so has no stable hash. A policy with `style-src` or `default-src` and no `style-src-attr` gains `style-src-attr 'unsafe-inline'`, so a server-rendered `style` attribute applies; a declared `style-src-attr` is left as written. It adds them through `Csp::widen`, so a policy that leaves `script-src` to `default-src` keeps `default-src`'s sources and a policy restricting neither stays unrestricted. A policy therefore never names what the host emitted. Absent, no policy is sent and whatever sits in front of the host owns the header. A payload and a fragment carry none, the first being data for a page that already has one and the second being written into one.
* `csp_report_only: Option<Csp>`: `[document.csp_report_only]`, the same composed the same way and sent as `Content-Security-Policy-Report-Only`. Both may be set at once, which is how a policy is rolled onto a page carrying third-party scripts: observe first, then enforce.
* `head: Vec<BTreeMap<String, String>>` is not a key: it holds what the host inferred from `icons/`, which a route's `meta` folds over. `head_meta(&self) -> Result<Meta, HostError>` is that as the outermost `Meta`.

### Csp

* `pub struct config::Csp(pub BTreeMap<String, Vec<String>>)`, one directive per key and its sources as a list, deserialised straight from `[document.csp]`.
* `pub fn add(&mut self, directive: &str, source: impl Into<String>)`: adds a source once, which is how the host contributes what a policy cannot name for itself.
* `pub fn widen(&mut self, directive: &str, source: impl Into<String>)`: adds a source without narrowing the directive. A directive the policy leaves to `default-src` starts from `default-src`'s sources; a policy naming neither is left alone, since it restricts nothing.
* `pub fn declares(&self, directive: &str) -> bool`: whether the policy has the directive at all.
* `pub fn names(&self, directive: &str, source: &str) -> bool`.
* `pub fn header(&self) -> Option<String>`: the header value, `directive source source; directive source`. A directive with no sources is written bare, for the likes of `upgrade-insecure-requests`. `None` when empty, since a policy naming nothing forbids everything.

### ClientBuild

* `pub enum config::ClientBuild { Auto, Readable, Minified }`, `document.client` as `"auto"`, `"readable"` or `"minified"`. The minified modules are about a third smaller and import their siblings by `.min.js`, which the host answers either way, so the choice is what a page's entry point gets and the rest of the graph follows it.
* `pub fn minified(self, dev: bool) -> bool`: `Auto` is minified unless `dev`; the other two ignore `dev`. Always true in a binary built without the `client_readable` feature, there being nothing else to answer with.

### Bundle

* `pub struct config::Bundle { pub route: String, pub graph: BTreeMap<String, Vec<String>>, pub externals: Vec<String>, pub minified: Option<String>, pub outputs: BTreeSet<String>, pub styles: Vec<String>, pub dir: PathBuf, .. }`: the bundle's own terms, read from `dist/.snapfire-build.json` when it was found. `route` is the URL prefix it is served under with no trailing slash, `graph` each entry's transitive static imports in the output directory's own terms, `externals` the bare specifiers the bundle carries, `minified` the suffix of each output's minified twin when the bundle was built minified, `outputs` every file the compiler wrote, `styles` the sheets of the Vue and Svelte components it compiled and `dir` the directory they are in: the `[[static]]` root that serves `route` when one does, as in a tree `fsr bundle` wrote, else the directory the facts file was read from. `None` on `Config` when no facts file was read. The input to `document.module_preload`.
* `Bundle::minified_url(&self, url: &str) -> String`: `url`'s minified twin when `url` is under `route` and `outputs` holds the twin, `url` as given otherwise.
* `Bundle::hashed(&self) -> &BTreeMap<String, String>`: each `.js` and `.css` output against its name carrying the first eight hex digits of its content's SHA-256, `src/main.3f2a91c0.js` for `src/main.js`. Read from `dir` on the first call; an output that cannot be read is left out.
* `Bundle::served_url(&self, url: &str) -> String`: `url`'s minified twin, then its hashed name. A host outside development names every URL of the bundle this way: the entry, the stylesheets, the preload links and the import map's values.
* `Bundle::hashed_modules(&self) -> Vec<(String, String)>`: each served module's own URL against its hashed one. A host outside development adds these to the import map, so a module importing a sibling by relative path loads it by its hashed name.

Outside development a hashed name answers the output it names with `Cache-Control: public, max-age=31536000, immutable`, a hashed name the bundle no longer holds is a 404 and the output's own name answers `no-cache` whatever `static_max_age` says.

### PublicValue

* `pub enum PublicValue { Str(String), Int(i64), Float(f64), Bool(bool) }`: one `[public]` value as written.
* `ts(&self) -> &'static str`: the TypeScript type a body sees it as; `to_value(&self) -> Value`: what `ctx.config` carries; `Display` prints it the way the boot report does.

### SessionSection

* The whole section is optional; absent, every field takes its default.

* `key: Option<String>`: absent, `HostBuilder::build` fills it with a generated key on a development host and refuses any other that builds its own key ring with `HostError::Config`; `config::SESSION_KEY_PLACEHOLDER` is refused on any host and a key written but empty, a `.c5encval` that did not decrypt, refuses the load. `previous_keys: Vec<String>` (default empty), keys that still verify a cookie signed before `key` replaced them, in order. Together they fill the `Keyring` the stock codec and a `derived` scheme sign with, `key` first. `store` (default `memory`; `service` keeps every record behind the client `client` names, over `ServiceSessionStore`; any other value is `HostError::Value`), `client: Option<String>` (required with `service` and must be a `[clients]` entry, else `HostError::Config`), `ttl` (default `8h`, the length of a new session; only `SessionCell::extend` moves the end), `capacity` (default 4096), `secure` (default false), `csrf` (default `identified`; `always` mints the token for any session whose page renders it and establishes a fresh session on the response that rendered it, so a page rendering no token sets no cookie; any other value is `HostError::Config`), `csrf_scheme` (default `single_use`; `session` or `derived`; any other value is `HostError::Config`), `csrf_outstanding` (default 8, at least 1; how many single-use tokens stay valid at once).

### CacheSection

* `capacity: u64` (default 1000), `ttl: String` (default `1m`). Present at all means `build` installs `FibreCache::bounded(capacity, ttl)` on the app; absent means nothing is cached.
* `data: Option<DataCacheSection>`: present means `build` installs the service layer's `DataCache` over every method whose contract declares `cache`; absent means no method is cached.

### StoreSection

* `slot_order: Vec<String>` (default empty): `[store] slot_order`, the slot names that settle a store key two parallel slots at one depth both seed, the later name winning and `content` standing for the page. A slot the list leaves out loses to every slot it names and stands in name order among the others; empty, name order decides throughout. The host hands it to the runtime, which renders by it, then writes it on the document as `data-sf-slot-order`, a JSON array, so the browser store merges by the same rule. An empty name or a name listed twice is `HostError::Config`. A mounted site's own `[store]` is not read; `fsr doctor` reports one.

### DataCacheSection

* `capacity: Option<u64>`: entries per policy, `cache.capacity` when absent. Unknown keys are refused.

### ClientConfig

* `document: Option<String>`, inferred as `clients/<name>.openapi.json` when absent, falling back to `clients/<name>.proto` or `clients/<name>.graphql` when only that file exists; `base_url: Option<String>`, required unless the transport is `mock`. A `.proto` document is reached with `GrpcTransport`, a `.graphql` one with `GraphqlTransport` posting to `base_url` and anything else with `HttpTransport`. The table key is the service name. `bearer: Option<BearerKey>`: which custody entry the client's calls carry as a bearer token; absent, none.
* `transport: Option<String>`: `mock` answers from `responses` over a `MockTransport` and reaches nothing; any other value is `HostError::Value`. `responses: Option<String>`: the mock's file, relative to the app directory, `clients/<name>.mock.json` when absent; an object of method name to a response in the payload's JSON encoding or to `{"$fail": {"kind": "<FailureKind>", "message": "..."}}`.
* `credential: Option<CredentialConfig>`: an application credential the client's transport sends on every call. `pub struct CredentialConfig { pub secret: String, pub header: Option<String>, pub query: Option<String> }`: `secret` names a `[secrets.<name>]`; without `header` or `query` the value goes out as `authorization: Bearer <value>`, with `header` as that header's value, with `query` as that query parameter beside the arguments; a `.proto` client sends it as call metadata. Loading refuses a credential naming an undeclared secret, one with both `header` and `query`, `query` on a `.proto` client and an `authorization` credential on a client with `bearer` set. A mock client sends nothing.
* `is_mock(&self) -> bool`; `responses_file(&self, name: &str) -> String`.

### SecretEntry

* `[secrets.<name>]`: `description: Option<String>`, `value: Option<Secret>`. The value comes from `C5_SECRETS__<NAME>__VALUE` or a `.c5encval`; written as a TOML string in a loaded file it refuses to load outside development, naming the variable. A value that does not decrypt refuses to load unless `Loader::keep_undecrypted`, which lists `secrets.<name>.value` in `Config::undecrypted`.
* `pub struct Secret`: `Secret::new(value)`, `expose(&self) -> &str`; `Debug` prints `Secret(***)`.
* The host refuses to start when a secret `Config::sent_secrets` names has no value, naming its variable.

### StaticRoot

* `route: String`, `dir: String` relative to the app directory. A trailing slash on `route` is ignored. A written root with the same `route` as an inferred one replaces it. A root at `/` answers only the paths that name a file under `dir`; every other path goes on to the routes.

### RouteDecl

* `pattern: String`, a `[[route]]` entry: a pattern the application registers with `HostBuilder::route`. A declared pattern no route registers is listed in `HostReport::unanswered_routes` and printed on a `routes` row.

### client

The browser half of FSR, carried by the binary and served at `client::ROUTE`, `/static/js/fsr`, unless a `StaticRoot` claims that prefix. Each module answers with a strong `ETag`, the first eight bytes of its SHA-256, and a request naming it in `If-None-Match` is answered 304 with no body.

* `pub const ROUTE: &str`, the prefix; `pub const MEDIA_TYPE: &str`, what a module is served as.
* `pub const FILES: &[(&str, &str)]`: every module by file name, `index.js` through `values.js`, `template.js` among them. The bodies are the readable build with `client_readable` on and the minified one without it, so a binary carries one set of modules rather than two.
* `pub const MINIFIED: &[(&str, &str)]`: the same modules minified, keyed by the same plain names. Always present: a minified module imports its siblings as `./boot.min.js`, so those names have to be answered whatever is configured.
* `pub const TYPES: &[(&str, &str)]`: the matching declarations, which `fsr types` writes into an application. A `.d.ts` has no minified twin.
* `pub const HAS_READABLE: bool`: whether the readable build is in this binary, `cfg!(feature = "client_readable")`. The feature is on by default; without it a binary is about 215 KiB smaller and a configured `readable` serves the minified build instead, which the report's `client` row names.
* `pub fn get(name: &str, minified: bool) -> Option<&'static str>`: one module by file name, from whichever build. A `<name>.min.js` request is the minified module whatever `minified` says, since only a minified module asks for one and it is asking for its own sibling. A name holding `/` or `\\` matches nothing, so the prefix is the whole of what it answers. Without the `client_source_maps` feature the module comes without its trailing `sourceMappingURL` comment, since the binary carries no maps.
* `pub const MAPS: &[(&str, &str)]`, with `client_source_maps` only: each module's source map with its TypeScript in `sourcesContent`, keyed `<name>.js.map` for the readable build and `<name>.min.js.map` for the minified one. About 1.3 MB.
* `pub fn map(name: &str) -> Option<&'static str>`: one of those by the name a module's comment gives it, served at `/static/js/fsr/<name>` as `application/json`; `None` without the feature.
* `pub fn imports(name: &str, minified: bool) -> Vec<&'static str>`: the specifiers that module imports, `./sibling.js` for another embedded module and a bare specifier for anything the page's import map resolves, read out of the module text. A dynamic `import()` is not one of them. This is what `document.module_preload` walks, so it follows the build being served.
* `pub fn bytes(minified: bool) -> usize`: what the modules of that build come to.
* `pub fn write_to(dir: &Path, minified: bool) -> std::io::Result<Vec<PathBuf>>`: writes them into `dir`, which is what `fsr bundle` does. With `minified` it writes the `.min.js` spellings beside them, since a tree answering the prefix from disk needs the names the minified graph asks for. With `client_source_maps` it writes the maps too, as `Host::prerender` does beside the documents.
* `config::ClientBuild` picks between the two builds at boot.

### LocalesSection

* `pub struct locale::LocalesSection { pub supported: Vec<String>, pub default: Option<String>, pub order: Vec<String>, pub remember: bool, pub cookie: String }`, `#[non_exhaustive]`, the `[locales]` section. `LocalesSection::new(supported: Vec<String>) -> Self` gives every other field its default. `supported` is at least one tag of letters, digits, `_` and `-`, spelled as the application wants to see it; `default` is among them, the first when absent; `order` names the sources consulted, any subset of `prefix`, `cookie` and `header`, all three in that order when absent; `remember` is false by default; `cookie` is `sf_locale` by default. Checked at `build`, which fails with `HostError::Config` naming the offence.

### AuthSection

* With the `auth` feature off, a configuration carrying `[auth]` makes `build` a `HostError::Config` naming the feature; an `oidc` or `passkey` method without its feature does the same.
* `pub struct config::AuthSection { pub provider: Option<String>, pub login: String, pub users: Option<String>, pub client: Option<String>, pub methods: Vec<MethodSection>, pub second_factor: Option<String> }`, the `[auth]` section. One method is `provider` with its `users` or `client`; several are `[[auth.methods]]`. Naming both or neither is refused. `provider` is one of `config::PROVIDERS`, `["file", "service", "oidc", "passkey"]`; `login` is the application's login page, `/login` by default and must start with `/`; `users` is the `file` provider's table, `auth.toml` by default, relative to `config_dir` and read on its own rather than through the ladder; `client` is the `[clients]` entry a `service` provider sends its steps to, required with it and checked against the table. `second_factor` names one of two or more methods that every sign-in through the others must also pass, built as `Sequence` over `Methods` of the rest; without it several methods are `Methods` in their order. `fn all_methods(&self) -> Vec<MethodSection>` is every method, the single `provider` as one named after itself.
* `pub struct config::MethodSection { pub name: String, pub provider: String, pub users: Option<String>, pub client: Option<String>, pub issuer: Option<String>, pub preset: Option<String>, pub tenant: Option<String>, pub client_id: Option<String>, pub client_secret: Option<String>, pub scopes: Option<Vec<String>>, pub claims: BTreeMap<String, String>, pub team_id: Option<String>, pub key_id: Option<String>, pub signing_key: Option<String>, pub store: Option<String>, pub rp_name: Option<String>, pub verification: Option<String> }`, one `[[auth.methods]]` entry, `name` being what `/auth/login?method=` names; a name used twice is refused. `AuthSection` carries the same `oidc` fields for a single method, named after its `preset` or `oidc`.
* An `oidc` method needs `client_id` and `issuer` or a `preset` of `config::PRESETS`, `["google", "microsoft", "apple"]`; `apple` also needs `team_id`, `key_id` and `signing_key`. `client_secret` and `signing_key` name `[secrets]` entries, checked against the table. The provider is `snapfire_fsr_oidc::OidcProvider` with redirect URI `<document.origin>/auth/callback` (in development without an origin, `http://<server.listen>`); `microsoft` uses `tenant`, `common` when absent; `scopes` and `claims` override the scopes and add renamed claims. In development, while a named secret has no value, the method is `DevProvider::stand_in` over its `users` table and the boot report says `oidc <preset or issuer>, a stand-in until secrets.<name> has a value`; outside development that is a boot error naming `C5_SECRETS__<NAME>__VALUE`, as is a missing `document.origin`.
* A `passkey` method is `snapfire_fsr_passkey::Passkeys` over `FilePasskeys` at `store` relative to `config_dir`, `passkeys.json` when absent. The relying party is `document.origin`'s host and the origin itself; in development without an origin, `localhost` and `http://localhost:<port of server.listen>`; outside development a missing origin is a boot error. `rp_name` is `document.title` when absent, else the RP ID. `verification` is `required` or `preferred`, refused otherwise. The boot report names it `passkey for <rp id>`.

### BearerKey

* `pub enum config::BearerKey { Toggle(bool), Named(String) }`, untagged: `true`, `false` or a string in the file.
* `key(&self) -> Option<&str>`: `access_token` for `true`, the string for `Named`, `None` for `false`.

### TypecheckSection

* `pub struct config::TypecheckSection { pub version: Option<String>, pub sha512: Option<String>, pub tsc: Option<String>, pub enabled: Option<bool> }`, the `[typecheck]` section. The host reads none of it; `fsr` does, when it spawns `snapfiretc` over the application. `version` is the TypeScript a build checks with, the checker's own default when absent and `fsr` writes the one it resolved here when the file names none; `sha512` is the integrity a fetch of a version the checker pins no hash for must match; `tsc` is a compiler to use as given; `enabled` is whether a build checks types at all, on when absent.

### BuildSection

* `pub struct config::BuildSection { pub strict: Option<bool>, pub target: Option<String>, pub lib: Option<Vec<String>>, pub before: Vec<String>, pub after: Vec<String> }`, the `[build]` section. The host reads none of it; `fsr` does. `before` and `after` are Cargo projects, relative to the project root, that `fsr build` and `fsr dev` run in order ahead of the app build and once it is built; empty is none. A key the section does not take and an empty project name refuse the configuration. `strict` refuses a build that leaves a component to render in the browser alone, off when absent. `target` and `lib` are what every tsconfig the build writes carries, `config::DEFAULT_TARGET` (`es2022`) and the target's own library when absent. A target older than `es2017`, a name that is not `esnext` or `es` and a four-digit edition and an empty `lib` entry refuse the configuration.

### TraceSection

* `pub struct config::TraceSection { pub expose: Vec<String>, pub inline: Vec<String> }`, the `[trace]` section. `expose` lists the paths whose requests carry an `x-sf-request` token and whose trace `GET /__fsr/trace/{token}` reads back. `inline` lists the paths whose HTML pages end with their own trace. Entries are `matchit` route patterns: `/` is the home page alone and `/docs/{*rest}` the paths under `/docs`. Each is empty when not written.

### CrawlSection

* `pub struct config::CrawlSection { pub robots: Option<String>, pub sitemap: Option<String>, pub disallow: Vec<String> }`, the `[crawl]` section. `robots` and `sitemap` are the paths each file is served at, absent when it is not served; `disallow` the paths `robots.txt` disallows beside `/__fsr/`, `/_sf/` and `/auth/` when `[auth]` is set. Loading refuses a value that is not a path, a section naming neither file and a section without `document.origin`. On a host with `[site]` the sitemap is served under the prefix and `robots` is not served.

### DesktopSection

* `pub struct config::DesktopSection { pub name: Option<String>, pub serve: DesktopServe, pub cookies: DesktopCookies }`, the `[desktop]` section. `name` leads the title of the window `fsr --desktop` opens, before the page's own title. `serve` is `DesktopServe::Loopback` (`"loopback"`, the default) or `DesktopServe::Scheme` (`"scheme"`); `cookies` is `DesktopCookies::Memory` (`"memory"`, the default) or `DesktopCookies::Disk` (`"disk"`). Loading refuses any other value. The host reads none of it.


### PwaSection

* `pub struct config::PwaSection { pub name: Option<String>, pub short_name: Option<String>, pub description: Option<String>, pub theme_color: Option<String>, pub background_color: Option<String>, pub display: Option<String>, pub start_url: Option<String>, pub file: Option<String>, pub worker: Option<String>, pub offline: Option<String> }`, the `[pwa]` section. Loading refuses `file` beside any manifest field and a section with neither `file` nor `name`.
* `/manifest.webmanifest` answers `application/manifest+json`: the file `file` names, else a manifest from the fields, `start_url` `/` and `display` `standalone` when absent, its `icons` each PNG, WebP or SVG in the icons directory whose name carries `<w>x<h>`, smallest first, an SVG without one as `any`.
* `/sw.js` answers outside development only: the file `worker` names, else a worker the host writes at boot. It precaches the bundle's hashed names as `sf-build-<version>`, keeping the previous build's set; warms the client, the import map's local URLs, the document's other stylesheets, the manifest and `offline` into `sf-runtime`; answers the bundle cache first and every other same-origin GET network first, keeping a response that is not `private` or `no-store` and the document of a kept payload. It leaves `/_sf/`, `/__fsr/` and non-GET requests alone. Both routes send `Cache-Control: no-cache`.
* Every page carries `<link rel="manifest" href="/manifest.webmanifest">` in place of any inferred manifest link, `<meta name="theme-color">` from `theme_color` unless a head row sets one and, outside development, `<meta name="sf:worker" content="/sw.js">`.
### MdxSection

* `pub struct config::MdxSection { pub gfm: bool, pub heading_ids: bool, pub frontmatter: bool, pub highlight: Option<bool>, pub collections: BTreeMap<String, String> }`, the `[mdx]` section, `Default` with `gfm` and `heading_ids` true, `frontmatter` false, `highlight` absent and no collection. The host reads none of it; `fsr` does. Loading refuses a key outside these five, a flag that is not `true` or `false` and a `[mdx.collections]` value that is not a directory under the app, absolute or reaching `..`.

### SiteSection

* `pub struct SiteSection { pub name: String, pub at: String, pub shell: Option<String>, pub probe: Vec<Probe> }`: `[site]`. `name` is lowercase letters, digits, `_` and `-`; `at` a path with no trailing slash and no parameter; `shell` the path, against the project root, of the shell contract the site's build reads; `probe` the paths a shell renders against a candidate holding this site before it swaps in, see [Probe](#probe). `Deserialize`, `Clone`, `PartialEq`, `Eq`.
* `prefix(&self) -> String`: `<name>:`. `under(&self, path: &str) -> String`: `at` joined with a path, `/` being `at` itself.
* An application with `[site]` and `[sites]` is refused. Its `styles/` root is inferred under `<at>/static/css`.

### SitesSection

* `pub struct SitesSection { pub root: Option<String>, pub poll: Option<String>, pub store: Option<String>, pub mounts: BTreeMap<String, MountConfig> }`, `#[non_exhaustive]`: `[sites]`. `root` is where `name@version` artifacts resolve, required by any mount spelled that way and by `store`; `poll` a duration or absent; `store` a directory of archives or an `http(s)` URL that `snapfire_fsr_sites` fetches a missing version from. `root`, `poll` and `store` are not mount names.

### MountConfig

* `pub struct MountConfig { pub artifact: Option<String>, pub hash: Option<String>, pub allow_engine: bool }`, `#[non_exhaustive]`: `[sites.<name>]`. `MountConfig::new(artifact: Option<String>, hash: Option<String>, allow_engine: bool) -> Self`. `artifact` is `name@version` under the root or a path against the project root; `hash` pins the content hash; `allow_engine` admits an artifact with engine-owned rows. A row without `artifact` or with any other key is refused at load.

### DirsSection

`[dirs]`: where each kind of asset lives under the app directory. Every key defaults to the conventional name, so an application that writes nothing changes nothing. The routes stay fixed: `/static/css`, `/static/icons` and the bundle's public path answer whatever directory a key names.

* `pub struct DirsSection { pub styles: String, pub fonts: String, pub images: String, pub icons: String, pub public: String }`, `#[non_exhaustive]`, defaults `styles`, `fonts`, `images`, `icons` and `public`.
* `public`, when the directory exists and the configuration is not a site's, is inferred as a static root at `/` unless a written root is already at `/` or already serves the directory. A static root at `/` answers a path only when it names a file under the directory, read percent-decoded with no `.` or `..` component. It is tried after every other root.
* `styles`, `fonts` and `icons` are scanned, so the key moves where discovery looks. `images` are found by reference, so the key is the directory `fsr build` scans into the asset manifest for a template's `fsr_picture` to name.

### MediaSection

* `pub struct MediaSection { pub embed_stills: EmbedStills }`, `[media]`, `#[non_exhaustive]`. `pub enum EmbedStills { Provider, Local }`, `provider` by default: an `Embed`'s still loads from the provider's CDN. With `local` the build fetches it once into `generated/embeds/` and the page serves it, falling back to the provider's when the fetch fails or the `id` is computed.

### ImagesSection

`[images]`: the variant policy every image is built under and how a remote one is addressed.

* `pub struct ImagesSection { pub widths: Vec<u32>, pub formats: Vec<String>, pub quality: BTreeMap<String, u8>, pub rewrite: bool, pub strip: bool, pub base: Option<String>, pub sources: BTreeMap<String, ImageSource> }`, `#[non_exhaustive]`.
* `widths` (default `[640, 960, 1280, 1920, 2560]`), each at least 1; `formats` (default `["avif", "webp"]`), each `avif` or `webp`; `quality` keyed by format, 0 to 100, a format left out keeping its default of 60 for AVIF and 80 for WebP.
* `rewrite` (default `true`): whether an `<img>` whose `src` is an imported asset's is lowered as a `Picture`.
* `strip` (default `true`): whether the served original of a JPEG, PNG, WebP or GIF is `snapfire_media::strip::strip` of the source, keeping the EXIF orientation, creator and copyright and the ICC profile. Off, the original is the source as saved.
* `base`: `scheme://host/path` with no trailing slash, prefixed onto every emitted image URL at build time for a static tree a CDN serves.
* `sources`: `[images.sources.<name>] template = "…"`, a URL template a string `src` goes through, which must carry `{src}` and may carry `{width}`. `pub struct ImageSource { pub template: String }`.
* Each rule above is refused at load with the key named.

### FontsSection

`[fonts]`: `base` is where the font files are served from when a CDN answers them and one `[fonts.<key>]` per family the application declares. A family the directory holds and no key names is inferred from the files' own name tables under the key its family slugs to.

* `pub struct FontsSection { pub base: Option<String>, pub faces: BTreeMap<String, FontEntry> }`, `#[non_exhaustive]`.
* A key is lowercase letters, digits and `-`, since it names `--font-<key>`; `base` is `scheme://host/path` with no trailing slash.

### FontEntry

One `[fonts.<key>]`. Every field is optional, though a key carries at least one, since an empty table is not read.

* `pub struct FontEntry { pub family: Option<String>, pub files: Vec<String>, pub fallback: Option<String>, pub display: Option<String>, pub preload: Option<bool>, pub remote: Option<String>, pub variable: Option<String>, pub scripts: Vec<String> }`, `#[non_exhaustive]`.
* `family` names the family when the files' tables are not to be trusted or `remote` carries no file to read; `files` names the files under `[dirs] fonts` the key covers, else every file whose family matches.
* `fallback` is the system face the fallback is sized against, one `snapfire_fsr_assets::font::fallbacks` names, `Arial` absent; `display` is `font-display`, `swap` absent; `preload` is whether the faces are preloaded, the regular weight only when absent.
* `remote` is a provider's own stylesheet URL, which the host links and preconnects to; it needs `family` and excludes `files`. `variable` is the CSS variable the family is defined as, which must start with `--`; `--font-<key>` absent. `scripts` names the Unicode scripts a generated card image draws with these faces before any other; a name Unicode does not have refuses the boot of a host with card images.

### parse_duration

* `pub fn config::parse_duration(raw: &str) -> Option<Duration>`: `<n>`, `<n>s`, `<n>m`, `<n>h`, `<n>d`.

### socket

The `ws` feature's module, `snapfire_fsr_host::socket`.

* `Row { key: String, value: Value }` and `Row::new(key, value)`: one store row, the key an island reads and the value it takes.
* `On`: `Joined`, `Said(Row)`, `Left`, what happened on a topic.
* `Who { topic, session, identity, connection }`: who it happened to. `connection` is unique per socket, which is what tells two windows of one session apart and what presence should be keyed by.
* `Reply { everyone, others, sender }`, with `Reply::everyone(rows)`, `Reply::others(rows)` and `Reply::sender(rows)`: the rows to send and to whom. `everyone` includes the sender, which is what a transcript wants; `others` is what a typing indicator wants. `Reply::default()` sends nothing, which is what a key the application does not know deserves.
* `Sockets`, from `Host::sockets()` or given to `HostBuilder::sockets`: `on(topic) -> usize`, how many sockets a topic holds, `connections(topic) -> Vec<u64>`, which ones, `push(topic, rows)`, rows to all of them from outside any connection and `push_to(topic, connection, rows)`, rows to one, which is what an application building a view per recipient needs. `push_to_each(topic, connections, rows)` sends the same rows to several connections, for a view everyone on a topic shares. A send encodes its rows once, however many connections take them.
* `SocketHandler`, the boxed `Fn(&Who, On) -> Reply`.

## 2. Building

### Artifact

* `pub struct Artifact { pub config: Config, pub plan: String, pub contract: Option<Contract> }`: a built application as the host reads it. `Clone` and `Debug`. `#[non_exhaustive]`: build one with `of` or `new`.
* `Artifact::new(config: Config, plan: String, contract: Option<Contract>) -> Self`: the three fields as given; nothing is read.
* `Artifact::of(config: Config) -> Result<Artifact, HostError>`: reads the plan file and the contracts directory the configuration names, the latter merged with `Contract::merge` file by file when it exists; `Io` for a plan that does not read.

### Host::from

* `Host::from(loader: impl Into<Loader>) -> Result<HostBuilder, HostError>`: `loader.load()`, then `from_artifact`, keeping the loader on the builder so `Host::reload` reads the artifact again the same way. A path is `Loader::at(path)`.
* `Host::from_cwd() -> Result<HostBuilder, HostError>` is `Host::from(".")`.
* `Host::from_config(config: Config) -> Result<HostBuilder, HostError>` is `from_artifact(Artifact::of(config)?)`.
* `Host::from_artifact(artifact: Artifact) -> Result<HostBuilder, HostError>`: over an artifact already in memory, which is how `fsr test` renders a route a spec loads. `Config` naming the plan when `server.render` is set. No loader is kept, so the host reloads only through a reloader or `reload_with`.

### HostBuilder

* `services_over(self, transport: Arc<dyn Transport>) -> Self`: every client's calls go to this transport; the contract still comes from the documents.
* `services(self, services: Arc<Services>) -> Self`: a registry built elsewhere, in place of the clients and of any `service`.
* `service<T>(self, service: Arc<T>) -> Self where T: Transport + DeclaredService + 'static`: a `#[service]` block, served in process under `T::NAME` through the same registry, interceptors and data cache a client goes through. `T::contract()` is merged with the contracts directory's through `Contract::adopt`, so a build that read the block writes the same contract and one that disagrees fails `build` with `HostError::Service`. `services_over` replaces the clients' transports and leaves this one in place. The report lists it as a `services` row of kind `rust` with the Rust type's path.
* `session_store(self, store: Arc<dyn SessionStore>) -> Self`.
* `csrf(self, scheme: Arc<dyn CsrfScheme>) -> Self`: the CSRF scheme, in place of the one `session.csrf_scheme` names; `snapfire_fsr_session`'s `SingleUse`, `PerSession` and `Derived` or the application's own. A builder given one is not rebuildable by a sites reload.
* `codec(self, codec: Arc<dyn CookieCodec>) -> Self`: the cookie codec, in place of `HmacCodec` over the configuration's keys; `key` and `previous_keys` then feed only a `derived` scheme. A builder given one is not rebuildable by a sites reload.
* `keyring(self, ring: Arc<Keyring>) -> Self`: the ring the stock codec and a `derived` scheme sign with, held and rotated by the application, in place of one built from `key` and `previous_keys`; a reload leaves it alone. A builder given one is not rebuildable by a sites reload.
* `http2(self, on: bool) -> Self`: negotiates HTTP/2 as well as HTTP/1.1 on a served connection, over `server.http2`.
* `sockets(self, sockets: Arc<socket::Sockets>) -> Self`, the `ws` feature: the registry to serve from, for an application that must hold it before the host exists, such as one whose own transport pushes into it. Without this the host makes its own.
* `socket<F>(self, handler: F) -> Self where F: Fn(&socket::Who, socket::On) -> socket::Reply + Send + Sync + 'static`, the `ws` feature: what the application makes of what a page sends over `/_sf/socket`. Called once when a connection joins a topic, once per row it sends and once when it leaves; whatever it answers goes out to that topic as store rows. Without one the endpoint is 404, since a socket nobody answers does nothing.
* `topics<F>(self, rule: F) -> Self where F: Fn(&str, &SessionCell, Option<&Identity>) -> bool + Send + Sync + 'static`: who may follow which topic on `/_sf/live`, asked once per topic as a stream opens against the session the request's cookie names. One refused topic refuses the stream, 403 naming it, rather than opening it half. Without a rule any topic may be followed by anyone, which is right for a board on a wall and wrong for a room. `TopicRule` is the boxed form.
* `shell(self, evaluator: Arc<dyn Evaluator>) -> Self`.
* `prerendered(self, dir: impl Into<PathBuf>) -> Self`: where prerendered documents are read from, over `server.prerender`.
* `meta(self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self`: describes the segment whose data source is `name` once its data has loaded, the `AppBuilder` method of the same name.
* `identity(self, provider: Arc<dyn IdentityProvider>) -> Self`, the `auth` feature: the provider behind the `/auth/` routes, in place of the one `[auth]` names; the login page is `auth.login` when the section is written, `/login` otherwise.
* `extension<F>(self, name: impl Into<String>, reach: Reach, f: F) -> Self` where `F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static`: the Rust half of a native pair, forwarded to `AppBuilder::extension`; `name` is `module.member` as the `native(..)` declaration under `ext/` spells it. A plan calling a name nothing registers fails `build` with `BindError::UnknownExtension`. `Reach`, `Ambient`, `Fail`, `FailureKind` and `Catalogs` are `snapfire_fsr_core::ext`'s, re-exported at the crate root with the module itself as `ext`, which holds the argument helpers.
* `pub fn traces(self, traces: Option<trace::Traces>) -> Self`: the collector this host serves `/__fsr/traces` from under development and `/__fsr/trace/{token}` from for the paths `trace.expose` lists. What [`trace::install`](#installing) and its siblings return goes here.
* `island_handler<F, Fut>(self, module: impl Into<String>, name: impl Into<String>, f: F) -> Self where F: Fn(RequestCtx, IslandEvent) -> Fut, Fut: Future<Output = Result<Value, ActionError>>`: one handler of an island a template renders. `module` is what the placement names and `name` what its markup binds with `data-sf-on="click:<name>"`; the handler answers with the state the module is rendered from next. The boot report lists each as an `islands` row, `<module> <name>`.
* `route`, `route_override`, `not_found`, `handler`, `handler_override`, `middleware`, `middleware_override`, `source`, `source_override`, `source_impl`, `action`, `action_override`, `evaluator`, `native`: the `snapfire_fsr::AppBuilder` methods with the same signatures. `native(name, Arc<dyn Native>)` registers the application's own Rust under the name a body reaches it with, `ctx.native.<name>.<method>()`.
* `mount(self, mount: Mount) -> Self`: mounts a site, see `Mount`. `config(&self) -> &Config`: the configuration the builder was made from. `loader(&self) -> Option<&Loader>`: how the artifact was read, `None` for a builder made from an `Artifact` in memory.
* `reload_check(self, check: Arc<dyn ReloadCheck>) -> Self`: a check every candidate must pass before a checked reload swaps it in, after the host's own probes; kept by the host across reloads, so a builder a reloader returns need not carry it. See [ReloadCheck](#reloadcheck).
* `reloader<F>(self, f: F) -> Self where F: Fn() -> Result<HostBuilder, HostError> + Send + Sync + 'static`: how `Host::reload` rebuilds the tables, a builder for the application as it now stands on disk with whatever this builder was given added again. Needed only by a host that cannot be rebuilt from its artifact; see `reload`.
* `build(self) -> Result<Host, HostError>`: with the `tera` feature, reads every `.tera` under the app outside `vendor/`, `dist/`, `generated/`, `node_modules/`, `tests/` and `types/` into one `Tera` through `tera::evaluator`, the files `template_files` lists, each named by its path under the app with the markers registered. It registers that for every module `is_template_module` matches unless an evaluator given to `evaluator` already answers `.tera`; after the app is built, every template module a route, intercept or not-found plan names is checked, `Uncovered` when no evaluator answers it and `TemplateMissing` when the stock one does but read no template of that name. Reads `<app>/locales/*.toml` into the app's catalogs through `locale::load_catalogs`, a file that does not read or a message that is not a string, number or boolean being `HostError::Config`; imports the clients, builds the registry with trace and identity interceptors plus one `CredentialInterceptor::bearer(key).only(clients)` per custody key the clients' `bearer` name, mounts the provider (`DevProvider::from_toml` for `file`, `HostError::Config` naming the file when it cannot be read), registers the shell for the document module and `NullEvaluator` for the rest after any evaluators given, applies the contract, builds the app under the binding rule, refuses a bundle that carries a server module (`HostError::Leak`, see below), then the session layer and the static roots. Everything but the session layer is the host's tables, swapped whole by `reload`.

## 3. The Host

### Host

* `pub struct Host { .. }`: the tables a request reads, the app, the head, the static roots, the prerender directory, the locales, the identity flow and the report, behind one swappable pointer, plus the sessions, which outlive a reload. A request takes the tables once at the edge and keeps them for its lifetime.
* `report(&self) -> Arc<HostReport>`: what the host bound, as of the last reload; `listen(&self) -> &str`, the configured address.
* `locales(&self) -> Locales`: the locales the host serves and how it resolves a request's.
* `reload(&self) -> Result<Arc<HostReport>, HostError>`: rebuilds the tables, checks them the way `build` does and swaps them in; a request in flight finishes on the tables it started with. With a `reloader`, the builder it returns is the rebuild. Without one, the artifact is read again through the loader `Host::from` kept and the `sites_mounter` runs over the builder, which rebuilds a host given nothing but a configuration; `Value("reload", ..)` names the reloader when the host was built from an `Artifact` in memory, when its builder was given services, a session store, a CSRF scheme, an evaluator, an identity provider or a prerendered directory, when anything was registered on the app by hand and when a `Mount` was given by hand with no `sites_mounter`, since a reread would drop each of those. `Value("session", ..)` when the new configuration's `[session]` differs from the one the running sessions were built from, the keys aside, since the store outlives the reload and the tables are left alone; a changed `key` or `previous_keys` is applied to the running ring with `Keyring::replace` when the host built the ring itself. Calls `changed` on success.
* `reload_with(&self, builder: HostBuilder) -> Result<Arc<HostReport>, HostError>`: `reload` over a builder the caller made.
* `async fn reload_checked(&self)`, `async fn reload_sites_checked(&self)` and `async fn reload_with_checked(&self, builder: HostBuilder)`, each `-> Result<Arc<HostReport>, HostError>`: the same three with every probe run over the candidate before the swap, in order: each warmable source loaded once, a document rendered for every path the routes that read nothing of the request answer, a pattern with a parameter once per set its `paths` export returns, each mounted site's `[site] probe` list and then every `reload_check`. The host's own probes refuse on any 5xx kind and on `request_timeout`, which is a backend down, slow or crashed. They pass any other 4xx kind, `not_found` or `forbidden` among them, which is a page's own answer; a probe someone named refuses on any failure. The first refusal is `HostError::Probe` naming the source, the site and path or the check's reason. The running tables keep serving. `SIGHUP`, the poll and `POST /__fsr/sites/reload` reload this way; `POST /__fsr/reload`, which is the dev loop's, swaps whatever assembles, as the three unchecked methods do.
* `pub type Reloader = Box<dyn Fn() -> Result<HostBuilder, HostError> + Send + Sync>`.
* `catalogs(&self) -> Option<Arc<Catalogs>>`: the message catalogs loaded from `locales/`, `None` when the directory holds none. A document embeds the request locale's merged table as `<script type="application/json" data-sf-i18n="<tag>">`; a payload carries it as a `D` row unless the request's `x-sf-catalog` header names that locale.
* `render(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`; `path` may carry a locale prefix, stripped before the route matches and resolved into `ctx.locale` and a query string, decoded into `ctx.query`. Services are bound with the session's identity and no credentials and no CSRF token is minted; `handle` is where custody and the token ride. A prefixed request for the default locale carries `canonical` in its head.
* `render_to_string(&self, path, mode, session) -> Result<String, HostError>`.
* `render_with_status(&self, path, mode, session, locale: Option<&str>) -> Result<(StatusCode, String), HostError>`: the same text with the status `handle` answers, under `locale` as the request's `Accept-Language`, which a path prefix outranks, the page loader's failure kind mapped through `FailureKind::http_status` when it failed, `200` otherwise. `fsr test` answers a spec's `fetch` through it.
* `intercept_for(&self, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)>`: the intercept a soft navigation to `path` renders, of the route's variants in file order: with `into`, the one whose slot it names; otherwise the first whose layouts, module for module from the shell down to the one declaring its slot, the route of `from`, the origin's path, shares. Both paths are without their query.
* `render_navigation(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell) -> Result<BoxStream<'static, String>, HostError>`: the payload for a soft navigation: the intercept when `intercept_for` finds one, else `render` in `Payload` mode. `path` may carry its query; `from` may too. Under an intercept the layouts the origin's route shares with the intercept's, from the root down to the one declaring the slot, load and are keyed under the origin's path, params and query through `assemble_under`, while their `$path` is the target's; every node's `ctx.document` is the origin's path and `ctx.address` the target's request.
* An intercepted render carries `from`, locale prefix stripped, as `RequestCtx::document`, so a link inside the variant marked `current="document"` is judged by the page the intercept opens over; every other render leaves it `None`.
* `render_navigation_to_string(&self, path, from, into, session) -> Result<String, HostError>`.
* `render_navigation_with_status(&self, path, from, into, session, locale: Option<&str>) -> Result<(StatusCode, String), HostError>`, `locale` as above.
* `render_not_found(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<Option<BoxStream<'static, String>>, HostError>`: the application's not-found tree for `path`, with `params.path` set to the path without its query string or `None` when the application has none.
* `auth_call(&self, flow: &AuthFlow, method: &str, path: &str, query: &str, body: &[u8], headers: &[(String, String)], session: SessionCell) -> Option<(u16, Vec<(String, String)>, String)>`: the three `/auth/` routes against a session cell rather than a cookie, for a caller that holds the session itself, as a status, the response headers and the body. `None` when no provider is mounted or the path is not one of the three. The identity the flow settles lands in `session`, the cell the caller renders pages with; the `Set-Cookie` comes back as a header and is the caller's to ignore. `fsr test` is the caller and a journey starts at `GET /auth/login`, which is what puts a flow in progress; a callback with none is 400.
* `pub struct AuthFlow`, with `new()` and `Default`: what a cookie carries between the calls of one journey, the session id the flow is keyed by and the custody its state lives in. One per session, since a second `AuthFlow` is a second browser.
* `call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`: `call_action_in` under the default locale.
* `call_action_in(&self, id: &str, session: SessionCell, locale: Locale, input: Value) -> Result<Value, ActionError>`: runs the action with `locale` as its `ctx.locale`.
* `prerenderable(&self) -> Vec<String>`: the patterns one render serves for every request: every source lowered and reading nothing of the request (`snapfire_fsr_ir::body_reads_request`), no Rust source, no page or layout on the plan reading its `identity` or `csrf_token` prop (`Component::reads_prop`) and no parameter, unless the page loader's `paths` names the sets to render (`App::paths`).
* `prerenderable_anonymous(&self) -> Vec<String>`: the patterns one anonymous render serves for every anonymous request: no parameter unless `paths` names its sets, every source lowered and reading nothing of the request but `identity` and calls through a client whose `bearer` is set, no page or layout on the plan reading `csrf_token`. `handle` serves their files only to a request whose session carries no identity and renders a signed-in visitor live; `Display` lists them under `prerender` with `for anonymous visitors`.
* `refresh_sitemap(&self) -> Result<(), HostError>`: builds the sitemap again from what the host serves now and swaps it in; requests meanwhile get the one held and a failed build keeps it, returning `HostError::Sitemap`. The host never calls it; an application that wants a fresher sitemap than its last reload calls it from its own job. `Ok(())` without `[crawl] sitemap`.
* `prerender(&self, out: &Path) -> Result<Vec<(String, PathBuf)>, HostError>`: warms every source `App::warmable` names, once per supported locale with nothing of a request behind it, writing them to `<out>/LOADS_FILE`; then renders each prerenderable pattern, the anonymous class included, anonymously once per supported locale and writes `<out>/<path>/index.html` and `index.payload`, `/` at the top of `out`, a locale other than the default under its tag, `<out>/fr_FR/<path>/`. A pattern in `App::paths` is rendered once per set its body returns for the locale, at the path the set fills the pattern to; a set missing a parameter or giving one an empty value is `HostError::Paths`. It may run while the host serves. Each file is written beside its place and renamed in, a page whose loader fails keeps the file the last run wrote. Once every page is written the files the last run listed in `<out>/WRITTEN_FILE` that this run did not write are removed along with the directories that emptied, then the run writes its own list there and `<out>/BUILD_FILE`, the build it rendered from: a hash of the plan and every mounted site's, the templates and each assets manifest. A host whose own build differs from the one a directory's `BUILD_FILE` names reads none of its documents, loads or renders at boot, `HostReport::prerender_stale` set. So does one that finds a list with no `BUILD_FILE`; a `prerender` from this host into that directory puts it back in use. Under `dev` the edge never answers from the directory and its loads and renders files are not read at boot. A `GET` whose query carries `__prerendered` is rendered as `prerender` would render it, anonymously with the plain head. It is answered with `x-sf-prerendered: preview`, or `404` naming why: the route reads the request, the path is outside its `paths` or its loader failed. After each page's `index.html` its card images are drawn and written beside it, `<stem>.png` or `.jpg` or `<stem>/<id>` for a listed image, into the written list; one that fails is logged and its file from the last run kept. When the host answers `client::ROUTE` from the binary and a document was written, the client modules are written under `<out>/static/js/fsr/` too, in the build the host serves, so a directory served by anything else loads what its documents' import map names. Returns what it wrote, each path with its prefix, a client module under its URL, the loads file and the written list under their own names. The warm pass runs first and replaces the memo the host booted with, so a rerun writes documents from the loads it just took. A source whose load fails is left out and logged rather than written as a failure. So is a path whose page loader fails, since a failure document written to disk would answer `200` for ever. After the documents, the render pass: every subtree in `App::renderable` is assembled on its own, once per locale and per `paths` set, with the build's renders cleared and `WarmRenders::record` on, so each render lands in the memo under its request-time key; the entries are written to `<out>/RENDERS_FILE` and stay in the memo of the host that ran the pass. A subtree whose render fails is logged and left out.
* `changed(&self)`: tells every open `/__fsr/events` stream that something changed; nothing when `dev` is off.
* `publish(&self, topic: impl Into<String>)`: tells every open `/_sf/live` stream watching `topic` that it changed. A publish nobody is watching is a send into an empty channel. What a listener does with it is the client's: `live()` revalidates the route it is showing, so the loaders run again and the page follows.
* `invalidate(&self, plan_key: &str) -> usize`: drops every cached subtree under the plan `cache_key`, a lowered page's or layout's module name and says how many went; zero without a `[cache]` section.
* `invalidate_tags<I, S>(&self, tags: I)`: drops every data cache answer under the named tags; nothing without `[cache.data]`.
* `services(&self) -> Arc<Services>`: the registry the routes call through.
* `LOADS_FILE: &str`: `loads.json`, what a warm pass writes into the prerender directory and a boot reads back: every memoizable source's data under the key a request composes for it, values in the payload crate's JSON encoding. An absent or unreadable file costs loads rather than a boot, since a warm pass is an optimization.
* `WRITTEN_FILE: &str`: `prerendered.json`, the files a `prerender` run wrote as paths relative to the directory, a JSON list. The next run removes exactly those before writing; an absent file removes nothing.
* `TEMPLATE_EXTENSIONS: &[&str]`: `["tera"]`, the extensions of a route or layout file rendered by a template evaluator rather than the lowered tree. `is_template_module(module: &ModuleId) -> bool` says whether a module's path ends in one.
* `template_files(app: &Path) -> Result<Vec<PathBuf>, HostError>`: every template the host reads at boot, sorted: each file under `app` ending in one of `TEMPLATE_EXTENSIONS`, outside a dot directory and outside `vendor/`, `dist/`, `generated/`, `node_modules/`, `tests/` and `types/` directly under `app`. Compiled without the `tera` feature too; `HostError::Io` when a directory does not read. `fsr bundle` places the same files in the deploy tree.
* `RENDERS_FILE: &str`: `renders.json`, what a render pass writes beside the loads file and a boot reads into a `WarmRenders` in front of whatever `[cache]` configured or `NoCache`: every subtree `App::renderable` names, rendered once per locale, under the memo key a request composes for it, as `{ node, segments, digest }` with the node in the payload's row form. An absent or unreadable entry costs a render rather than a boot.
* `prerendered(&self, path: &str, mode: RenderMode) -> Option<String>`: the text held under the prerender directory for the path, its locale prefix choosing the locale's directory and its query string ignored; `None` without a directory or a file.
* `call_handler(&self, method: &str, path: &str, session: SessionCell, input: Value) -> Result<Value, ActionError>`: the handler matching the method and the path, its locale prefix stripped and resolved into `ctx.locale` and its query string becoming `ctx.query`, run with `input` as the request body; `NotFound` when none matches. What the handler queued with `after` starts when it returns, as `call_action` and `call_action_in` start theirs.
* `fragment_of(raw_query: &str) -> Option<Option<String>>`: the fragment a query asks for, read as `application/x-www-form-urlencoded`: `Some(None)` for the bare key or an empty value (`__fragment`, `__fragment=`) and `Some(Some(slot))` for a named one, the key and the slot percent-decoded and `+` a space; the first `__fragment` pair decides.
* `preview_of(raw_query: &str) -> bool`: whether a query asks a development host to preview a page as `prerender` would write it, any pair whose decoded key is `__prerendered`.
* `payload_of(raw_query: &str) -> bool`: whether a query asks for the navigator's payload, any pair whose decoded key is `__payload`, whatever its value.
* `with_fragment(location: &str, slot: Option<&str>) -> String`: `location` with that key asked for again, which is how a form posted from a fragment is answered with one.
* `referer_path(referer: &str) -> Option<String>`: the path of a `Referer` on this origin, where a form post lands again; `None` for another origin. The three are public so a test runner answers a form post the way the edge does.
* `percent_decoded(segment: &str) -> String`: a path segment with each `%` followed by two hex digits decoded, `+` and any other `%` kept as they are; a segment that decodes to invalid UTF-8 is returned as given. The edge decodes an action id and an island module with it.
* `handle(&self, req: Request<Bytes>) -> Response<Body>`: static roots first, by prefix; then the locale, `Locales::resolve` over the path, the `Cookie` header and `Accept-Language` or for the action route over the `x-sf-from` header's path instead of the request's, the stripped path standing in for the rest and a prefixed `/_sf/`, `/__fsr/` or, with a provider mounted, `/auth/` path answered `404`; then, with a provider mounted, the identity routes, each answering the `Step` `Auth` returns: `GET /auth/login` calls `Auth::begin` with `Start::sign_in`, its `return_to` the query's when that is a path on this origin, else the `Referer`'s path, else `/`; its `method` is the query's; `POST /auth/enrol` verifies `_csrf` or `x-sf-csrf` (403 when it fails), answers 401 to an anonymous session and calls `Auth::begin` with `Start::enrol` for the session's identity; `/auth/callback` reads its params from a form-encoded or JSON `POST` body or from a `GET` query and calls `Auth::advance`. A request whose `Accept` names `application/json` gets `{ "next": "redirect", "to" }`, `{ "next": "present", "kind", "data" }` or `{ "next": "complete", "to" }` with 200. A refusal gets `{ "error": "denied" | "invalid", "message" }` with the error's status. Any other request gets 303 to the redirect, 303 to `<login>?return_to=<pending>` for a step the page presents, 303 to the destination on completion, 303 to `<login>?error=denied&return_to=<pending>` on `AuthError::Denied` and 400 with the message on `Invalid`. A completed sign-in renews the session id and rotates the CSRF scheme; a completed enrolment keeps both; `POST /auth/logout` verifies `_csrf` from the body or `x-sf-csrf` from the headers with `Sessions::verify_csrf`, 403 when it fails, else `Auth::logout`, `Sessions::destroy` and 303 `/` carrying the expiring cookie and no persist; a `GET` of the login page calls `Auth::ensure_flow` with the query's `return_to`, else the `Referer`'s path when that is not the login page, else `/`, then continues; then the middleware chain, run by `snapfire_fsr_runtime::drive_middleware`: the shell's middleware with `site` in its input naming the mounted site the path belongs to or null, whose `next` runs the middleware of the site the request it names belongs to, whose `next` runs the route. Each answers an `HttpResponse` value, its own, the route's with the status and headers it left or nothing to go on; the route's body streams under the head the chain answered. A site's middleware handing `next` a path outside its prefix is `Internal`; then `POST /_sf/action/<id>`, the id's `%XX` escapes decoded since a site's carries a colon: `403` when the request's `Origin` names neither its `Host` nor `[document] origin` (the rule a route handler's write and a step take too), a request with no `Origin` taken as not a browser; `415` for a body that is neither `application/json` nor a form; with a JSON body, `400` on a body that does not parse, the value as JSON with the action's `redirect` target in `x-sf-redirect`, the failure kind's status on error; with a form-encoded body, `_csrf` taken out and verified with `Sessions::verify_csrf`, `403` when it fails, the remaining fields the input read against the action's declared input type, since a form body carries text and nothing else, a success answered `303` to the path the action passed to `redirect` or else to the `Referer`'s path on this origin (else `/`); a failure answered `303` to that page with the failure on the session; a form caller naming `application/json` in `Accept` answered as a JSON body is; then a handler matching the method and path, its JSON body as the input or `null` when empty, a form-encoded one having its `_csrf` verified and its fields read against the handler's declared input type, answered with the value as JSON or, when the value is an `HttpResponse`, with that response's status, headers and body, `400` on a body that does not parse and the failure kind's status on error; then, for a `GET` the prerender directory holds, that text with `x-sf-prerendered: 1`, unless the route is prerendered for anonymous visitors and the session carries an identity; then a page, `__payload` in the query selecting the payload mode, in which an `enc` outside `PAYLOAD_ENCODINGS` is `406` and in that mode an `x-sf-from` or `x-sf-into` header makes it `render_navigation`, which skips the prerender directory when an intercept applies, answered with the status of the route's own page loader's failure kind when that loader failed, `404` for `not_found`, the document still rendered around the error segment and a payload or a fragment carrying the same status, while a layout or a slot failing degrades its segment at `200`; for no route, the not-found tree with status `404` when the application has one, else `404` with a line of text. The session is opened from `Cookie` and a `Set-Cookie` is appended when it changed; so is the locale cookie a `Resolution` asks for. Every body under `handle` runs with the session's token custody bound to its services and, once the session is identified, the session's CSRF token as `ctx.csrf`; an anonymous request carries no token, so its renders share the memo.
* `GET /_sf/socket?topic=a` upgrades to a WebSocket, the `ws` feature: `HostBuilder::topics` decides whether this visitor may open it at all, `HostBuilder::socket` answers what it sends and each frame is one row, `{"key": ..., "value": ...}` in, `{"rows": [...]}` out. No handler is 404, no topic is 400, a topic the rule refuses is 403 and a request that is not an upgrade is 400. Framework-owned and never locale-prefixed. Serving a connection `with_upgrades` is what `serve_listener` does, so an upgrade works on the hyper listener without any arrangement.
* `GET /_sf/live?topics=a,b` answers with a `text/event-stream`, whatever `dev` says, after `HostBuilder::topics` has allowed every topic asked for: a `: open` comment frame at once, then one `data: {"topic":"a"}` per `publish` of a topic in the list, until the client goes away. No topics is 400. The path is framework-owned and never locale-prefixed.
* `GET /__fsr/sites` answers, whatever `dev` says, with `{"sites": [{"name", "at", "version", "hash"}, ..]}` for every mounted site, before statics, middleware and sessions.
* A `GET` or `HEAD` an `App::images` row matches is answered with the drawn card image after the session opens: a prerendered file for the path outside `dev` when the directory is current, else the bytes kept for the path and props, else a draw. A data source's not found and an unlisted id answer 404, a draw failure 500 with the reason.
* A card element's `class` is drawn from the table the build writes at `CARD_CLASSES_FILE`, `pub const CARD_CLASSES_FILE: &str = "generated/tailwind.cards.json"` under the app: each class's declarations, in the table's order and filtered by the card's width, are written ahead of the element's own `style`. A mounted site's images read the site's table. A table that is missing reads as empty; one that does not parse is logged on target `fsr::card` and reads as empty.
* The document's theme, for a page or payload request when the plan or a mounted site's carries `theme-of` rows or a `theme` body: after the middleware, the route's static theme is read down its plan's `content` chain, the deepest row winning unless a row above it forces its own and the outer of two forced rows winning. The shell's `theme` body then runs with `{ name, forced }` as its input, then the mounted site's with the shell's answer as `name`; each answers `{ name, scheme }`, strings or null. A body that fails or answers another shape is logged on target `fsr::theme` and leaves the choice where it was. The answer is written onto the opening `<html>` of every document, live and prerendered, as `data-sf-theme` and `data-sf-scheme`, plus an empty `data-sf-theme-forced` when the route forces its theme and the answer kept it, an attribute already there replaced and one the answer leaves out removed; a payload carries it in its eager `H` row, a prerendered payload's row rewritten to match. A fragment request is left as it is.
* On a mounted site's routes the head gains the site's stylesheets and `<script type="module">` for its entry and the payload an `E` row naming the entry, so the navigator loads the site's islands on first arrival.
* The head gains, as owned `<link rel="stylesheet" data-sf-css>` and in the payload's `C` row, each sheet the plan's `module-sheets` rows name under the request's layouts, page, fallbacks and error boundaries that the bundle holds, by its served URL. Those sheets leave the bundle's component stylesheets linked on every document; a component sheet no row names stays there.
* With `dev` on, `handle` also answers `GET /__fsr/traces` with the last 50 traces as [`trace::to_value`](#reading) writes them, newest last or `[]` when no collector was given to `traces`. Development only.
* For a request whose path `trace.expose` covers (any path with `dev` on), when a collector was given to `traces`: the response carries `x-sf-request: <id>.<hmac>`, the trace id and an HMAC-SHA256 of `fsr.trace:<id>` under the session keyring's current key. A document's head carries `<meta name="sf-request" content="<token>">`. `GET /__fsr/trace/{token}` answers that one trace as [`trace::one_value`](#reading) writes it, with `Cache-Control: no-store`, once the request has finished and while the collector's ring holds it; a token that verifies under no key in the ring, an id the ring no longer holds and a trace still in flight are 404 `no such trace`. With `expose` empty and `dev` off, no response carries a token and the path is not a route.
* For an HTML page whose path `trace.inline` covers, when a collector was given to `traces`: the body ends with `<script type="application/json" id="sf-trace">`, written before the last `</body>` once the rest of the body has been sent. It holds the request's trace as [`trace::one_value`](#reading) writes it, with `<` escaped, every span the request ran and `ms` measured up to that point. The response is `Cache-Control: private, no-cache` and carries no `Content-Length`. A payload or fragment request gets no script.
* With `dev` on, `handle` answers `GET /__fsr/events` with a `text/event-stream` body, one `data: {"bundle":"<id>","boot":"<id>","generation":<n>,"styles":[...]}` event on open and one per `changed`, `styles` the document's stylesheets as the tables serving now link them, `POST /__fsr/changed` with 204 after calling `changed` and `POST /__fsr/reload` with 200 and the new report as text after `reload` or 409 with the error, each of the three `403` for a request carrying `Origin`, which `fsr dev` never sends, all before statics, middleware and sessions; static files gain `Cache-Control: no-cache`. The bundle id is a hash over every output `dist/.snapfire-build.json` lists, source maps aside, `-` without a bundle; a served document's head carries `dev_script` with the id of that moment and `prerender` writes the plain head.
* `service(self: &Arc<Self>) -> HostService`.
* `owner_of_source(&self, name: &str) -> Option<Owner>`.

### Server-mode islands

* `POST /_sf/island/<module>`, the module id percent-encoded, with a JSON body `{ props, state, handler, event }`: one round trip of an island in server mode. A module the plan lowered is stepped by the interpreter; a module `island_handler` registered is stepped by its own handlers; the two are told apart by the plan rather than by the request. For a lowered component `state` names the component's state, with a component rendered inside it under `<address>/<name>`; `handler` is the index of the handler that fired or `<address>/<index>` for a handler of a component inside it, the token its `data-sf-on` carries; for a template island `state` is whatever its handlers agreed on and `handler` is a name, `400` either way when it is the other kind. `handler` is null to render as is; `event` is what the browser saw. A step is held to an action's origin rule: `403` from another origin. A body that is not `application/json` is `415`. A failure of kind `internal`, from either path, is logged on target `fsr::island` with the module and its message; outside development its answer is `{ kind: "internal", message: "the island step failed" }`.

  A template island's step runs the named handler with the props, the state and the event, all three from the browser, then renders the module through its own evaluator with `snapfire_fsr_runtime::island_data`: the props less `$s` and `$k`, plus the answered state under `state`. It answers `{ state, html, revalidate }`, `revalidate` true when the handler wrote the session, `404` for a name the module does not have and the handler's own failure kind otherwise. A slot emitted by such a template is `500`, since an island has no child to fill it. Every action the handler called is dispatched in order before the answer, through the same `dispatch` an action route uses, with the session, the locale and the host of this request; the first failure is answered as the action route would answer it and nothing after it runs. Answers `200` with `{ state, html, revalidate }`, the state after the handler, the island's markup rendered from it and whether an action ran, `400` with `{ kind: "invalid", message }` for a body that is not the shape or a state key the component lacks, `404` for a module that is not lowered, a handler index it lacks or an address its render does not reach and the interpreter's own status for a failure. The document's locale is the island's, from `x-sf-from` the way an action's is. The session cookie is set as on every response.
* `pub fn island_body(body: &[u8], locale: &str) -> Result<IslandBody, (StatusCode, serde_json::Value)>`: a step's body before either path has judged it, `IslandBody { props: ValueMap, state: Value, handler: Value, event: Value }`, with `locale` joining the props as it does for a page. Both paths read it.
* `pub fn island_step(lowered: Option<&IrEvaluator>, module: &str, body: &[u8], locale: &str) -> Result<IslandStep, (StatusCode, serde_json::Value)>`: the route's work short of dispatching, which `fsr test` calls too. `IslandStep { state: ValueMap, store: ValueMap, html: String, acts: Vec<(String, Value)> }` is what the step produced, `store` the store keys its handler wrote and `acts` the actions it called with their inputs, for the caller to dispatch; `IslandStep::json(&self)` is the `{ state, store, html, revalidate }` answer, `revalidate` true when `acts` is not empty. The step reads the store from the `$store` prop the browser sends. The error is the status and body to answer with; a slot in the rendered markup is a `500`, since a step has no child to fill it and an empty answer would take the slot's content out of the document.
* `Host::lowered(&self) -> Option<Arc<IrEvaluator>>`: the lowered components and their interpreter, `None` when the plan lowered no component.

### Probe

* `pub enum Probe { Path(String), As { path: String, subject: String } }`: one entry of a site's `[site] probe` list, `"/billing"` or `{ path = "/billing/overdue", as = "probe" }`, the second rendered by a visitor signed in as that subject with no claims. `path(&self) -> &str`, `subject(&self) -> Option<&str>`. `Deserialize`, untagged.

### ReloadCheck

* `pub trait ReloadCheck: Send + Sync { fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>>; }`: what `HostBuilder::reload_check` takes; `Err` refuses the candidate with the reason. Implemented for any `Fn(&Candidate) -> BoxFuture<Result<(), String>>` that is `Send + Sync`.
* `pub struct Candidate<'a>`: the tables a checked reload would swap in, rendered through the running host's sessions, evaluators and services but never served. `report(&self) -> &HostReport`; `async fn render(&self, path: &str, session: SessionCell) -> Result<(StatusCode, String), HostError>`, the path with its query as a document in the default locale, the status `handle` would answer and the document, `NotFound` for a path no route matches; `async fn render_as(&self, path: &str, subject: &str)`, the same signed in as `subject`. Middleware does not run under a candidate render, so a guarded page is probed as the page itself.

### Mount

* `pub struct Mount { pub name: String, pub version: String, pub hash: String, pub allow_engine: bool, pub artifact: Artifact }`: a site's artifact as the host mounts it, read from its directory by `Loader::mount` on the shell's loader or a loader of the caller's own. Where it came from is the caller's business; `artifact.config.root`, `version` and `hash` reach the report.
* `Mount::new(name, version, hash, allow_engine: bool, artifact: Artifact) -> Mount`.
* At `build`, a mount is refused (`HostError::Mount`) when its configuration has no `[site]` or names another site, when its prefix is already served, when it carries engine-owned rows without `allow_engine` or when its bundle carries a server module. Otherwise its routes and intercepts are nested under the shell's `routes/layout.tsx#default` or under the document when the shell has none, with ids renumbered; its sources, actions, components and handlers join the shell's tables; its contract is merged; its clients register under `<name>:<client>` with their bearer keys; its static roots under its prefix are served and the rest listed as ignored, with `session` and `auth`, `locales` and `cache` when set; its import map entries the shell lacks are added; its middleware and its `theme` body are held apart, see `handle`; its `theme-of` rows join the shell's; its stylesheets and entry module join the head on its routes.

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

* `pub struct locale::Resolution { pub locale: Locale, pub path: String, pub prefixed: bool, pub set_cookie: Option<String> }`: the locale, the path without its prefix, whether it had one and the `Set-Cookie` value to append when `remember` is on and the prefix chose a locale the cookie does not hold, `sf_locale=<tag>; Path=/; Max-Age=31536000; SameSite=Lax`.

### RenderMode

* `Html`, `Payload`, `Fragment(Option<String>)`. `Clone`, `PartialEq`, `Eq`; not `Copy`.
* `Fragment(None)` renders the route's page segment alone; `Fragment(Some(slot))` the parallel slot of that name, wherever it sits on the route. Either is bare markup: no shell, no segment delimiters, no sidecar, every deferred segment resolved before anything is written, followed by the route's store seed as `<script type="application/json" data-sf-store>` when it has one. `handle` chooses it from `__fragment` in the query, bare for the page or `__fragment=<slot>` and answers `text/html`; `parse_query` drops the key, so no loader and no segment key sees it.

### HostReport

* `pub struct HostReport { pub app: snapfire_fsr::Report, pub services: Vec<(String, String, String)>, pub statics: Vec<(String, PathBuf)>, pub unanswered_routes: Vec<String>, pub cache: Option<(u64, String)>, pub locales: Vec<String>, pub auth: Option<(String, String)>, pub bearer: Vec<(String, String)>, pub extensions: Vec<String>, pub site: Option<(String, String)>, pub sites: Vec<SiteReport>, pub config: Vec<PathBuf>, pub inferred: Vec<String>, pub public: Vec<(String, String)>, pub ignored: Vec<String> }`
* `extensions: Vec<String>`: the native pairs registered beside the standard library, by name; `Display` prints them as `natives` rows labelled `rust`, after `bearer`.
* `catalogs: Vec<(String, usize)>`: each locale with a file under `locales/` and how many keys it holds; `Display` prints one `catalogs` row, `en_US 5 keys, fr_FR 5 keys`.
* `client: Option<(&'static str, usize, usize, bool)>`: the prefix the embedded client answers, how many modules it holds, what they come to and whether the build served is the minified one; `None` when a static root claims the prefix. `Display` prints one `client` row after the `static` rows, naming the build as `minified` or `readable`.
* `import_map_csp: Option<String>`: the `script-src` source covering the document's inline import map, `'sha256-<base64>'`, which is the only executable inline script a page carries; `None` without an import map. The host merges the shell's map with each mounted site's, so it matches no single file on disk and a deployment writing a Content-Security-Policy reads it from a boot rather than hashing a file. `Display` prints it as a `csp` row after the `client` row.
* `csp: Option<String>`: `document.csp` with that source already substituted, the header an HTML response carries; `None` when none is set. `Display` prints it on the line under the `csp` row, which says so outright when it is unset.
* `crawl: Option<CrawlReport>`: `[crawl]` when configured, see [CrawlReport](#crawlreport).
* `site: Option<(String, String)>`: the application's own `[site]`, name and prefix; `Display` prints one `site` row. `sites: Vec<SiteReport>`: the mounted sites; `Display` prints a `sites` row per mount, `billing at /billing from <artifact> <version> <hash>` and a row naming what the mount ignored.
* `HostReport::brief(&self) -> Brief<'_>` prints only what needs attention: `unanswered_routes` as `routes` rows, each site's ignored rows, each prerenderable pattern when the prerender directory was written by another build, `crawl.unlisted` and `ignored`. `fsr serve` prints it unless `--verbose`.
* `Display` prints the app's report, then `services` rows as `<http, grpc, mock or rust> <base url, responses file or Rust type>`, `static` rows, `config` sources and `inferred` lines.
* `prerender: Option<PathBuf>`: the prerender directory when one is configured; `Display` lists each prerenderable pattern with it (`not configured` when there is none), `for anonymous visitors` on the anonymous class and `per paths` on a pattern `Report::paths` names. `prerender_stale: bool` says that directory was written by another build; `Display` then says so on each row.
* `rendered: usize`: how many memo entries the prerender directory's renders file answered at boot; `Display` lists each of `app.renderable`'s subtrees under `render`, the pattern and the module, the first row carrying that count or `not rendered`.
* `warmed: usize`: how many keys the prerender directory's loads file answered at boot; `Display` lists each of `app.warmable`'s sources under `warm`, the first row carrying that count or `not warmed` when it is zero.
* `cache: Option<(u64, String)>`: the capacity and lifetime as written; `Display` prints one `cache` row when set.
* `dev: bool`: `Display` prints one `dev` row naming the two paths when true.
* `http2: bool`: `Display` prints one `http2` row when true, naming the ALPN list under TLS and saying it is h2c without.
* `tls: Option<TlsReport>`: `cert`, `key`, `alpn` and `reload`, the signal or `None`. `Display` prints the two files and what re-reads them.
* `locales: Vec<String>`: the configured locales, the default first, empty without a `[locales]` section; `Display` prints one `locales` row, `en_US (default, unprefixed), fr_FR`, when set.
* `auth: Option<(String, String)>`: the provider name (`file`, `service via <client>`, else `custom` for one the builder was handed) and the login page; `Display` prints one `auth` row naming both and the four routes, plus `bearer    none; no client carries a token` when `bearer` is empty.
* `bearer: Vec<(String, String)>`: client and custody key for every client whose `bearer` names one; `Display` prints a `bearer` row per client.
* `session: Option<String>`: the client the sessions live behind when `session.store` is `service`; `Display` prints one `session` row, `service via <client>`.
* `cached: Vec<(String, String)>`: `service.method` and its policy as text, `ttl 15s shared, stale 2m [tags]`; `Display` prints a `cached` row per method. `writers: Vec<(String, String)>`: `service.method` and `[tags]`; a `writes` row per method. The `auth` name for a `service` provider reads `service via <client>`.
* `shared: Vec<(String, Vec<String>, Vec<String>)>`: every `@` tag, the one spelling a site's prefix leaves alone, with the methods cached under it and the methods that drop it across the shell and every site; `Display` prints a `shared` row per tag as `cached by [a:cat.list], dropped by [b:ledger.pay]`.

### SiteReport

* `pub struct SiteReport { pub name: String, pub at: String, pub artifact: PathBuf, pub version: String, pub hash: String, pub ignored: Vec<String> }`: one mounted site; `ignored` lists the site's configuration the shell did not take, `static <route>` for a root outside the prefix or one the shell serves and `session`, `auth`, `locales`, `cache` when the site set them.

### CrawlReport

* `pub struct CrawlReport { pub robots: Option<String>, pub sitemap: Option<String>, pub unlisted: Vec<String> }`: where `[crawl]` answers (the sitemap path already under a site's prefix) and the patterns with a parameter and no `paths` export, which the sitemap cannot list. `Display` prints a `crawl` row per file and one row per unlisted pattern.

### ServiceSessionStore

* `pub struct ServiceSessionStore`, a `SessionStore` over a client: `new(services: Arc<Services>, client: impl Into<String>, ttl: Duration) -> Self`, `ttl` being the end given to a record the service stored before records carried one. `load` calls `getSession { id }` and reads `record` from the answer, `None` on a `not_found` failure and, logged, on any other; `save` calls `putSession { id, record }`; `delete` calls `deleteSession { id }`. The record is `encode_record`'s string.
* `pub fn encode_record(record: &SessionRecord) -> String` and `pub fn decode_record(text: &str, expires_when_missing: u64) -> Option<SessionRecord>`: `SessionRecord::encode` and `SessionRecord::decode`, the record as one JSON string in the payload encoding, `{ data, identity, tokens, csrf, expires }` with `identity` as `{ subject, claims }` or `null` and `expires` an integer, seconds since the Unix epoch; a string without `expires` decodes with `expires_when_missing`.

### ServiceProvider

* `pub struct ServiceProvider`, the `auth` feature, an `IdentityProvider` over a client: `new(services: Arc<Services>, client: impl Into<String>) -> Self`. When the client's contract declares `beginSignIn` and `advanceSignIn`, `begin` and `advance` call them with the same values: `beginSignIn { intent: "sign_in" | "enrol", return_to, method, identity }` and `advanceSignIn { params, state }`, each answering `{ next: "redirect", to, state }`, `{ next: "present", kind, data, state }` or `{ next: "complete", subject, claims, tokens }`. `refresh` calls `refreshSignIn { tokens }` when declared, which answers `{ tokens }` or null. An open map, `identity`, `params`, `state`, `data`, `claims` or `tokens`, is sent as one string in the payload's JSON encoding, since a contract has no open map type; an answer may write one as a map or as such a string. A client declaring only `authenticate` is a password service: `begin` presents the password form and `advance` sends `user` and `password` to `authenticate`, reading `subject`, `claims` and `access_token` from a map answer. A failure of kind `unauthorized`, `not_found` or `invalid` is `AuthError::Denied` with the service's message; any other failure or a malformed answer is `AuthError::Invalid`. The host refuses at boot a client declaring neither `authenticate` nor both step methods.

### Assets

What the build derived from the application's images and fonts reaches a document through `generated/assets.json`, read at boot when present:

* `<meta name="sf:images">` carries the policy, `ImagePolicy` as JSON, which the client's `Picture` reads so a browser writes the markup the server wrote.
* The font CSS is inlined as `<style data-sf-fonts>` and its hash widened into `style-src`; one `<link rel="preload" as="font" crossorigin>` per face the manifest marks; for a provider's stylesheet, `<link rel="preconnect">` per origin and the `<link rel="stylesheet">`.
* `font-src`, `img-src` and `style-src` are widened with the origin of `fonts.base`, `images.base`, each source template and each preconnect.
* A plan component's `head` rows are merged into the document of every page whose plan places that module, the preload of a priority image among them; a row is merged by identity, so two pages placing one image preload it once.
* A static file whose name carries a content hash, `hero.0a1b2c3d.png` or `hero.0a1b2c3d.640.avif`, answers `Cache-Control: public, max-age=31536000, immutable` in place of `static_max_age`; a plain name revalidates unless `static_max_age` sets a lifetime.
* With the `tera` feature, `fsr_picture(src, alt, sizes, priority, class)` writes what the `Picture` tag writes for an image the manifest holds, by its path under the app. `fsr_fonts()` writes the inline style and the font links for a template that builds its own head. An image the manifest lacks fails the render.

### AssetsManifest

`assets::AssetsManifest`, `generated/assets.json`, `assets::ASSETS_FILE`.

* `pub struct AssetsManifest { pub version: u32, pub images: ImagePolicy, pub entries: Vec<ImageEntry>, pub fonts: Fonts, pub media: Vec<MediaFile> }`; `pub struct MediaFile { pub source: String, pub url: String, pub path: String }`, a video, an audio file or a caption track a module imports, placed under `dist/` as it is under its hash; `media` is absent from the file when empty; `read(app: &Path) -> Option<AssetsManifest>`, `policy_json(&self) -> String`, `image(&self, source: &str) -> Option<&ImageEntry>`.
* `pub struct ImagePolicy { pub widths: Vec<u32>, pub formats: Vec<String>, pub quality: BTreeMap<String, u8>, pub base: Option<String>, pub sources: BTreeMap<String, String> }`.
* `pub struct ImageEntry { pub source: String, pub src: String, pub path: String, pub hash: String, pub width: u32, pub height: u32, pub passthrough: bool, pub stripped: bool, pub widths: Vec<u32>, pub quality: BTreeMap<String, u8>, pub variants: Vec<Variant> }`; `pub struct Variant { pub width: u32, pub format: String, pub url: String, pub path: String }`, each `path` under the bundle's output directory, the entry's being the hashed original's. `stripped` says the derive pass writes the original stripped of its metadata; `hash` is then of the stripped bytes.
* `pub struct Fonts { pub base: Option<String>, pub faces: Vec<Face>, pub css: String, pub preload: Vec<String>, pub remote: Vec<Remote>, pub referenced: Vec<FontFile>, pub variables: BTreeMap<String, String> }`; `pub struct FontFile { pub source: String, pub url: String, pub path: String }`, a font a module or a stylesheet names outside the fonts directory, placed under `dist/` like a face and written into no `@font-face`; `pub struct Face { pub key: String, pub family: String, pub weight: u16, pub weight_range: Option<(u16, u16)>, pub style: String, pub source: String, pub url: String, pub path: String, pub unicode_range: Option<String>, pub preload: bool }`; `pub struct Remote { pub key: String, pub family: String, pub href: String, pub preconnect: Vec<String> }`.

### Body

* `pub type Body = UnsyncBoxBody<Bytes, std::io::Error>`.

### PAYLOAD_ENCODINGS

* `pub const PAYLOAD_ENCODINGS: &[&str] = &["json"]`: what a payload request may name in `enc`; the wire's `V` row names the one it got.

## 4. Serving

### HostService

* `pub struct HostService(pub Arc<Host>)`, `Clone`.
* `impl<B: http_body::Body + Send + 'static> tower::Service<Request<B>> for HostService` with `Response = Response<Body>` and `Error = Infallible`; the request body is collected before `handle`.

### hyper

* `Host::serve(self: Arc<Self>, listen: &str) -> std::io::Result<()>` binds and serves until the listener fails: HTTP/1.1 or HTTP/1.1 and h2c when `server.http2` is on.
* `Host::serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()>`: `serve_until` on [`shutdown_signal`], so it returns `Ok(())` once a SIGTERM or SIGINT has drained. `Host::serve_until(self: Arc<Self>, listener, shutdown: impl Future<Output = ()>)` takes any other trigger. `shutdown_signal()` resolves on SIGTERM or SIGINT and on Ctrl-C where there are no signals. Each connection is served by `hyper::server::conn::http1` or by `hyper_util::server::conn::auto` when `http2` is on, which reads the connection preface and answers either version. With `[server.tls]` the connection is a TLS one first, the version chosen by ALPN and a handshake that fails is logged and dropped without touching the listener.
* `Host::after_settled(&self)` resolves once the work every request queued with `after` has finished. `handle` starts a request's work when its response body has ended or been dropped, on the tokio runtime it is polled on; without one the work is dropped with a warning. `serve_until` waits for it after the connections close, inside the same `server.shutdown_grace`. A server that owns the listener awaits it itself before it exits.
* `Host::begin_drain(&self)` turns `server.ready` to 503 `shutting down` for the rest of the process and ends the event streams the host holds open, `/_sf/live` and `/__fsr/events`, so a page left open does not hold the shutdown for `server.shutdown_grace`; `Host::draining(&self) -> bool` says whether it has been called. `serve_until` calls it once its shutdown resolves. A server that owns the listener, an axum router the host is nested in or an actix `HttpServer`, calls it before it stops, so a balancer stops routing to the instance while open requests finish. `server.health` keeps answering 200.
* `Host::reload_tls(&self) -> Result<(), HostError>`, the `tls` feature: re-reads the certificate and its key and swaps what the next handshake presents. Connections already up keep the certificate they started on and a file that will not read leaves the running one in place, so a failed reload never takes the listener down. `serve_listener` calls it on the configured signal; nothing happens without `[server.tls]`.

### actix

Behind the `actix` feature.

* `actix::handle(req: HttpRequest, host: Data<Arc<Host>>, body: Bytes) -> HttpResponse`: maps the request onto `http::Request<Bytes>`, the response's status, headers and body stream back.
* `actix::serve(host: Arc<Host>, addr: (&str, u16)) -> std::io::Result<()>`: serves until SIGTERM or SIGINT, then calls `begin_drain` and stops the server with `server.shutdown_grace` (at least a second) for open requests. actix's own signal handling is off.
* An `App` that mounts `handle` in an `HttpServer` of its own calls `host.begin_drain()` before it stops the server.

## 5. Observing a Request

`snapfire_fsr_host::trace`. Collection is `fibre_tracing`; this module installs it, hands the host the handle and turns a trace into what a route or a header carries. Re-exports `Trace`, `Span`, `TraceLayer` and `Traces`.

### Installing

Each returns `None` when a global subscriber is already set, which is not an error: something else owns the dispatcher and nothing is collected.

* `pub fn layer() -> (TraceLayer, Traces)`: the collector as a layer and its handle, setting nothing global. For an application whose framework owns the subscriber and composes extra layers into it.
* `pub fn install() -> Option<Traces>`: the collector alone, set as the global subscriber.
* `pub fn install_with<L>(other: L) -> Option<Traces>` where `L: Layer<Registry> + Send + Sync + 'static`: the collector composed beside a layer already handling the events. Sets the subscriber only, never the `log` bridge.
* `pub fn observe(config: &Path) -> (Option<Traces>, Option<fibre_logging::InitResult>, Option<String>)`: `fibre_logging` from `config` and the collector, on one registry. The `InitResult` must be held, since its `Drop` flushes the appenders. A configuration that cannot be read is not fatal: the collector is installed alone and the third field says why.

### Reading

* `pub fn to_value(traces: &[Trace]) -> Value`: one entry per trace with `id`, `ms` and `spans`; each span carries `name`, `depth`, `at`, `ms`, its `fields` and `outcome` when it has one. Durations are milliseconds to three decimal places. Fields beginning `fibre.` are left out, since they are the collector's own.
* `pub fn one_value(trace: &Trace) -> Value`: the single entry `to_value` writes for one trace, what `/__fsr/trace/{token}` answers.
* `pub fn tail_script(trace: &Trace) -> String`: `<script type="application/json" id="sf-trace">` holding `one_value` of `trace` as JSON with `<` escaped, what a page on a `trace.inline` path ends with.
* `pub fn server_timing(trace: &Trace) -> String`: a `Server-Timing` value, one entry per span below the root, described by the span's `id`, `module` or `method` when it has one.

### The Spans

Opened by the framework, all on target `fsr::trace`.

| Span | Where | Fields |
| --- | --- | --- |
| `request` | `Host::handle`, the root of every trace | `method`, `path`, `status`, `fibre.outcome` of `ok` or `error` |
| `session` | `Host::handle`, opening the session before anything reads it and saving it before the response leaves | `op` of `open` or `save` |
| `middleware` | `Host::handle`, the application's and a mounted site's middleware for a page or action request | none |
| `match` | resolving a path to its route's plan | `pattern`, the route the path matched |
| `source` | per plan node, in the assembler's parallel load | `id`, `node`, `fibre.outcome` of `ok` or `failed` |
| `render` | per plan node, nested as the plan nests | `module`, `cache` of `hit` or `miss` when the node is memoized |
| `call` | `TraceInterceptor`, so every transport | `service`, `method`, `cache` of `hit`, `miss` or `none`, `fibre.outcome` being the failure kind or `ok` |

With no collector installed each is a relaxed atomic load and a branch.

## 6. The Shell

### DocumentShell

* `pub struct shell::DocumentShell { pub slot_order: Vec<String> }`, `Default`, an `Evaluator` emitting `<!doctype html><html lang="<locale>" data-sf-locale="<locale>"><head>`, with `data-sf-slot-order="<json array>"` on the element when `slot_order` is not empty, the `head` slot, `</head><body><div id="app">`, the `content` slot, `</div></body></html>`. `lang` is the `locale` prop in BCP 47 spelling, `fr-FR` for `fr_FR`; `data-sf-locale` is the prop as written; `en` for both without the prop.

### head

* `pub fn shell::dev_script(bundle: &str, styles: &[String], nonce: Option<&str>) -> String`: the `<script>` a development document carries, with `bundle` as the id it was rendered against, `styles` the document's stylesheets it was rendered with and `nonce` the one both policies name, since the script's own text changes per request and has no stable hash. It opens `EventSource("/__fsr/events")`; an event whose `bundle` differs reloads, the first event after a connect is otherwise ignored and any later one whose `styles` adds or removes one sheet against the list the page holds links or unlinks it in place, reloads on any other change to the list, re-links every stylesheet with a `__sf` query string and calls `window.__sf.refresh` or reloads when nothing is registered there.
* `pub fn shell::head(title: &str, styles: &[String], import_map: Option<&str>, preload: &[String], entry: Option<&str>) -> snapfire_fsr_runtime::Head`: a head whose default title is `title` and whose `rest` is `<meta charset>`, a viewport meta, a `<link rel="stylesheet">` per style, the import map inlined verbatim as `<script type="importmap">`, a `<link rel="modulepreload">` per `preload` href, the entry as `<script type="module" src>`. The preload links sit after the import map, since a bare specifier in one resolves through the other.
* `pub fn shell::preload_link(href: &str) -> String`: one `<link rel="modulepreload">`, which a mounted site adds to a document on its own routes for a module of its own bundle the shell's links do not already cover.

### canonical

* `pub fn shell::canonical(path: &str) -> String`: `<link rel="canonical" href="<path>">`, which a prefixed request for the default locale carries in its head.

## 7. Conformance

Module `conformance`: the requests a hostile page or client sends, against every route a host answers before an application's pages, each answer judged by its route's class. An application runs it in a test against the host it builds, with its own state-changing routes added to the framework's.

```rust
use snapfire_fsr_host::conformance::{self, Route};

let mut routes = conformance::framework_routes(&host);
routes.push(Route::action("cart.addToCart", r#"{"product_id":1,"quantity":1}"#));
routes.push(Route::handler("/api/cart", r#"{"product_id":1,"quantity":1}"#));
let breaches = conformance::check(&host, &routes).await;
assert!(breaches.is_empty(), "{breaches:?}");
```

`conformance::AUTHORITY`, `app.example`, is the `Host` every probe names and so the origin a page of this site sends.

### Class

* `BrowserWrite`: changes state for the visitor whose session it carries; refused from another origin, from a sibling subdomain and with a `text/plain` body or no content type.
* `HandlerWrite`: a route handler's write; refused from another origin and a sibling subdomain. It is taken from this site's origin with the `text/plain` body a string `fetch` sends.
* `DevControl`: called by `fsr dev` alone, which sends no `Origin`; refused whenever a request carries one.
* `CrossOrigin`: called from another origin by design, `POST /auth/callback`.
* `Read`: changes nothing.

### Route

`Route { method, path, class, body, content_type }`, the body and content type a request this site's own pages send. `Debug`, `Clone`.

* `Route::read(path)`: `GET`.
* `Route::write(path, json)`: a `BrowserWrite` sent as `application/json`.
* `Route::form(path, fields)`: a `BrowserWrite` sent as a form; its own-origin probe is not judged, since the probe cannot forge its token.
* `Route::action(id, json)`, `Route::island(module, json)`: `/_sf/action/<id>` and `/_sf/island/<module>`, `#` percent-encoded.
* `Route::handler(path, json)`: a `HandlerWrite` answering `POST`.

### Breach

`Breach { method, path, probe, status, wanted }`, an answer that broke its route's contract. `Display` prints `POST /api/cart with a foreign Origin: 200, wanted a 4xx refusal`.

### framework_routes and check

* `pub fn framework_routes(host: &Host) -> Vec<Route>`: `/__fsr/sites`, `/_sf/live`, `/__fsr/trace/<token>` and the client modules; with `dev` on, `/__fsr/traces`, `/__fsr/events` and the three control routes; with `[auth]`, `/auth/login`, `/auth/logout` and `/auth/callback`.
* `pub async fn check(host: &Host, routes: &[Route]) -> Vec<Breach>`: sends each route a foreign `Origin`, a sibling subdomain's, this site's, HTTP/1.0 and no `Host`; a write also a `text/plain` body, no content type, a malformed, an empty and an oversized body; a read also a `POST`. A socket upgrade from another origin is sent once. No probe may be answered with a 5xx.

## 8. Error Handling

### HostError

* `Io(PathBuf, std::io::Error)`
* `NoConfig(PathBuf)`
* `Config(PathBuf, String)`, the source and the loading, deserialising or unknown-key message.
* `Value(String, String)`, a setting and the value that did not parse.
* `Bind(BindError)`, transparent.
* `Import { document: String, error: ImportError }`
* `Transport(String, String)`, the client name and why its transport could not be built.
* `Service(String, String)`, the name of a `service` whose contract disagrees with the contracts directory and the duplicate the merge refused.
* `Contract(PathBuf, String)`, a contract file that did not parse or defines a type or service an earlier file already defined.
* `NotFound(String)`
* `NoSlot(String)`: a `Fragment` named a slot the route does not have; `handle` answers it 404 with `no slot named \`<name>\` on this route`.
* `Assemble(AssembleError)`, transparent.
* `Mount(String, String)`: a site could not be mounted, the site's name and why.
* `Probe(String)`: a checked reload's candidate was refused by a probe, `reload refused by a probe: <which>`; `source <id>: <kind> <message>` for a warm load, `<path>: the page failed with <kind>` for one of the host's own renders, `site <name>, <path>[ as <subject>]: ...` for a `[site] probe` entry and a `reload_check`'s own reason for one of those.
* `Uncovered(String)`: the plan names a template module no evaluator answers: build with the `tera` feature or register one with `HostBuilder::evaluator`.
* `TemplateMissing(String)`: the plan names a template module the stock evaluator answers and no template of that name is under the app.
* `Template(PathBuf, String)`: a template under the app did not parse, with Tera's message.
* `Sitemap(String)`: `refresh_sitemap` could not build the sitemap, with why; the one held is kept.
* `Paths(String, String)`: a route's `paths` failed during `prerender`, the pattern and why: the body failed or returned something other than a list of objects or a set left a parameter of the pattern unnamed or empty.
* `Leak(String)`: the bundle under `dist/` carries a server module. `build` reads `dist/.snapfire-build.json` when it exists, takes every module the plan's sources, actions and handlers name plus `middleware.ts` as server-only, maps each to its output by the `app/<path>.ts` to `dist/<path>.js` convention and refuses when an output is one of them or, by the manifest's `graph`, imports one; the message lists each with its reason.
